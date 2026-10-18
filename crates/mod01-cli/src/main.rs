use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read as _, Write as _},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode, Stdio},
    thread,
    time::Duration,
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use modl_core::{
    AssetCatalog, AssetKind, CartridgePngMetadata, CompileMode, DecodedCartridge, Diagnostic,
    FileId, GeneratedProgram, ProjectManifest, SourceFile, WorkcartTest, WorkcartTestKind,
    analyze_module, compile, compile_project, decode_cartridge, decode_cartridge_png,
    encode_cartridge_png, encode_workcart, export_itch_workcart, export_itch_zip,
    export_standalone_html, export_standalone_workcart, format_source, load_cartridge_program,
    materialize_workcart_recipe, pack_project, pack_workcart, parse_project_manifest,
    parse_workcart, unpack_cartridge_project, unpack_workcart,
};

const HEADLESS_HOST: &str = include_str!("../../../packages/runtime/standalone/headless-host.mjs");
const MAX_TRACE_BYTES: u64 = 8 * 1024 * 1024;

mod lsp;

#[derive(Debug, Parser)]
#[command(
    name = "mod01",
    version,
    about = "MOD-01 Color Development Unit toolchain",
    subcommand_required = true,
    arg_required_else_help = true
)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug)]
struct LoadedProject {
    input: ProjectInput,
    manifest_source: String,
    manifest: ProjectManifest,
    files: BTreeMap<String, Vec<u8>>,
    workcart_tests: Vec<WorkcartTest>,
}

#[derive(Debug)]
enum ProjectInput {
    Directory,
    Workcart(String),
}

impl LoadedProject {
    fn pack(&self) -> Result<modl_core::PackedCartridge, modl_core::CartridgeError> {
        match &self.input {
            ProjectInput::Directory => pack_project(&self.manifest_source, &self.files),
            ProjectInput::Workcart(source) => pack_workcart(source),
        }
    }

    fn standalone_html(&self) -> Result<String, modl_core::CartridgeError> {
        match &self.input {
            ProjectInput::Directory => export_standalone_html(&self.manifest_source, &self.files),
            ProjectInput::Workcart(source) => export_standalone_workcart(source),
        }
    }

    fn itch_zip(&self) -> Result<Vec<u8>, modl_core::CartridgeError> {
        match &self.input {
            ProjectInput::Directory => export_itch_zip(&self.manifest_source, &self.files),
            ProjectInput::Workcart(source) => export_itch_workcart(source),
        }
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new Git-friendly cartridge project without overwriting an existing path.
    New {
        path: PathBuf,
        #[arg(long)]
        title: Option<String>,
        #[arg(long, default_value = "@gongahkia")]
        author: String,
    },
    /// Convert one V1 project directory into a source-visible M01W/2 work-cart.
    Migrate {
        path: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Materialize a deterministic author-time asset recipe in an M01W/2 work-cart.
    Generate {
        #[command(subcommand)]
        command: GenerateCommand,
    },
    /// Compile one MODL/1 source module to JavaScript and a source map.
    Build {
        path: PathBuf,
        /// Emit debug probes and suspendable source-debug metadata.
        #[arg(long)]
        debug: bool,
        /// Generated JavaScript path. Defaults to the input path with a `.js` extension.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Check MODL/1 source modules without producing output.
    Check { paths: Vec<PathBuf> },
    /// Format MODL/1 source modules in place.
    Fmt {
        /// Report files that differ without writing them.
        #[arg(long)]
        check: bool,
        paths: Vec<PathBuf>,
    },
    /// Pack a project directory into one deterministic `.m01c` artifact.
    Pack {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Run ordinary MODL tests, expected compiler failures, and scripted frame snapshots.
    Test {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Export a project in a redistributable format.
    Export {
        #[command(subcommand)]
        command: ExportCommand,
    },
    /// Export and open a project in the system browser.
    Run {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Write and validate the player without opening a browser.
        #[arg(long)]
        no_open: bool,
        /// Execute deterministically without a browser and emit a revisioned JSON trace.
        #[arg(long)]
        headless: bool,
        /// Number of display frames for a headless run.
        #[arg(long, default_value_t = 60)]
        frames: u32,
        /// Unsigned 32-bit deterministic seed for a headless run.
        #[arg(long, default_value_t = 604_772_761)]
        seed: u32,
        /// Optional revision-1 scripted controller trace JSON.
        #[arg(long)]
        input: Option<PathBuf>,
        /// Optional raw save image of at most 8 KiB.
        #[arg(long)]
        save: Option<PathBuf>,
    },
    /// Repack a project whenever its files change.
    Watch {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Build once and exit; useful for editor and CI integration checks.
        #[arg(long)]
        once: bool,
        /// Serve the refreshing player without opening the system browser.
        #[arg(long)]
        no_open: bool,
    },
    /// Print toolchain revisions, a project manifest, or packed-cartridge metadata.
    Info { path: Option<PathBuf> },
    /// Run the MODL/1 language server over standard input/output.
    Lsp,
}

#[derive(Debug, Subcommand)]
enum ExportCommand {
    /// Bundle a game with the native player, Node runtime and all source-visible assets.
    Native(NativeExportArguments),
    /// Export one offline, source-inspectable standalone HTML player.
    Html {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Export a MOD-01 cartridge-object PNG containing the complete canonical `.m01c`.
    Png {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Export an itch.io-ready ZIP containing the offline player as `index.html`.
    Zip {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Args)]
struct NativeExportArguments {
    path: PathBuf,
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Native player binary for the selected platform (defaults to sibling mod01-studio).
    #[arg(long)]
    player: Option<PathBuf>,
    #[arg(long)]
    node: Option<PathBuf>,
    #[arg(long)]
    host: Option<PathBuf>,
    #[arg(long)]
    node_license: Option<PathBuf>,
    /// Cross-platform exports require matching player and Node binaries; no cross-compilation is performed.
    #[arg(long, value_enum)]
    platform: Option<NativeTarget>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum NativeTarget {
    Linux,
    Windows,
    Macos,
}

fn export_native_directory(arguments: &NativeExportArguments) -> ExitCode {
    let result = (|| -> Result<PathBuf, String> {
        let (rom, cart, _) = load_headless_cartridge(&arguments.path)
            .map_err(|()| "cannot load native cartridge".to_owned())?;
        let mut tools = modl_core::NativeToolchain::discover(
            arguments.player.as_deref(),
            arguments.node.as_deref(),
            arguments.host.as_deref(),
            arguments.node_license.as_deref(),
        )
        .map_err(|error| error.to_string())?;
        if let Some(platform) = arguments.platform {
            tools.platform = match platform {
                NativeTarget::Linux => modl_core::NativePlatform::Linux,
                NativeTarget::Windows => modl_core::NativePlatform::Windows,
                NativeTarget::Macos => modl_core::NativePlatform::Macos,
            };
        }
        let output = arguments.output.clone().unwrap_or_else(|| {
            project_output_directory(&arguments.path).join(format!("{}-native", cart.manifest.id))
        });
        modl_core::export_native_game(&output, &rom, &tools).map_err(|error| error.to_string())
    })();
    match result {
        Ok(executable) => {
            println!("exported native game {}", executable.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug, Subcommand)]
enum GenerateCommand {
    /// Generate the map named by one stored Noise or WFC recipe.
    Map {
        path: PathBuf,
        recipe: String,
        /// Write a new work-cart instead of replacing the input atomically.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match Arguments::parse().command {
        Command::New {
            path,
            title,
            author,
        } => new_project(&path, title.as_deref(), &author),
        Command::Migrate { path, output } => migrate_project(&path, output.as_deref()),
        Command::Generate {
            command:
                GenerateCommand::Map {
                    path,
                    recipe,
                    output,
                },
        } => generate_map(&path, &recipe, output.as_deref()),
        Command::Build {
            path,
            debug,
            output,
        } => build_file(&path, output.as_deref(), debug),
        Command::Check { paths } => check_files(&paths),
        Command::Fmt { check, paths } => format_files(&paths, check),
        Command::Pack { path, output } => pack_directory(&path, output.as_deref()),
        Command::Test { path } => test_directory(&path),
        Command::Export {
            command: ExportCommand::Native(arguments),
        } => export_native_directory(&arguments),
        Command::Export {
            command: ExportCommand::Html { path, output },
        } => export_html_directory(&path, output.as_deref())
            .map_or(ExitCode::FAILURE, |_| ExitCode::SUCCESS),
        Command::Export {
            command: ExportCommand::Png { path, output },
        } => export_png_directory(&path, output.as_deref())
            .map_or(ExitCode::FAILURE, |_| ExitCode::SUCCESS),
        Command::Export {
            command: ExportCommand::Zip { path, output },
        } => export_zip_directory(&path, output.as_deref())
            .map_or(ExitCode::FAILURE, |_| ExitCode::SUCCESS),
        Command::Run {
            path,
            output,
            no_open,
            headless,
            frames,
            seed,
            input,
            save,
        } => {
            if headless {
                run_headless(
                    &path,
                    output.as_deref(),
                    frames,
                    seed,
                    input.as_deref(),
                    save.as_deref(),
                )
            } else {
                run_directory(&path, output.as_deref(), no_open)
            }
        }
        Command::Watch {
            path,
            output,
            once,
            no_open,
        } => watch_directory(&path, output.as_deref(), once, no_open),
        Command::Info { path } => info(path.as_deref()),
        Command::Lsp => lsp::run(),
    }
}

fn export_html_directory(path: &Path, output: Option<&Path>) -> Result<PathBuf, ()> {
    let project = load_project(path)?;
    let html = project.standalone_html().map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    let output = output.map_or_else(
        || project_output_directory(path).join(format!("{}.html", project.manifest.id)),
        Path::to_path_buf,
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            eprintln!("{}: {error}", parent.display());
        })?;
    }
    fs::write(&output, html.as_bytes()).map_err(|error| {
        eprintln!("{}: {error}", output.display());
    })?;
    println!("exported {} ({} bytes)", output.display(), html.len());
    Ok(output)
}

fn export_png_directory(path: &Path, output: Option<&Path>) -> Result<PathBuf, ()> {
    let project = load_project(path)?;
    let packed = project.pack().map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    let identity = project
        .files
        .get("presentation/cartridge.json")
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok());
    let year = identity
        .as_ref()
        .and_then(|value| value.get("year"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| (1970..=9999).contains(value))
        .unwrap_or(1999);
    let players = identity
        .as_ref()
        .and_then(|value| value.get("players"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .filter(|value| (1..=4).contains(value))
        .unwrap_or(1);
    let controls = identity
        .as_ref()
        .and_then(|value| value.get("controls"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| value.len() <= 64)
        .unwrap_or("PAD")
        .to_owned();
    let metadata = CartridgePngMetadata {
        title: project.manifest.title.clone(),
        author: project.manifest.author.clone(),
        year,
        players,
        controls,
    };
    let png = encode_cartridge_png(&packed.bytes, &metadata).map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    let output = output.map_or_else(
        || project_output_directory(path).join(format!("{}.m01c.png", project.manifest.id)),
        Path::to_path_buf,
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| eprintln!("{}: {error}", parent.display()))?;
    }
    fs::write(&output, &png).map_err(|error| eprintln!("{}: {error}", output.display()))?;
    println!("exported {} ({} bytes)", output.display(), png.len());
    Ok(output)
}

fn export_zip_directory(path: &Path, output: Option<&Path>) -> Result<PathBuf, ()> {
    let project = load_project(path)?;
    let zip = project.itch_zip().map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    let output = output.map_or_else(
        || project_output_directory(path).join(format!("{}-itch.zip", project.manifest.id)),
        Path::to_path_buf,
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| eprintln!("{}: {error}", parent.display()))?;
    }
    fs::write(&output, &zip).map_err(|error| eprintln!("{}: {error}", output.display()))?;
    println!("exported {} ({} bytes)", output.display(), zip.len());
    Ok(output)
}

fn run_directory(path: &Path, output: Option<&Path>, no_open: bool) -> ExitCode {
    let Ok(output) = export_html_directory(path, output) else {
        return ExitCode::FAILURE;
    };
    if no_open {
        return ExitCode::SUCCESS;
    }
    match browser_command(&output.display().to_string()).spawn() {
        Ok(_) => {
            println!("opened {}", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!(
                "{}: could not open system browser: {error}",
                output.display()
            );
            ExitCode::FAILURE
        }
    }
}

fn run_headless(
    path: &Path,
    output: Option<&Path>,
    frames: u32,
    seed: u32,
    input: Option<&Path>,
    save: Option<&Path>,
) -> ExitCode {
    let Ok((rom, cartridge, program)) = load_headless_cartridge(path) else {
        return ExitCode::FAILURE;
    };
    let Ok(javascript) = std::str::from_utf8(&program) else {
        eprintln!("{}: compiled program is not UTF-8", path.display());
        return ExitCode::FAILURE;
    };
    let Ok(trace) = read_json_input(input) else {
        return ExitCode::FAILURE;
    };
    let Ok(save_bytes) = read_save_image(save) else {
        return ExitCode::FAILURE;
    };
    let manifest = &cartridge.manifest;
    let request = serde_json::json!({
        "revision": 1,
        "javascript": javascript,
        "manifest": {
            "id": manifest.id,
            "updateRate": manifest.update_rate,
            "display": manifest.display,
            "assets": manifest.assets,
        },
        "entries": cartridge.entries,
        "rom": rom,
        "seed": seed,
        "frames": frames,
        "trace": trace,
        "save": save_bytes,
    });
    let Ok(request) = serde_json::to_vec(&request) else {
        eprintln!("{}: could not encode headless request", path.display());
        return ExitCode::FAILURE;
    };
    let Ok(serialized) = execute_headless_host(&request, frames) else {
        return ExitCode::FAILURE;
    };
    if let Some(output) = output {
        if let Some(parent) = output.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            eprintln!("{}: {error}", parent.display());
            return ExitCode::FAILURE;
        }
        if let Err(error) = fs::write(output, &serialized) {
            eprintln!("{}: {error}", output.display());
            return ExitCode::FAILURE;
        }
        println!(
            "ran {} headlessly for {frames} frames -> {}",
            path.display(),
            output.display()
        );
    } else {
        println!("{}", String::from_utf8_lossy(&serialized));
    }
    ExitCode::SUCCESS
}

fn execute_headless_host(request: &[u8], frames: u32) -> Result<Vec<u8>, ()> {
    let host_path = std::env::temp_dir().join(format!(
        "mod01-headless-{}-{}.mjs",
        std::process::id(),
        frames
    ));
    if let Err(error) = fs::write(&host_path, HEADLESS_HOST) {
        eprintln!("{}: {error}", host_path.display());
        return Err(());
    }
    let node = std::env::var_os("MOD01_NODE").unwrap_or_else(|| "node".into());
    let child = ProcessCommand::new(node)
        .arg(&host_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = child.map_err(|error| {
        let _ = fs::remove_file(&host_path);
        eprintln!("could not start the MOD-01 Node headless host: {error}");
    })?;
    if let Some(mut stdin) = child.stdin.take()
        && let Err(error) = stdin.write_all(request)
    {
        let _ = child.kill();
        let _ = fs::remove_file(&host_path);
        eprintln!("could not send cartridge to headless host: {error}");
        return Err(());
    }
    let result = child.wait_with_output();
    let _ = fs::remove_file(&host_path);
    let result = match result {
        Ok(result) if result.status.success() => result,
        Ok(result) => {
            eprint!("{}", String::from_utf8_lossy(&result.stderr));
            return Err(());
        }
        Err(error) => {
            eprintln!("headless host did not complete: {error}");
            return Err(());
        }
    };
    let parsed: serde_json::Value = match serde_json::from_slice(&result.stdout) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("headless host returned invalid JSON: {error}");
            return Err(());
        }
    };
    Ok(serde_json::to_vec_pretty(&parsed).expect("JSON value always serializes"))
}

fn load_headless_cartridge(
    path: &Path,
) -> Result<(Vec<u8>, modl_core::DecodedCartridge, Vec<u8>), ()> {
    let packed = if path.is_dir() || is_workcart_path(path) {
        let project = load_project(path)?;
        project.pack().map_err(|error| {
            eprintln!("{}: {error}", path.display());
        })?
    } else if path
        .extension()
        .is_some_and(|extension| extension == "m01c" || extension == "png")
    {
        let file_bytes = fs::read(path).map_err(|error| {
            eprintln!("{}: {error}", path.display());
        })?;
        let bytes = if path.extension().is_some_and(|extension| extension == "png") {
            decode_cartridge_png(&file_bytes)
                .map_err(|error| eprintln!("{}: {error}", path.display()))?
                .cartridge
        } else {
            file_bytes
        };
        let decoded = decode_cartridge(&bytes).map_err(|error| {
            eprintln!("{}: {error}", path.display());
        })?;
        if decoded.manifest.format_revision == modl_core::WORKCART_FORMAT_REVISION {
            let source = unpack_workcart(&bytes).map_err(|error| {
                eprintln!("{}: {error}", path.display());
            })?;
            pack_workcart(&source).map_err(|error| {
                eprintln!("{}: {error}", path.display());
            })?
        } else {
            let project = unpack_cartridge_project(&bytes).map_err(|error| {
                eprintln!("{}: {error}", path.display());
            })?;
            pack_project(&project.manifest, &project.files).map_err(|error| {
                eprintln!("{}: {error}", path.display());
            })?
        }
    } else {
        eprintln!(
            "{}: expected a project directory, .m01w, .m01c, or .m01c.png cartridge",
            path.display()
        );
        return Err(());
    };
    let (cartridge, program) = load_cartridge_program(&packed.bytes).map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    Ok((packed.bytes, cartridge, program))
}

fn read_json_input(path: Option<&Path>) -> Result<serde_json::Value, ()> {
    let Some(path) = path else {
        return Ok(serde_json::json!({ "revision": 1, "frames": [] }));
    };
    let metadata = fs::metadata(path).map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    if metadata.len() > MAX_TRACE_BYTES {
        eprintln!("{}: controller trace exceeds 8 MiB", path.display());
        return Err(());
    }
    let source = fs::read(path).map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    serde_json::from_slice(&source).map_err(|error| {
        eprintln!("{}: invalid controller trace JSON: {error}", path.display());
    })
}

fn read_save_image(path: Option<&Path>) -> Result<Vec<u8>, ()> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let bytes = fs::read(path).map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    if bytes.len() > 8 * 1024 {
        eprintln!("{}: save image exceeds 8 KiB", path.display());
        return Err(());
    }
    Ok(bytes)
}

fn test_directory(path: &Path) -> ExitCode {
    let Ok(project) = load_project(path) else {
        return ExitCode::FAILURE;
    };
    if matches!(&project.input, ProjectInput::Workcart(_)) {
        return test_workcart(&project);
    }
    let tests_root = path.join("tests");
    let mut test_paths = Vec::new();
    if tests_root.is_dir()
        && let Err(error) = collect_test_paths(&tests_root, &mut test_paths)
    {
        eprintln!("{}: {error}", tests_root.display());
        return ExitCode::FAILURE;
    }
    test_paths.sort();
    let mut passed = 0_usize;
    let mut failed = 0_usize;
    let main_cartridge = test_paths
        .iter()
        .any(|test| {
            test.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".m01run.json"))
        })
        .then(|| project.pack());
    for test_path in &test_paths {
        let result = if test_path
            .extension()
            .is_some_and(|extension| extension == "modl")
        {
            run_modl_test(path, &project, test_path)
        } else if test_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".m01run.json"))
        {
            match &main_cartridge {
                Some(Ok(cartridge)) => run_scripted_test(path, cartridge, test_path),
                Some(Err(error)) => Err(error.to_string()),
                None => Err("internal test runner error".to_owned()),
            }
        } else {
            continue;
        };
        match result {
            Ok(()) => {
                passed += 1;
                println!("PASS {}", test_path.display());
            }
            Err(error) => {
                failed += 1;
                eprintln!("FAIL {}: {error}", test_path.display());
            }
        }
    }
    println!("test result: {passed} passed; {failed} failed");
    status(failed > 0)
}

fn test_workcart(project: &LoadedProject) -> ExitCode {
    let main_cartridge = project
        .workcart_tests
        .iter()
        .any(|test| test.kind == WorkcartTestKind::Replay)
        .then(|| project.pack());
    let mut passed = 0_usize;
    let mut failed = 0_usize;
    for test in &project.workcart_tests {
        let result = match test.kind {
            WorkcartTestKind::Modl => run_modl_test_payload(project, &test.path, &test.payload),
            WorkcartTestKind::Replay => match &main_cartridge {
                Some(Ok(cartridge)) => {
                    run_scripted_test_payload(None, cartridge, test.payload.as_bytes())
                }
                Some(Err(error)) => Err(error.to_string()),
                None => Err("internal test runner error".to_owned()),
            },
        };
        match result {
            Ok(()) => {
                passed += 1;
                println!("PASS {}", test.name);
            }
            Err(error) => {
                failed += 1;
                eprintln!("FAIL {}: {error}", test.name);
            }
        }
    }
    println!("test result: {passed} passed; {failed} failed");
    status(failed > 0)
}

fn collect_test_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_test_paths(&entry.path(), paths)?;
        } else {
            paths.push(entry.path());
        }
    }
    Ok(())
}

fn run_modl_test(root: &Path, project: &LoadedProject, test_path: &Path) -> Result<(), String> {
    let relative = test_path
        .strip_prefix(root)
        .map_err(|error| error.to_string())?
        .to_str()
        .ok_or_else(|| "test path is not valid UTF-8".to_owned())?;
    let source = fs::read_to_string(test_path).map_err(|error| error.to_string())?;
    run_modl_test_payload(project, relative, &source)
}

fn run_modl_test_payload(
    project: &LoadedProject,
    relative: &str,
    source: &str,
) -> Result<(), String> {
    let compile_fail = relative.ends_with(".fail.modl");
    let expected_code = source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("// expect ")
            .map(str::trim)
            .filter(|code| code.starts_with("M01"))
    });
    let mut manifest = project.manifest.clone();
    relative.clone_into(&mut manifest.entry);
    manifest.assets.clear();
    manifest.label = None;
    manifest.thumbnail = None;
    manifest.display = None;
    let manifest_source = toml::to_string(&manifest).map_err(|error| error.to_string())?;
    let mut files = project.files.clone();
    files.insert(relative.to_owned(), source.as_bytes().to_vec());
    let compilation = compile_project(&manifest_source, &files, CompileMode::Debug);
    if compile_fail {
        let code = match compilation {
            Err(error) => Some(error.code.to_owned()),
            Ok(output) => output
                .analysis
                .diagnostics
                .first()
                .map(|diagnostic| diagnostic.code.clone()),
        };
        return match (expected_code, code) {
            (Some(expected), Some(actual)) if expected == actual => Ok(()),
            (Some(expected), Some(actual)) => {
                Err(format!("expected {expected}, compiler returned {actual}"))
            }
            (Some(expected), None) => {
                Err(format!("expected {expected}, but compilation succeeded"))
            }
            (None, _) => Err("compile-fail test needs `// expect M01....`".to_owned()),
        };
    }
    let compilation = compilation.map_err(|error| error.to_string())?;
    if let Some(diagnostic) = compilation.analysis.diagnostics.first() {
        return Err(format!("error[{}] {}", diagnostic.code, diagnostic.message));
    }
    let generated = compilation
        .generated
        .ok_or_else(|| "test produced no executable program".to_owned())?;
    let request = serde_json::json!({
        "revision": 1,
        "javascript": generated.javascript,
        "manifest": { "id": project.manifest.id, "updateRate": project.manifest.update_rate,
            "display": null, "assets": {} },
        "entries": {}, "rom": [0], "seed": 0x4d30_3031_u32, "frames": 1,
        "trace": { "revision": 1, "frames": [] }, "save": [],
    });
    let result = execute_headless_host(
        &serde_json::to_vec(&request).map_err(|error| error.to_string())?,
        1,
    )
    .map_err(|()| "headless test host failed".to_owned())?;
    let result: serde_json::Value =
        serde_json::from_slice(&result).map_err(|error| error.to_string())?;
    if let Some(fault) = result.get("fault") {
        return Err(format!(
            "error[{}] {} at {}..{}",
            fault["code"].as_str().unwrap_or("M01????"),
            fault["message"].as_str().unwrap_or("runtime fault"),
            fault["sourceSpan"]["start"].as_u64().unwrap_or_default(),
            fault["sourceSpan"]["end"].as_u64().unwrap_or_default()
        ));
    }
    Ok(())
}

fn run_scripted_test(
    root: &Path,
    packed: &modl_core::PackedCartridge,
    test_path: &Path,
) -> Result<(), String> {
    let source = fs::read(test_path).map_err(|error| error.to_string())?;
    run_scripted_test_payload(Some(root), packed, &source)
}

fn run_scripted_test_payload(
    root: Option<&Path>,
    packed: &modl_core::PackedCartridge,
    source: &[u8],
) -> Result<(), String> {
    if source.len() > usize::try_from(MAX_TRACE_BYTES).unwrap_or(usize::MAX) {
        return Err("scripted test exceeds 8 MiB".to_owned());
    }
    let spec: serde_json::Value =
        serde_json::from_slice(source).map_err(|error| error.to_string())?;
    if spec.get("revision").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err("scripted test revision must be 1".to_owned());
    }
    let frames = spec
        .get("frames")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| "scripted test needs a u32 `frames`".to_owned())?;
    let seed = spec
        .get("seed")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0x4d30_3031);
    let trace = spec
        .get("input")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({ "revision": 1, "frames": [] }));
    let save = match spec.get("save").and_then(serde_json::Value::as_str) {
        Some(relative) => {
            let root = root.ok_or_else(|| {
                "work-cart replay tests cannot reference external save fixtures".to_owned()
            })?;
            read_save_image(Some(&root.join(relative)))
                .map_err(|()| "could not load scripted save fixture".to_owned())?
        }
        None => Vec::new(),
    };
    let (cartridge, program) =
        load_cartridge_program(&packed.bytes).map_err(|error| error.to_string())?;
    let javascript = std::str::from_utf8(&program)
        .map_err(|_| "packed project has no UTF-8 program".to_owned())?;
    let manifest = &cartridge.manifest;
    let request = serde_json::json!({
        "revision": 1, "javascript": javascript,
        "manifest": { "id": manifest.id, "updateRate": manifest.update_rate,
            "display": manifest.display, "assets": manifest.assets },
        "entries": cartridge.entries, "rom": packed.bytes, "seed": seed, "frames": frames,
        "trace": trace, "save": save,
    });
    let result = execute_headless_host(
        &serde_json::to_vec(&request).map_err(|error| error.to_string())?,
        frames,
    )
    .map_err(|()| "headless scripted test host failed".to_owned())?;
    let result: serde_json::Value =
        serde_json::from_slice(&result).map_err(|error| error.to_string())?;
    if let Some(fault) = result.get("fault") {
        return Err(format!(
            "runtime error[{}] {}",
            fault["code"].as_str().unwrap_or("M01????"),
            fault["message"].as_str().unwrap_or("runtime fault")
        ));
    }
    let expected = spec
        .get("expect")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "scripted test needs an `expect` object".to_owned())?;
    let summary = result
        .get("summary")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "headless host returned no summary".to_owned())?;
    for (key, expected_value) in expected {
        if summary.get(key) != Some(expected_value) {
            return Err(format!(
                "snapshot `{key}` expected {expected_value}, got {}",
                summary.get(key).unwrap_or(&serde_json::Value::Null)
            ));
        }
    }
    Ok(())
}

fn new_project(path: &Path, title: Option<&str>, author: &str) -> ExitCode {
    if path.exists() {
        eprintln!("{}: refusing to overwrite an existing path", path.display());
        return ExitCode::FAILURE;
    }
    let id = project_id(path);
    let title = title.map_or_else(|| project_title(&id), ToOwned::to_owned);
    let source_directory = path.join("src");
    if let Err(error) = fs::create_dir_all(&source_directory) {
        eprintln!("{}: {error}", source_directory.display());
        return ExitCode::FAILURE;
    }
    let manifest = format!(
        "format = 1\nlanguage = \"MODL/1\"\nid = \"{id}\"\ntitle = {}\nauthor = {}\nversion = \"0.1.0\"\nentry = \"src/main.modl\"\nupdate_rate = 60\n\n[assets]\n",
        toml_string(&title),
        toml_string(author)
    );
    let source =
        "// Made by @gongahkia\n\non draw:\n  clear(0)\n  print(\"HELLO, MOD-01\", 72, 68, 7)\n";
    for (target, contents) in [
        (path.join("cart.toml"), manifest.as_bytes()),
        (source_directory.join("main.modl"), source.as_bytes()),
    ] {
        if let Err(error) = fs::write(&target, contents) {
            eprintln!("{}: {error}", target.display());
            return ExitCode::FAILURE;
        }
    }
    println!("created {}", path.display());
    ExitCode::SUCCESS
}

fn migrate_project(path: &Path, output: Option<&Path>) -> ExitCode {
    if is_workcart_path(path) || !path.is_dir() {
        eprintln!(
            "{}: migrate expects a V1 cartridge project directory",
            path.display()
        );
        return ExitCode::FAILURE;
    }
    let Ok(project) = load_project(path) else {
        return ExitCode::FAILURE;
    };
    let mut test_paths = Vec::new();
    let tests_root = path.join("tests");
    if tests_root.is_dir()
        && let Err(error) = collect_test_paths(&tests_root, &mut test_paths)
    {
        eprintln!("{}: {error}", tests_root.display());
        return ExitCode::FAILURE;
    }
    test_paths.sort();
    let mut tests = Vec::new();
    for test_path in test_paths {
        let kind = if test_path
            .extension()
            .is_some_and(|extension| extension == "modl")
        {
            WorkcartTestKind::Modl
        } else if test_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".m01run.json"))
        {
            WorkcartTestKind::Replay
        } else {
            continue;
        };
        let relative =
            if let Some(relative) = test_path.strip_prefix(path).ok().and_then(Path::to_str) {
                relative.to_owned()
            } else {
                eprintln!("{}: test path is not valid UTF-8", test_path.display());
                return ExitCode::FAILURE;
            };
        let payload = match fs::read_to_string(&test_path) {
            Ok(payload) => payload,
            Err(error) => {
                eprintln!("{}: {error}", test_path.display());
                return ExitCode::FAILURE;
            }
        };
        tests.push(WorkcartTest {
            name: relative.clone(),
            path: relative,
            kind,
            payload,
        });
    }
    let source = match encode_workcart(&project.manifest, &project.files, &[], &tests) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let output = output.map_or_else(|| path.with_extension("m01w"), Path::to_path_buf);
    if output.exists() {
        eprintln!(
            "{}: refusing to overwrite an existing path",
            output.display()
        );
        return ExitCode::FAILURE;
    }
    if let Some(parent) = output.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!("{}: {error}", parent.display());
        return ExitCode::FAILURE;
    }
    if let Err(error) = fs::write(&output, source) {
        eprintln!("{}: {error}", output.display());
        return ExitCode::FAILURE;
    }
    println!("migrated {} -> {}", path.display(), output.display());
    ExitCode::SUCCESS
}

fn generate_map(path: &Path, recipe: &str, output: Option<&Path>) -> ExitCode {
    if !is_workcart_path(path) {
        eprintln!(
            "{}: generate map expects an .m01w work-cart",
            path.display()
        );
        return ExitCode::FAILURE;
    }
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let generated = match materialize_workcart_recipe(&source, recipe) {
        Ok(generated) => generated,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let output = output.map_or_else(|| path.to_path_buf(), Path::to_path_buf);
    if output != path && output.exists() {
        eprintln!(
            "{}: refusing to overwrite an existing path",
            output.display()
        );
        return ExitCode::FAILURE;
    }
    if let Err(error) = write_workcart_atomically(&output, generated.as_bytes()) {
        eprintln!("{}: {error}", output.display());
        return ExitCode::FAILURE;
    }
    println!("generated map '{recipe}' in {}", output.display());
    ExitCode::SUCCESS
}

fn write_workcart_atomically(path: &Path, source: &[u8]) -> Result<(), std::io::Error> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "output name is not valid UTF-8",
            )
        })?;
    for attempt in 0..=100_u16 {
        let temporary = parent.join(format!(
            ".{name}.mod01-{}-{attempt}.tmp",
            std::process::id()
        ));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = file.write_all(source).and_then(|()| file.sync_all());
        drop(file);
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        if let Err(error) = fs::rename(&temporary, path) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        return Ok(());
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not reserve a temporary output path",
    ))
}

fn pack_directory(path: &Path, output: Option<&Path>) -> ExitCode {
    let Ok(project) = load_project(path) else {
        return ExitCode::FAILURE;
    };
    let packed = match project.pack() {
        Ok(packed) => packed,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = decode_cartridge(&packed.bytes) {
        eprintln!("{}: post-pack validation failed: {error}", path.display());
        return ExitCode::FAILURE;
    }
    let output = output.map_or_else(
        || project_output_directory(path).join(format!("{}.m01c", project.manifest.id)),
        Path::to_path_buf,
    );
    if let Some(parent) = output.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!("{}: {error}", parent.display());
        return ExitCode::FAILURE;
    }
    if let Err(error) = fs::write(&output, &packed.bytes) {
        eprintln!("{}: {error}", output.display());
        return ExitCode::FAILURE;
    }
    println!(
        "packed {} ({} bytes, {} files)",
        output.display(),
        packed.bytes.len(),
        packed.manifest.files.len()
    );
    ExitCode::SUCCESS
}

fn watch_directory(path: &Path, output: Option<&Path>, once: bool, no_open: bool) -> ExitCode {
    if once {
        return pack_directory(path, output);
    }
    let output = output.map_or_else(
        || project_output_directory(path).join("watch.html"),
        Path::to_path_buf,
    );
    if export_watch_html(path, &output).is_err() {
        return ExitCode::FAILURE;
    }
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("mod01 watch: could not bind local player: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = listener.set_nonblocking(true) {
        eprintln!("mod01 watch: could not configure local player: {error}");
        return ExitCode::FAILURE;
    }
    let address = listener
        .local_addr()
        .expect("bound listener has an address");
    let url = format!("http://{address}/");
    println!("watching {} at {url}", path.display());
    if !no_open && let Err(error) = browser_command(&url).spawn() {
        eprintln!("mod01 watch: could not open system browser: {error}");
    }
    let mut fingerprint = directory_fingerprint(path).ok();
    let mut revision = 1_u64;
    loop {
        match listener.accept() {
            Ok((stream, _)) => serve_watch_request(stream, &output, revision),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => eprintln!("mod01 watch: local player request failed: {error}"),
        }
        let current = directory_fingerprint(path).ok();
        if current != fingerprint {
            fingerprint = current;
            if export_watch_html(path, &output).is_ok() {
                revision = revision.wrapping_add(1);
                println!("rebuilt revision {revision}");
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn browser_command(target: &str) -> ProcessCommand {
    let mut command = if cfg!(target_os = "macos") {
        ProcessCommand::new("open")
    } else {
        ProcessCommand::new("xdg-open")
    };
    command.arg(target);
    command
}

fn export_watch_html(path: &Path, output: &Path) -> Result<(), ()> {
    let project = load_project(path)?;
    let html = project.standalone_html().map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    let reload = r"<script>(()=>{let revision;setInterval(async()=>{try{const next=await fetch('/revision',{cache:'no-store'}).then(response=>response.text());if(revision!==undefined&&next!==revision)location.reload();revision=next}catch{}},250)})()</script>";
    let html = html.replace("</body>", &format!("{reload}</body>"));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            eprintln!("{}: {error}", parent.display());
        })?;
    }
    fs::write(output, html).map_err(|error| {
        eprintln!("{}: {error}", output.display());
    })
}

fn serve_watch_request(mut stream: TcpStream, output: &Path, revision: u64) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
    let mut request = [0_u8; 4096];
    let length = stream.read(&mut request).unwrap_or_default();
    let revision_request = request[..length].starts_with(b"GET /revision ");
    let (content_type, body) = if revision_request {
        (
            "text/plain; charset=utf-8",
            revision.to_string().into_bytes(),
        )
    } else {
        (
            "text/html; charset=utf-8",
            fs::read(output).unwrap_or_else(|_| b"MOD-01 WATCH BUILD UNAVAILABLE".to_vec()),
        )
    };
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&body);
}

fn directory_fingerprint(path: &Path) -> Result<u64, std::io::Error> {
    let mut files = Vec::new();
    collect_fingerprint_files(path, path, &mut files)?;
    files.sort();
    let mut hasher = DefaultHasher::new();
    for file in files {
        file.strip_prefix(path).unwrap_or(&file).hash(&mut hasher);
        fs::read(file)?.hash(&mut hasher);
    }
    Ok(hasher.finish())
}

fn collect_fingerprint_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), std::io::Error> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = entry.file_name();
            if !matches!(
                name.to_str(),
                Some(".git" | "dist" | "node_modules" | "target")
            ) {
                collect_fingerprint_files(root, &entry.path(), files)?;
            }
        } else if entry.path() != root.join("cart.m01c") {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn info(path: Option<&Path>) -> ExitCode {
    let Some(path) = path else {
        println!(
            "{} cartridge-format/{} compiler/{}",
            modl_core::LANGUAGE_REVISION,
            modl_core::CARTRIDGE_FORMAT_REVISION,
            modl_core::compiler_version()
        );
        return ExitCode::SUCCESS;
    };
    let metadata = cartridge_report(path).and_then(|report| {
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())
    });
    match metadata {
        Ok(metadata) => {
            println!("{metadata}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            ExitCode::FAILURE
        }
    }
}

#[derive(Default)]
struct ReportSectionSizes {
    source: usize,
    map: usize,
    font: usize,
    audio: usize,
    visual: usize,
    metadata: usize,
}

fn report_sections(cartridge: &DecodedCartridge) -> (ReportSectionSizes, Vec<(String, usize)>) {
    let mut sizes = ReportSectionSizes::default();
    let mut source_bytes = 0_usize;
    let mut largest_entries = cartridge
        .entries
        .iter()
        .map(|(name, value)| (name.clone(), value.len()))
        .collect::<Vec<_>>();
    largest_entries.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    largest_entries.truncate(8);
    for (name, value) in &cartridge.entries {
        if name.starts_with("source/") {
            source_bytes += value.len();
        } else if name == "manifest.json" || name.starts_with("presentation/") {
            sizes.metadata += value.len();
        }
    }
    sizes.source = source_bytes;
    for asset in cartridge.manifest.assets.values() {
        let length = cartridge
            .entries
            .get(&asset.path)
            .map_or(0, std::vec::Vec::len);
        match asset.kind {
            AssetKind::Map => {
                sizes.map += length;
                sizes.visual += length;
            }
            AssetKind::Font => {
                sizes.font += length;
                sizes.visual += length;
            }
            AssetKind::Sound | AssetKind::Music => sizes.audio += length,
            AssetKind::Sprite | AssetKind::Animation | AssetKind::TileSet => {
                sizes.visual += length;
            }
        }
    }
    (sizes, largest_entries)
}

fn profile_cartridge(
    bytes: &[u8],
    cartridge: &DecodedCartridge,
    program: &[u8],
) -> Result<serde_json::Value, String> {
    let manifest = &cartridge.manifest;
    let request = serde_json::json!({
        "revision": 1,
        "javascript": std::str::from_utf8(program).map_err(|_| "compiled program is not UTF-8")?,
        "manifest": { "id": manifest.id, "updateRate": manifest.update_rate,
            "display": manifest.display, "assets": manifest.assets },
        "entries": cartridge.entries, "rom": bytes, "seed": 0x4d30_3031_u32,
        "frames": 60, "trace": { "revision": 1, "frames": [] }, "save": [],
    });
    let runtime = execute_headless_host(
        &serde_json::to_vec(&request).map_err(|error| error.to_string())?,
        60,
    )
    .map_err(|()| "headless profile failed".to_owned())?;
    serde_json::from_slice(&runtime).map_err(|error| error.to_string())
}

fn cartridge_report(path: &Path) -> Result<serde_json::Value, String> {
    let (bytes, cartridge, program) = load_headless_cartridge(path)
        .map_err(|()| "could not load cartridge for analysis".to_owned())?;
    let sections = archive_sections(&bytes)?;
    let unpacked = unpack_cartridge_project(&bytes).map_err(|error| error.to_string())?;
    let compilation = compile_project(&unpacked.manifest, &unpacked.files, CompileMode::Release)
        .map_err(|error| error.to_string())?;
    let generated = compilation
        .generated
        .ok_or_else(|| "release compilation produced no program".to_owned())?;
    let (section_sizes, largest_entries) = report_sections(&cartridge);
    let raw_bytes = sections.iter().map(|(_, raw, _)| raw).sum::<usize>();
    let encoded_bytes = sections
        .iter()
        .map(|(_, _, encoded)| encoded)
        .sum::<usize>();
    let container_overhead = bytes.len().saturating_sub(encoded_bytes);
    let mut largest_symbols = compilation
        .analysis
        .ir
        .as_ref()
        .map(|ir| {
            ir.routines
                .iter()
                .map(|routine| {
                    serde_json::json!({
                        "name": routine.name,
                        "sourceBytes": routine.span.end.saturating_sub(routine.span.start),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    largest_symbols
        .sort_by_key(|value| std::cmp::Reverse(value["sourceBytes"].as_u64().unwrap_or_default()));
    largest_symbols.truncate(8);

    let runtime = profile_cartridge(&bytes, &cartridge, &program)?;
    let summary = runtime
        .get("summary")
        .cloned()
        .ok_or_else(|| "headless profile returned no summary".to_owned())?;

    let size_class = match bytes.len() {
        0..=4096 => "4K",
        4097..=16384 => "16K",
        16385..=65536 => "64K",
        _ => "256K",
    };
    let mut warnings = Vec::new();
    if bytes.len() > 196_608 {
        warnings.push("cartridge is above 75% of 256 KiB; inspect largestEntries");
    }
    if section_sizes.visual > 98_304 {
        warnings.push("visual assets are above 75% of 128 KiB; reduce the largest visual asset");
    }
    if summary["workPeak"].as_u64().unwrap_or_default() > 40_000 {
        warnings.push("work peak is above 80% of 50,000; inspect the largest routine/call site");
    }
    if warnings.is_empty() {
        warnings.push("within fixed cartridge, visual, work, command and voice limits");
    }
    let analysis = serde_json::json!({
        "revision": 1,
        "sizeClass": size_class,
        "sections": {
            "canonicalCartridgeBytes": bytes.len(),
            "sourceBytes": section_sizes.source,
            "generatedReleaseBytes": generated.generated_bytes,
            "visualBytes": section_sizes.visual,
            "mapBytes": section_sizes.map,
            "fontBytes": section_sizes.font,
            "audioBytes": section_sizes.audio,
            "metadataBytes": section_sizes.metadata,
            "containerOverheadBytes": container_overhead,
            "rawArchiveBytes": raw_bytes,
            "encodedArchiveBytes": encoded_bytes,
            "compressionGainBytes": i64::try_from(raw_bytes).unwrap_or(i64::MAX)
                - i64::try_from(encoded_bytes).unwrap_or(i64::MAX),
            "saveAllocationBytes": 8192,
        },
        "runtime60Frames": {
            "summary": summary,
            "fault": runtime.get("fault").cloned(),
        },
        "largestEntries": largest_entries.into_iter().map(|(path, bytes)|
            serde_json::json!({ "path": path, "bytes": bytes })).collect::<Vec<_>>(),
        "largestSymbols": largest_symbols,
        "warnings": warnings,
    });
    let mut report = if path.is_dir() {
        serde_json::to_value(
            parse_project_manifest(&unpacked.manifest).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?
    } else {
        serde_json::to_value(&cartridge.manifest).map_err(|error| error.to_string())?
    };
    report
        .as_object_mut()
        .ok_or_else(|| "manifest report is not an object".to_owned())?
        .insert("analysis".to_owned(), analysis);
    Ok(report)
}

fn archive_sections(bytes: &[u8]) -> Result<Vec<(String, usize, usize)>, String> {
    if bytes.len() < 12 {
        return Err("cartridge header is truncated".to_owned());
    }
    let mut cursor = 8_usize;
    let count = u32::from_le_bytes(
        bytes[cursor..cursor + 4]
            .try_into()
            .map_err(|_| "invalid cartridge entry count")?,
    );
    cursor += 4;
    let mut sections = Vec::new();
    for _ in 0..count {
        if cursor + 42 > bytes.len() {
            return Err("cartridge entry header is truncated".to_owned());
        }
        let path_length = usize::from(u16::from_le_bytes(
            bytes[cursor..cursor + 2]
                .try_into()
                .map_err(|_| "invalid path length")?,
        ));
        let raw_length = usize::try_from(u32::from_le_bytes(
            bytes[cursor + 2..cursor + 6]
                .try_into()
                .map_err(|_| "invalid raw length")?,
        ))
        .map_err(|_| "raw length does not fit this host")?;
        let encoded_length = usize::try_from(u32::from_le_bytes(
            bytes[cursor + 6..cursor + 10]
                .try_into()
                .map_err(|_| "invalid encoded length")?,
        ))
        .map_err(|_| "encoded length does not fit this host")?;
        cursor += 42;
        let end = cursor
            .checked_add(path_length)
            .and_then(|value| value.checked_add(encoded_length))
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "cartridge entry payload is truncated".to_owned())?;
        let path = std::str::from_utf8(&bytes[cursor..cursor + path_length])
            .map_err(|_| "cartridge path is not UTF-8")?
            .to_owned();
        sections.push((path, raw_length, encoded_length));
        cursor = end;
    }
    Ok(sections)
}

fn load_project(path: &Path) -> Result<LoadedProject, ()> {
    if is_workcart_path(path) {
        return load_workcart(path);
    }
    if !path.is_dir() {
        eprintln!(
            "{}: expected a cartridge project directory or .m01w work-cart",
            path.display()
        );
        return Err(());
    }
    let manifest_path = path.join("cart.toml");
    let manifest_source = fs::read_to_string(&manifest_path).map_err(|error| {
        eprintln!("{}: {error}", manifest_path.display());
    })?;
    let manifest = parse_project_manifest(&manifest_source).map_err(|error| {
        eprintln!("{}: {error}", manifest_path.display());
    })?;
    let mut files = BTreeMap::new();
    collect_sources(path, path, &mut files)?;
    let required_paths = manifest
        .assets
        .values()
        .map(|asset| asset.path.as_str())
        .chain(manifest.label.as_deref())
        .chain(manifest.thumbnail.as_deref())
        .chain(manifest.display.as_deref());
    for relative in required_paths {
        if files.contains_key(relative) {
            continue;
        }
        let target = path.join(relative);
        let bytes = fs::read(&target).map_err(|error| {
            eprintln!("{}: {error}", target.display());
        })?;
        files.insert(relative.to_owned(), bytes);
    }
    Ok(LoadedProject {
        input: ProjectInput::Directory,
        manifest_source,
        manifest,
        files,
        workcart_tests: Vec::new(),
    })
}

fn load_workcart(path: &Path) -> Result<LoadedProject, ()> {
    let source = fs::read_to_string(path).map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    let project = parse_workcart(&source).map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    let manifest_source = toml::to_string(&project.manifest).map_err(|error| {
        eprintln!(
            "{}: could not materialize work-cart manifest: {error}",
            path.display()
        );
    })?;
    Ok(LoadedProject {
        input: ProjectInput::Workcart(source),
        manifest_source,
        manifest: project.manifest,
        files: project.files,
        workcart_tests: project.tests,
    })
}

fn is_workcart_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "m01w")
}

fn project_output_directory(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("dist")
    } else {
        path.parent().unwrap_or_else(|| Path::new(".")).join("dist")
    }
}

fn collect_sources(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), ()> {
    let entries = fs::read_dir(directory).map_err(|error| {
        eprintln!("{}: {error}", directory.display());
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            eprintln!("{}: {error}", directory.display());
        })?;
        let file_type = entry.file_type().map_err(|error| {
            eprintln!("{}: {error}", entry.path().display());
        })?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = entry.file_name();
            if !matches!(
                name.to_str(),
                Some(".git" | "dist" | "node_modules" | "target")
            ) {
                collect_sources(root, &entry.path(), files)?;
            }
            continue;
        }
        let target = entry.path();
        if target
            .extension()
            .is_none_or(|extension| extension != "modl")
        {
            continue;
        }
        let relative = target.strip_prefix(root).map_err(|error| {
            eprintln!("{}: {error}", target.display());
        })?;
        let Some(relative) = relative.to_str() else {
            eprintln!("{}: source path is not valid UTF-8", target.display());
            return Err(());
        };
        let bytes = fs::read(&target).map_err(|error| {
            eprintln!("{}: {error}", target.display());
        })?;
        files.insert(relative.to_owned(), bytes);
    }
    Ok(())
}

fn project_id(path: &Path) -> String {
    let raw = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("game");
    let mut id = String::new();
    let mut separator = false;
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() {
            id.push(char::from(byte.to_ascii_lowercase()));
            separator = false;
        } else if !separator && !id.is_empty() {
            id.push('-');
            separator = true;
        }
    }
    while id.ends_with('-') {
        id.pop();
    }
    if id.len() < 3 {
        format!("game-{id}")
    } else {
        id
    }
}

fn project_title(id: &str) -> String {
    id.split('-')
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("Rust strings always serialize")
}

fn build_file(path: &Path, output: Option<&Path>, debug: bool) -> ExitCode {
    if path.is_dir() || is_workcart_path(path) {
        return build_directory(path, output, debug);
    }
    let Ok(text) = read_source(path) else {
        return ExitCode::FAILURE;
    };
    let source = SourceFile::new(FileId(0), path.display().to_string(), text);
    let compilation = compile(
        &source,
        &AssetCatalog::default(),
        if debug {
            CompileMode::Debug
        } else {
            CompileMode::Release
        },
    );
    if !compilation.analysis.diagnostics.is_empty() {
        emit_diagnostics(path, &source, &compilation.analysis.diagnostics);
        return ExitCode::FAILURE;
    }
    let Some(generated) = compilation.generated else {
        eprintln!("{}: compiler produced no output", path.display());
        return ExitCode::FAILURE;
    };
    let output = output.map_or_else(|| path.with_extension("js"), Path::to_path_buf);
    write_program(&generated, &output)
}

fn build_directory(path: &Path, output: Option<&Path>, debug: bool) -> ExitCode {
    let Ok(project) = load_project(path) else {
        return ExitCode::FAILURE;
    };
    let compilation = match compile_project(
        &project.manifest_source,
        &project.files,
        if debug {
            CompileMode::Debug
        } else {
            CompileMode::Release
        },
    ) {
        Ok(compilation) => compilation,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    if !compilation.analysis.diagnostics.is_empty() {
        emit_project_diagnostics(path, &compilation.analysis.diagnostics);
        return ExitCode::FAILURE;
    }
    let Some(generated) = compilation.generated else {
        eprintln!("{}: compiler produced no output", path.display());
        return ExitCode::FAILURE;
    };
    let output = output.map_or_else(
        || project_output_directory(path).join("cartridge.js"),
        Path::to_path_buf,
    );
    write_program(&generated, &output)
}

fn write_program(generated: &GeneratedProgram, output: &Path) -> ExitCode {
    let source_map = output.with_extension("js.map");
    if let Some(parent) = output.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!("{}: {error}", parent.display());
        return ExitCode::FAILURE;
    }
    let Some(source_map_name) = source_map.file_name().and_then(|name| name.to_str()) else {
        eprintln!(
            "{}: output source-map name is not valid UTF-8",
            source_map.display()
        );
        return ExitCode::FAILURE;
    };
    let javascript = format!(
        "{}//# sourceMappingURL={}\n",
        generated.javascript, source_map_name
    );
    if let Err(error) = fs::write(output, javascript) {
        eprintln!("{}: {error}", output.display());
        return ExitCode::FAILURE;
    }
    if let Err(error) = fs::write(&source_map, &generated.source_map_json) {
        eprintln!("{}: {error}", source_map.display());
        return ExitCode::FAILURE;
    }
    println!(
        "built {} ({} bytes, {:?})",
        output.display(),
        generated.generated_bytes,
        generated.mode
    );
    ExitCode::SUCCESS
}

fn check_files(paths: &[PathBuf]) -> ExitCode {
    let defaults = [PathBuf::from(".")];
    let paths = if paths.is_empty() {
        &defaults[..]
    } else {
        paths
    };
    let mut failed = false;
    for (index, path) in paths.iter().enumerate() {
        if path.is_dir() || is_workcart_path(path) {
            let Ok(project) = load_project(path) else {
                failed = true;
                continue;
            };
            match compile_project(
                &project.manifest_source,
                &project.files,
                CompileMode::Release,
            ) {
                Ok(output) if output.analysis.diagnostics.is_empty() => match project.pack() {
                    Ok(_) => {}
                    Err(error) => {
                        eprintln!("{}: {error}", path.display());
                        failed = true;
                    }
                },
                Ok(output) => {
                    emit_project_diagnostics(path, &output.analysis.diagnostics);
                    failed = true;
                }
                Err(error) => {
                    eprintln!("{}: {error}", path.display());
                    failed = true;
                }
            }
            continue;
        }
        let Ok(text) = read_source(path) else {
            failed = true;
            continue;
        };
        let source = SourceFile::new(file_id(index), path.display().to_string(), text);
        let output = analyze_module(&source, &AssetCatalog::default());
        if !output.diagnostics.is_empty() {
            emit_diagnostics(path, &source, &output.diagnostics);
            failed = true;
        }
    }
    status(failed)
}

fn emit_project_diagnostics(path: &Path, diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        eprintln!(
            "{}: error[{}]: {}",
            path.display(),
            diagnostic.code,
            diagnostic.message
        );
    }
}

fn format_files(paths: &[PathBuf], check: bool) -> ExitCode {
    let defaults = [PathBuf::from(".")];
    let requested = if paths.is_empty() {
        &defaults[..]
    } else {
        paths
    };
    let mut paths = Vec::new();
    for requested_path in requested {
        if requested_path.is_dir() {
            if let Err(error) = collect_modl_paths(requested_path, &mut paths) {
                eprintln!("{}: {error}", requested_path.display());
                return ExitCode::FAILURE;
            }
        } else {
            paths.push(requested_path.clone());
        }
    }
    paths.sort();
    let mut failed = false;
    for (index, path) in paths.iter().enumerate() {
        let Ok(text) = read_source(path) else {
            failed = true;
            continue;
        };
        let source = SourceFile::new(file_id(index), path.display().to_string(), text.clone());
        match format_source(&source) {
            Ok(formatted) if formatted == text => {}
            Ok(_) if check => {
                eprintln!("{}: requires formatting", path.display());
                failed = true;
            }
            Ok(formatted) => {
                if let Err(error) = fs::write(path, formatted) {
                    eprintln!("{}: {error}", path.display());
                    failed = true;
                }
            }
            Err(error) => {
                emit_diagnostics(path, &source, &error.diagnostics);
                failed = true;
            }
        }
    }
    status(failed)
}

fn collect_modl_paths(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if !matches!(
                entry.file_name().to_str(),
                Some(".git" | "dist" | "node_modules" | "target")
            ) {
                collect_modl_paths(&entry.path(), paths)?;
            }
        } else if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "modl")
        {
            paths.push(entry.path());
        }
    }
    Ok(())
}

fn read_source(path: &Path) -> Result<String, ()> {
    fs::read_to_string(path).map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })
}

fn emit_diagnostics(path: &Path, source: &SourceFile, diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        let position = source.line_column(diagnostic.primary.span.start);
        eprintln!(
            "{}:{}:{}: error[{}]: {}",
            path.display(),
            position.line + 1,
            position.column + 1,
            diagnostic.code,
            diagnostic.message
        );
        if !diagnostic.primary.message.is_empty() {
            eprintln!("  {}", diagnostic.primary.message);
        }
        for note in &diagnostic.notes {
            eprintln!("  note: {note}");
        }
    }
}

fn file_id(index: usize) -> FileId {
    FileId(u32::try_from(index).unwrap_or(u32::MAX))
}

const fn status(failed: bool) -> ExitCode {
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
