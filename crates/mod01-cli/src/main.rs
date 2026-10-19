use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    hash::{DefaultHasher, Hash, Hasher},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode, Stdio},
    thread,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use clap::{Args, Parser, Subcommand, ValueEnum};
use modl_core::{
    AssetCatalog, CompileMode, Diagnostic, FileId, GeneratedProgram, ProjectManifest, SourceFile,
    WorkcartTest, WorkcartTestKind, analyze_module, compile, compile_project, decode_cartridge_png,
    encode_workcart, format_source, materialize_workcart_recipe, parse_project_manifest,
    parse_workcart,
};

const HEADLESS_HOST: &str = include_str!("../../../packages/runtime/standalone/headless-host.mjs");
const MAX_TRACE_BYTES: u64 = 8 * 1024 * 1024;

mod lsp;
mod report;

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
    source: String,
    manifest_source: String,
    manifest: ProjectManifest,
    files: BTreeMap<String, Vec<u8>>,
    workcart_tests: Vec<WorkcartTest>,
}

impl LoadedProject {
    fn pack_game(&self) -> Result<modl_core::game::PackedGame, modl_core::CartridgeError> {
        modl_core::game::pack_game_workcart(&self.source)
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
    /// Materialize a deterministic author-time asset recipe in an M01W/2 work-cart.
    Generate {
        #[command(subcommand)]
        command: GenerateCommand,
    },
    /// Import source art through native Studio's shared conversion and validation.
    Import {
        #[command(subcommand)]
        command: ImportCommand,
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
    /// Inspect shared asset validation, references and exact visual capacity as JSON.
    Assets {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
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
    /// Launch the native game player, or execute the shared core headlessly.
    Run {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Execute deterministically without a window and emit a revisioned JSON trace.
        #[arg(long)]
        headless: bool,
        /// Number of display frames for a headless run.
        #[arg(long, default_value_t = 60)]
        frames: u32,
        /// Unsigned 32-bit deterministic seed for a headless run.
        #[arg(long, default_value_t = modl_core::DEFAULT_SEED)]
        seed: u32,
        /// Optional revision-1 scripted controller trace JSON.
        #[arg(long)]
        input: Option<PathBuf>,
        /// Optional raw save image of at most 8 KiB.
        #[arg(long)]
        save: Option<PathBuf>,
        /// Initial save catalog JSON keyed by cartridge ID with canonical base64 byte images.
        #[arg(long, conflicts_with = "save")]
        saves: Option<PathBuf>,
        /// Initial transfer parameter, at most 1024 UTF-8 bytes (headless only).
        #[arg(long, default_value = "")]
        parameter: String,
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
    },
    /// Print toolchain revisions, a project manifest, or packed-cartridge metadata.
    Info {
        path: Option<PathBuf>,
        #[arg(long)]
        source: bool,
        /// Report admitted shipping/member sizes without running a Node profile.
        #[arg(long = "static", conflicts_with = "source", requires = "path")]
        static_only: bool,
    },
    /// Emit member-scoped compiler definitions, diagnostics and call/state/asset relationships as JSON.
    Graph {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Run the MODL/1 language server over standard input/output.
    Lsp,
}

#[derive(Debug, Subcommand)]
enum ExportCommand {
    /// Bundle a game with the native player, Node runtime and all source-visible assets.
    Native(NativeExportArguments),
    /// Export a MOD-01 cartridge-object PNG containing the complete canonical `.m01c`.
    Png {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PngImportKind {
    Sprite,
    TileSet,
}

#[derive(Debug, Subcommand)]
enum ImportCommand {
    /// Import a PNG as a named asset, atomically updating the input workcart by default.
    Png {
        path: PathBuf,
        name: String,
        png: PathBuf,
        #[arg(long, value_enum, default_value = "sprite")]
        kind: PngImportKind,
        /// Replace an entire existing asset and its editable layered source explicitly.
        #[arg(long)]
        replace: bool,
        /// Write a new workcart instead; existing outputs are never overwritten.
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
        let game = load_game(&arguments.path).map_err(|()| "cannot load native game".to_owned())?;
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
            project_output_directory(&arguments.path).join(format!("{}-native", game.file_stem()))
        });
        modl_core::export_native_game(&output, game.bytes(), &tools)
            .map_err(|error| error.to_string())
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
        Command::Generate {
            command:
                GenerateCommand::Map {
                    path,
                    recipe,
                    output,
                },
        } => generate_map(&path, &recipe, output.as_deref()),
        Command::Import {
            command:
                ImportCommand::Png {
                    path,
                    name,
                    png,
                    kind,
                    replace,
                    output,
                },
        } => import_png(&path, &name, &png, kind, replace, output.as_deref()),
        Command::Build {
            path,
            debug,
            output,
        } => build_file(&path, output.as_deref(), debug),
        Command::Check { paths } => check_files(&paths),
        Command::Assets { path } => asset_report(&path),
        Command::Fmt { check, paths } => format_files(&paths, check),
        Command::Pack { path, output } => pack_directory(&path, output.as_deref()),
        Command::Test { path } => test_directory(&path),
        Command::Export {
            command: ExportCommand::Native(arguments),
        } => export_native_directory(&arguments),
        Command::Export {
            command: ExportCommand::Png { path, output },
        } => export_png_directory(&path, output.as_deref())
            .map_or(ExitCode::FAILURE, |_| ExitCode::SUCCESS),
        Command::Run {
            path,
            output,
            headless,
            frames,
            seed,
            input,
            save,
            saves,
            parameter,
        } => {
            if headless {
                run_headless(
                    &path,
                    &HeadlessRun {
                        output: output.as_deref(),
                        frames,
                        seed,
                        input: input.as_deref(),
                        save: save.as_deref(),
                        saves: saves.as_deref(),
                        parameter: &parameter,
                    },
                )
            } else {
                if output.is_some()
                    || input.is_some()
                    || save.is_some()
                    || saves.is_some()
                    || !parameter.is_empty()
                {
                    eprintln!(
                        "--output, --input, --save, --saves and --parameter require --headless"
                    );
                    return ExitCode::FAILURE;
                }
                run_directory(&path)
            }
        }
        Command::Watch { path, output, once } => watch_directory(&path, output.as_deref(), once),
        Command::Info {
            path,
            source,
            static_only,
        } => info(path.as_deref(), source, static_only),
        Command::Graph { path } => graph_project(&path),
        Command::Lsp => lsp::run(),
    }
}

fn graph_project(path: &Path) -> ExitCode {
    let source = if path.is_dir() || is_workcart_path(path) {
        load_project(path).map(|project| project.source)
    } else {
        load_game(path).map(|game| game.source().to_owned())
    };
    let Ok(source) = source else {
        return ExitCode::FAILURE;
    };
    match modl_core::tooling::analyze_game_model(&source) {
        Ok(model) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&model).expect("compiler model serializes")
            );
            status(
                model
                    .members
                    .values()
                    .any(|member| !member.model.diagnostics.is_empty()),
            )
        }
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            ExitCode::FAILURE
        }
    }
}

fn export_png_directory(path: &Path, output: Option<&Path>) -> Result<PathBuf, ()> {
    let project = load_project(path)?;
    let packed = project.pack_game().map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    let png = modl_core::cartridge_png::encode_game_png(&packed).map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    let output = output.map_or_else(
        || project_output_directory(path).join(format!("{}.m01c.png", packed.file_stem())),
        Path::to_path_buf,
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| eprintln!("{}: {error}", parent.display()))?;
    }
    fs::write(&output, &png).map_err(|error| eprintln!("{}: {error}", output.display()))?;
    println!("exported {} ({} bytes)", output.display(), png.len());
    Ok(output)
}

fn run_directory(path: &Path) -> ExitCode {
    let player = std::env::var_os("MOD01_PLAYER")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|executable| {
                executable.parent().map(|directory| {
                    directory.join(if cfg!(windows) {
                        "mod01-studio.exe"
                    } else {
                        "mod01-studio"
                    })
                })
            })
        });
    let Some(player) = player.filter(|path| path.is_file()) else {
        eprintln!("native player not found; build mod01-native or set MOD01_PLAYER");
        return ExitCode::FAILURE;
    };
    match ProcessCommand::new(player).arg("--play").arg(path).status() {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            eprintln!("native player exited with {status}");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("cannot launch native player: {error}");
            ExitCode::FAILURE
        }
    }
}

struct HeadlessRun<'a> {
    output: Option<&'a Path>,
    frames: u32,
    seed: u32,
    input: Option<&'a Path>,
    save: Option<&'a Path>,
    saves: Option<&'a Path>,
    parameter: &'a str,
}

fn headless_request(
    game: &modl_core::game::PackedGame,
    mode: CompileMode,
    frames: u32,
    seed: u32,
    trace: &serde_json::Value,
    parameter: &str,
    saves: BTreeMap<String, Vec<u8>>,
) -> Result<serde_json::Value, String> {
    if parameter.len() > 1024 {
        return Err("transfer parameter exceeds 1024 UTF-8 bytes".to_owned());
    }
    let runtime = modl_core::runtime_game::prepare_runtime_game(game, mode)
        .map_err(|error| error.to_string())?;
    let saves = saves
        .into_iter()
        .map(|(id, image)| (id, STANDARD.encode(image)))
        .collect::<BTreeMap<_, _>>();
    Ok(
        serde_json::json!({ "revision": 2, "game": runtime, "seed": seed,
        "frames": frames, "trace": trace, "parameter": parameter, "saves": saves }),
    )
}

fn run_headless(path: &Path, options: &HeadlessRun<'_>) -> ExitCode {
    let Ok(game) = load_game(path) else {
        return ExitCode::FAILURE;
    };
    let Ok(trace) = read_json_input(options.input) else {
        return ExitCode::FAILURE;
    };
    let Ok(saves) = read_headless_saves(&game, options.save, options.saves) else {
        return ExitCode::FAILURE;
    };
    let request = match headless_request(
        &game,
        CompileMode::Release,
        options.frames,
        options.seed,
        &trace,
        options.parameter,
        saves,
    ) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let Ok(request) = serde_json::to_vec(&request) else {
        eprintln!("{}: could not encode headless request", path.display());
        return ExitCode::FAILURE;
    };
    let serialized = match execute_headless_host(&request, options.frames) {
        Ok(serialized) => serialized,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(output) = options.output {
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
            "ran {} headlessly for {} frames -> {}",
            path.display(),
            options.frames,
            output.display()
        );
    } else {
        println!("{}", String::from_utf8_lossy(&serialized));
    }
    ExitCode::SUCCESS
}

fn execute_headless_host(request: &[u8], frames: u32) -> Result<Vec<u8>, String> {
    let host_path = std::env::temp_dir().join(format!(
        "mod01-headless-{}-{}.mjs",
        std::process::id(),
        frames
    ));
    fs::write(&host_path, HEADLESS_HOST)
        .map_err(|error| format!("{}: {error}", host_path.display()))?;
    let node = std::env::var_os("MOD01_NODE").unwrap_or_else(|| "node".into());
    let child = ProcessCommand::new(node)
        .arg(&host_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = child.map_err(|error| {
        let _ = fs::remove_file(&host_path);
        format!("could not start the MOD-01 Node headless host: {error}")
    })?;
    if let Some(mut stdin) = child.stdin.take()
        && let Err(error) = stdin.write_all(request)
    {
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_file(&host_path);
        return Err(format!(
            "could not send cartridge to headless host: {error}"
        ));
    }
    let result = child.wait_with_output();
    let _ = fs::remove_file(&host_path);
    let result = match result {
        Ok(result) if result.status.success() => result,
        Ok(result) => {
            return Err(format!(
                "headless host exited {}: {}",
                result.status,
                String::from_utf8_lossy(&result.stderr).trim()
            ));
        }
        Err(error) => {
            return Err(format!("headless host did not complete: {error}"));
        }
    };
    let parsed: serde_json::Value = match serde_json::from_slice(&result.stdout) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Err(format!("headless host returned invalid JSON: {error}"));
        }
    };
    Ok(serde_json::to_vec_pretty(&parsed).expect("JSON value always serializes"))
}

fn load_game(path: &Path) -> Result<modl_core::game::PackedGame, ()> {
    let result = if path.is_dir() || is_workcart_path(path) {
        load_project(path)?
            .pack_game()
            .map_err(|error| error.to_string())
    } else if path
        .extension()
        .is_some_and(|extension| extension == "m01c" || extension == "png")
    {
        let bytes = read_bounded_file(path, modl_core::cartridge_png::MAX_CARTRIDGE_PNG_BYTES)
            .map_err(|error| eprintln!("{}: {error}", path.display()))?;
        let bytes = if path.extension().is_some_and(|extension| extension == "png") {
            decode_cartridge_png(&bytes)
                .map_err(|error| eprintln!("{}: {error}", path.display()))?
                .cartridge
        } else {
            bytes
        };
        modl_core::game::decode_game(&bytes).map_err(|error| error.to_string())
    } else {
        Err("expected a current .m01w, .m01c or .m01c.png game".to_owned())
    };
    result.map_err(|error| eprintln!("{}: {error}", path.display()))
}

fn read_bounded_file(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    let maximum_u64 = u64::try_from(maximum).unwrap_or(u64::MAX);
    if !metadata.is_file() || metadata.len() > maximum_u64 {
        return Err(format!(
            "input requires an ordinary file of at most {maximum} bytes"
        ));
    }
    let mut bytes = Vec::new();
    file.take(maximum_u64.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > maximum {
        return Err(format!("input exceeds {maximum} bytes"));
    }
    Ok(bytes)
}

fn read_headless_saves(
    game: &modl_core::game::PackedGame,
    entry: Option<&Path>,
    catalog: Option<&Path>,
) -> Result<BTreeMap<String, Vec<u8>>, ()> {
    if let Some(path) = entry {
        return read_save_image(Some(path))
            .map(|image| BTreeMap::from([(game.entry().manifest.id.clone(), image)]));
    }
    let Some(path) = catalog else {
        return Ok(BTreeMap::new());
    };
    let read = || -> Result<BTreeMap<String, Vec<u8>>, String> {
        let bytes = read_bounded_file(path, 512 * 1024)?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        let entries = value.as_object().ok_or_else(|| {
            "save catalog must map cartridge IDs to base64 byte images".to_owned()
        })?;
        if entries.len() > 32 {
            return Err("save catalog exceeds 32 cartridge identities".to_owned());
        }
        entries
            .iter()
            .map(|(id, image)| {
                if !game
                    .catalog()
                    .cartridges()
                    .values()
                    .any(|member| &member.manifest.id == id)
                {
                    return Err(format!(
                        "save catalog contains undeclared cartridge ID '{id}'"
                    ));
                }
                let encoded = image
                    .as_str()
                    .filter(|value| value.len() <= 8192_usize.div_ceil(3) * 4)
                    .ok_or_else(|| format!("invalid bounded save image for '{id}'"))?;
                let image = STANDARD
                    .decode(encoded)
                    .map_err(|error| format!("invalid base64 save for '{id}': {error}"))?;
                if image.len() > 8192 || STANDARD.encode(&image) != encoded {
                    return Err(format!("invalid canonical 8 KiB save image for '{id}'"));
                }
                Ok((id.clone(), image))
            })
            .collect()
    };
    read().map_err(|error| eprintln!("{}: {error}", path.display()))
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
    let bytes = read_bounded_file(path, 8 * 1024).map_err(|error| {
        eprintln!("{}: {error}", path.display());
    })?;
    if bytes.len() > 8 * 1024 {
        eprintln!("{}: save image exceeds 8 KiB", path.display());
        return Err(());
    }
    Ok(bytes)
}

fn test_directory(path: &Path) -> ExitCode {
    match load_project(path) {
        Ok(project) => test_workcart(&project),
        Err(()) => ExitCode::FAILURE,
    }
}

fn test_workcart(project: &LoadedProject) -> ExitCode {
    let main_cartridge = project
        .workcart_tests
        .iter()
        .any(|test| test.kind == WorkcartTestKind::Replay)
        .then(|| project.pack_game());
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
    if compilation.generated.is_none() {
        return Err("test produced no executable program".to_owned());
    }
    // Unit tests are disposable ordinary current cartridges, not executable JS paired with
    // a fake ROM. Their private entry is outside the authoring-only tests/ namespace.
    "__mod01_tests__/main.modl".clone_into(&mut manifest.entry);
    files.retain(|path, _| {
        Path::new(path)
            .extension()
            .is_some_and(|extension| extension == "modl")
    });
    files.insert(manifest.entry.clone(), source.as_bytes().to_vec());
    let source = encode_workcart(&manifest, &files, &[], &[]).map_err(|error| error.to_string())?;
    let game = modl_core::game::pack_game_workcart(&source).map_err(|error| error.to_string())?;
    let request = headless_request(
        &game,
        CompileMode::Debug,
        1,
        0x4d30_3031,
        &serde_json::json!({ "revision": 1, "frames": [] }),
        "",
        BTreeMap::new(),
    )?;
    let result = execute_headless_host(
        &serde_json::to_vec(&request).map_err(|error| error.to_string())?,
        1,
    )
    .map_err(|error| format!("headless test host failed: {error}"))?;
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

fn run_scripted_test_payload(
    root: Option<&Path>,
    packed: &modl_core::game::PackedGame,
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
    let parameter = spec.get("parameter").map_or(Ok(""), |value| {
        value
            .as_str()
            .ok_or_else(|| "scripted parameter must be Text".to_owned())
    })?;
    let request = headless_request(
        packed,
        CompileMode::Release,
        frames,
        seed,
        &trace,
        parameter,
        BTreeMap::from([(packed.entry().manifest.id.clone(), save)]),
    )?;
    let result = execute_headless_host(
        &serde_json::to_vec(&request).map_err(|error| error.to_string())?,
        frames,
    )
    .map_err(|error| format!("headless scripted test host failed: {error}"))?;
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
    let result = (|| -> Result<(), String> {
        if path.exists() {
            return Err("refusing to overwrite an existing path".to_owned());
        }
        let id_path = if is_workcart_path(path) {
            path.with_extension("")
        } else {
            path.to_path_buf()
        };
        let id = project_id(&id_path);
        let title = title.map_or_else(|| project_title(&id), ToOwned::to_owned);
        let metadata = format!(
            "format = 2\nlanguage = \"MODL/1\"\nid = \"{id}\"\ntitle = {}\nauthor = {}\nversion = \"0.1.0\"\nentry = \"src/main.modl\"\nupdate_rate = 60\nhardware_revision = 2\n",
            toml_string(&title),
            toml_string(author)
        );
        let manifest = parse_project_manifest(&metadata).map_err(|error| error.to_string())?;
        let files = BTreeMap::from([(
            "src/main.modl".to_owned(),
            b"on draw:\n  clear(0)\n  print(\"HELLO, MOD-01\", 72, 68, 7)\n".to_vec(),
        )]);
        let source =
            encode_workcart(&manifest, &files, &[], &[]).map_err(|error| error.to_string())?;
        let output = if is_workcart_path(path) {
            path.to_path_buf()
        } else {
            path.join("cart.m01w")
        };
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&output, source).map_err(|error| error.to_string())?;
        println!("created {}", output.display());
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            ExitCode::FAILURE
        }
    }
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

fn import_png(
    path: &Path,
    name: &str,
    png: &Path,
    kind: PngImportKind,
    replace: bool,
    output: Option<&Path>,
) -> ExitCode {
    use modl_core::asset_import::{PngAssetKind, import_workcart_png, read_png_source};
    let Ok(project) = load_project(path) else {
        return ExitCode::FAILURE;
    };
    let input = if path.is_dir() {
        path.join("cart.m01w")
    } else {
        path.to_path_buf()
    };
    let destination = output.unwrap_or(&input);
    if !is_workcart_path(destination) || output.is_some_and(|_| destination.exists()) {
        eprintln!("output must be a new .m01w file; omit --output to update the input atomically");
        return ExitCode::FAILURE;
    }
    let kind = match kind {
        PngImportKind::Sprite => PngAssetKind::Sprite,
        PngImportKind::TileSet => PngAssetKind::TileSet,
    };
    let imported = read_png_source(png)
        .and_then(|bytes| import_workcart_png(&project.source, name, &bytes, kind, replace));
    let source = match imported {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = write_workcart_atomically(destination, source.as_bytes()) {
        eprintln!("{}: {error}", destination.display());
        return ExitCode::FAILURE;
    }
    println!("imported '{name}' into {}", destination.display());
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
    let packed = match project.pack_game() {
        Ok(packed) => packed,
        Err(error) => {
            eprintln!("{}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = modl_core::game::decode_game(packed.bytes()) {
        eprintln!("{}: post-pack validation failed: {error}", path.display());
        return ExitCode::FAILURE;
    }
    let output = output.map_or_else(
        || project_output_directory(path).join(format!("{}.m01c", packed.file_stem())),
        Path::to_path_buf,
    );
    if let Some(parent) = output.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        eprintln!("{}: {error}", parent.display());
        return ExitCode::FAILURE;
    }
    if let Err(error) = fs::write(&output, packed.bytes()) {
        eprintln!("{}: {error}", output.display());
        return ExitCode::FAILURE;
    }
    println!(
        "packed {} ({} bytes, {} cartridges)",
        output.display(),
        packed.bytes().len(),
        packed.catalog().cartridges().len()
    );
    ExitCode::SUCCESS
}

fn watch_directory(path: &Path, output: Option<&Path>, once: bool) -> ExitCode {
    if once {
        return pack_directory(path, output);
    }
    let output = output.map_or_else(
        || project_output_directory(path).join("watch.m01c"),
        Path::to_path_buf,
    );
    let mut fingerprint = directory_fingerprint(path).ok();
    if pack_directory(path, Some(&output)) != ExitCode::SUCCESS {
        return ExitCode::FAILURE;
    }
    println!("watching {} -> {}", path.display(), output.display());
    loop {
        let current = directory_fingerprint(path).ok();
        if current != fingerprint {
            fingerprint = current;
            let _ = pack_directory(path, Some(&output));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn directory_fingerprint(path: &Path) -> Result<u64, std::io::Error> {
    // All authored modules/assets/tests live inside the one current workcart.
    // Generated output and unrelated neighbouring files are never inputs.
    let source = if path.is_dir() {
        path.join("cart.m01w")
    } else {
        path.to_path_buf()
    };
    let mut hasher = DefaultHasher::new();
    fs::read(source)?.hash(&mut hasher);
    Ok(hasher.finish())
}

fn info(path: Option<&Path>, source: bool, static_only: bool) -> ExitCode {
    if source {
        let path = path.unwrap_or_else(|| Path::new("."));
        let result = load_game(path).and_then(|game| {
            modl_core::parse_workcart(game.source()).map_err(|error| {
                eprintln!("{error}");
            })
        });
        return match result {
            Ok(project) => {
                println!(
                    "{}",
                    serde_json::to_string(&project).expect("source view serializes")
                );
                ExitCode::SUCCESS
            }
            Err(()) => ExitCode::FAILURE,
        };
    }
    let Some(path) = path else {
        println!(
            "{} cartridge-format/{} compiler/{}",
            modl_core::LANGUAGE_REVISION,
            modl_core::CARTRIDGE_FORMAT_REVISION,
            modl_core::compiler_version()
        );
        return ExitCode::SUCCESS;
    };
    let metadata = cartridge_report(path, static_only).and_then(|report| {
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

fn cartridge_report(path: &Path, static_only: bool) -> Result<serde_json::Value, String> {
    let game = load_game(path).map_err(|()| "could not load game for analysis".to_owned())?;
    let profile = if static_only {
        None
    } else {
        Some(profile_game(&game))
    };
    report::game_report(&game, profile)
}

fn profile_game(game: &modl_core::game::PackedGame) -> Result<serde_json::Value, String> {
    let request = headless_request(
        game,
        CompileMode::Release,
        60,
        modl_core::DEFAULT_SEED,
        &serde_json::json!({"revision":1,"frames":[]}),
        "",
        BTreeMap::new(),
    )?;
    let runtime = execute_headless_host(
        &serde_json::to_vec(&request).map_err(|error| error.to_string())?,
        60,
    )?;
    serde_json::from_slice(&runtime).map_err(|error| error.to_string())
}

fn load_project(path: &Path) -> Result<LoadedProject, ()> {
    let source = if path.is_dir() {
        path.join("cart.m01w")
    } else {
        path.to_path_buf()
    };
    if !is_workcart_path(&source) {
        eprintln!("{}: expected a current .m01w workcart", path.display());
        return Err(());
    }
    load_workcart(&source)
}

fn load_workcart(path: &Path) -> Result<LoadedProject, ()> {
    let bytes = read_bounded_file(path, modl_core::workcart::MAX_WORKCART_DOCUMENT_BYTES).map_err(
        |error| {
            eprintln!("{}: {error}", path.display());
        },
    )?;
    let source =
        String::from_utf8(bytes).map_err(|error| eprintln!("{}: {error}", path.display()))?;
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
        source,
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

fn asset_report(path: &Path) -> ExitCode {
    let Ok(project) = load_project(path) else {
        return ExitCode::FAILURE;
    };
    let report = modl_core::assets::analyze_project_assets(&project.manifest, &project.files);
    match serde_json::to_string_pretty(&report) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    }
    status(!report.is_valid())
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
            let assets =
                modl_core::assets::analyze_project_assets(&project.manifest, &project.files);
            for diagnostic in &assets.diagnostics {
                eprintln!(
                    "{}: error[{}]: {diagnostic}",
                    path.display(),
                    diagnostic.code
                );
            }
            if assets.diagnostics_truncated {
                eprintln!(
                    "{}: further asset diagnostics suppressed (limit 128)",
                    path.display()
                );
            }
            failed |= !assets.is_valid();
            match compile_project(
                &project.manifest_source,
                &project.files,
                CompileMode::Release,
            ) {
                Ok(output) if output.analysis.diagnostics.is_empty() => {
                    if assets.is_valid()
                        && let Err(error) = project.pack_game()
                    {
                        eprintln!("{}: {error}", path.display());
                        failed = true;
                    }
                }
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
        if requested_path.is_dir() && requested_path.join("cart.m01w").is_file() {
            paths.push(requested_path.join("cart.m01w"));
        } else if requested_path.is_dir() {
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
        if is_workcart_path(path) {
            match format_workcart(path, check) {
                Ok(changed) if changed && check => {
                    eprintln!("{}: requires formatting", path.display());
                    failed = true;
                }
                Ok(_) => {}
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

fn format_workcart(path: &Path, check: bool) -> Result<bool, String> {
    let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut project = parse_workcart(&source).map_err(|error| error.to_string())?;
    let mut changed = false;
    for (path, bytes) in &mut project.files {
        if Path::new(path)
            .extension()
            .is_none_or(|extension| extension != "modl")
        {
            continue;
        }
        let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
        let formatted = format_source(&SourceFile::new(FileId(0), path, text))
            .map_err(|error| format!("{path}: {:?}", error.diagnostics))?;
        if formatted != text {
            changed = true;
            *bytes = formatted.into_bytes();
        }
    }
    for test in &mut project.tests {
        if test.kind != WorkcartTestKind::Modl || test.path.ends_with(".fail.modl") {
            continue;
        }
        let formatted = format_source(&SourceFile::new(FileId(0), &test.path, &test.payload))
            .map_err(|error| format!("{}: {:?}", test.path, error.diagnostics))?;
        if formatted != test.payload {
            changed = true;
            test.payload = formatted;
        }
    }
    if changed && !check {
        let encoded = modl_core::workcart::encode_workcart_with_bundle(
            &project.manifest,
            &project.files,
            &project.recipes,
            &project.tests,
            &project.bundle,
        )
        .map_err(|error| error.to_string())?;
        write_workcart_atomically(path, encoded.as_bytes()).map_err(|error| error.to_string())?;
    }
    Ok(changed)
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
