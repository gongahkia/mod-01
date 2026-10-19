use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use serde_json::{Value, json};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mod01"))
}

fn write_project_file(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    let path = root.join("cart.m01w");
    let source = fs::read_to_string(&path)?;
    let mut project = modl_core::parse_workcart(&source).map_err(std::io::Error::other)?;
    if relative.starts_with("tests/") {
        project.tests.retain(|test| test.path != relative);
        let payload = String::from_utf8(bytes.as_ref().to_vec()).map_err(std::io::Error::other)?;
        project.tests.push(modl_core::WorkcartTest {
            name: relative.to_owned(),
            path: relative.to_owned(),
            kind: if Path::new(relative)
                .extension()
                .is_some_and(|extension| extension == "modl")
            {
                modl_core::WorkcartTestKind::Modl
            } else {
                modl_core::WorkcartTestKind::Replay
            },
            payload,
        });
    } else {
        project
            .files
            .insert(relative.to_owned(), bytes.as_ref().to_vec());
    }
    let source = modl_core::encode_workcart(
        &project.manifest,
        &project.files,
        &project.recipes,
        &project.tests,
    )
    .map_err(std::io::Error::other)?;
    fs::write(path, source)
}

#[test]
fn check_accepts_the_positive_language_fixture() {
    let fixture = format!(
        "{}/../modl-core/tests/fixtures/types/positive.modl",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = binary()
        .args(["check", &fixture])
        .output()
        .expect("CLI starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn formatter_check_distinguishes_valid_unformatted_source() {
    let path =
        std::env::temp_dir().join(format!("mod01-cli-{}-{}.modl", std::process::id(), line!()));
    fs::write(&path, "state  score:Int=0\n").expect("temporary source writes");
    let check = binary()
        .args(["fmt", "--check", path.to_str().expect("UTF-8 path")])
        .output()
        .expect("CLI starts");
    assert_eq!(check.status.code(), Some(1));

    let format = binary()
        .args(["fmt", path.to_str().expect("UTF-8 path")])
        .output()
        .expect("CLI starts");
    assert!(format.status.success());
    assert_eq!(
        fs::read_to_string(&path).expect("temporary source reads"),
        "state score: Int = 0\n"
    );
    fs::remove_file(path).expect("temporary source removes");
}

#[test]
fn build_emits_javascript_and_source_map() {
    let stem = std::env::temp_dir().join(format!("mod01-build-{}-{}", std::process::id(), line!()));
    let source = stem.with_extension("modl");
    let javascript = stem.with_extension("js");
    let source_map = stem.with_extension("js.map");
    fs::write(&source, "state score: Int = 0\non update:\n  score += 1\n")
        .expect("temporary source writes");
    let output = binary()
        .args([
            "build",
            source.to_str().expect("UTF-8 source path"),
            "--output",
            javascript.to_str().expect("UTF-8 output path"),
        ])
        .output()
        .expect("CLI starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fs::read_to_string(&javascript)
            .expect("JavaScript reads")
            .contains("export default function createCartridge")
    );
    let map: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&source_map).expect("source map reads"))
            .expect("source map is JSON");
    assert_eq!(map["version"], 3);
    for path in [source, javascript, source_map] {
        fs::remove_file(path).expect("temporary build artifact removes");
    }
}

#[test]
fn browser_exports_are_not_product_commands() {
    for format in ["html", "zip"] {
        let result = binary()
            .args(["export", format])
            .output()
            .expect("CLI starts");
        assert_eq!(result.status.code(), Some(2));
    }
}

#[test]
fn workcart_commands_pack_export_check_and_run_through_the_same_cartridge() {
    let cart = std::env::temp_dir().join(format!(
        "mod01-workcart-{}-{}.m01w",
        std::process::id(),
        line!()
    ));
    fs::write(
        &cart,
        r#"format = 2

[cartridge]
language = "MODL/1"
id = "cli-workcart"
title = "CLI WORKCART"
author = "@gongahkia"
version = "0.1.0"
entry = "src/main.modl"
update_rate = 60

[[module]]
path = "src/main.modl"
source = '''
on draw:
  clear(25)
'''
"#,
    )
    .expect("work-cart writes");
    let packed = cart.with_extension("m01c");
    let png = cart.with_extension("png");
    let trace = cart.with_extension("trace.json");
    for arguments in [
        vec!["check".to_owned(), cart.display().to_string()],
        vec![
            "pack".to_owned(),
            cart.display().to_string(),
            "--output".to_owned(),
            packed.display().to_string(),
        ],
        vec![
            "export".to_owned(),
            "png".to_owned(),
            cart.display().to_string(),
            "--output".to_owned(),
            png.display().to_string(),
        ],
        vec![
            "run".to_owned(),
            cart.display().to_string(),
            "--headless".to_owned(),
            "--frames".to_owned(),
            "2".to_owned(),
            "--output".to_owned(),
            trace.display().to_string(),
        ],
    ] {
        let output = binary().args(arguments).output().expect("CLI starts");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let image =
        modl_core::decode_cartridge_png(&fs::read(&png).expect("PNG reads")).expect("PNG decodes");
    assert_eq!(
        image.cartridge,
        fs::read(&packed).expect("canonical cartridge reads")
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&trace).expect("trace reads"))
            .expect("trace is JSON")["summary"]["completedFrames"],
        2
    );
    for path in [cart, packed, png, trace] {
        fs::remove_file(path).expect("temporary work-cart artifact removes");
    }
}

#[test]
fn generate_map_materializes_a_stored_workcart_recipe_atomically() {
    let cart = std::env::temp_dir().join(format!(
        "mod01-generate-{}-{}.m01w",
        std::process::id(),
        line!()
    ));
    let pixels = vec!["0"; 64].join(",");
    fs::write(
        &cart,
        format!(
            r#"format = 2

[cartridge]
language = "MODL/1"
id = "generated-map"
title = "GENERATED MAP"
author = "@gongahkia"
version = "0.1.0"
entry = "src/main.modl"
update_rate = 60

[[module]]
path = "src/main.modl"
source = "on draw:\n  clear(0)\n"

[[asset]]
name = "tiles"
kind = "tile_set"
path = "assets/tiles.m01g"
payload = '''{{"revision":1,"kind":"tile_set","tiles":[[{pixels}]],"flags":[0]}}'''

[[asset]]
name = "world"
kind = "map"
path = "assets/world.m01m"
payload = '''{{"revision":1,"kind":"map","layers":[]}}'''

[[recipe]]
algorithm = "noise"
id = "world-noise"
output = "world"
seed = 7
width = 4
height = 3
tile_set = "tiles"
tiles = [0]
"#
        ),
    )
    .expect("work-cart writes");
    let generate = binary()
        .args([
            "generate",
            "map",
            cart.to_str().expect("UTF-8 work-cart path"),
            "world-noise",
        ])
        .output()
        .expect("CLI starts");
    assert!(
        generate.status.success(),
        "{}",
        String::from_utf8_lossy(&generate.stderr)
    );
    let generated = fs::read_to_string(&cart).expect("generated work-cart reads");
    let project = modl_core::parse_workcart(&generated).expect("generated work-cart parses");
    let map: Value =
        serde_json::from_slice(&project.files["assets/world.m01m"]).expect("generated map is JSON");
    assert_eq!(map["layers"][0]["width"], 4);
    assert_eq!(map["layers"][0]["height"], 3);
    assert_eq!(map["layers"][0]["cells"].as_array().map(Vec::len), Some(12));
    let repeat = binary()
        .args([
            "generate",
            "map",
            cart.to_str().expect("UTF-8 work-cart path"),
            "world-noise",
        ])
        .output()
        .expect("CLI starts");
    assert!(repeat.status.success());
    assert_eq!(
        generated,
        fs::read_to_string(&cart).expect("regenerated work-cart reads")
    );
    fs::remove_file(cart).expect("temporary generated work-cart removes");
}

#[test]
fn check_reports_packed_capacity_failures_for_workcarts() {
    let cart = std::env::temp_dir().join(format!(
        "mod01-check-capacity-{}-{}.m01w",
        std::process::id(),
        line!()
    ));
    let padding = "0123456789abcdef".repeat(70_000);
    fs::write(
        &cart,
        format!(
            r#"format = 2

[cartridge]
language = "MODL/1"
id = "capacity-check"
title = "CAPACITY CHECK"
author = "@gongahkia"
version = "0.1.0"
entry = "src/main.modl"
update_rate = 60

[[module]]
path = "src/main.modl"
source = "on draw:\n  clear(0)\n"

[[asset]]
name = "oversized"
kind = "sprite"
path = "assets/oversized.m01g"
payload = '''{{"padding":"{padding}"}}'''
"#
        ),
    )
    .expect("oversized work-cart writes");
    let output = binary()
        .args(["check", cart.to_str().expect("UTF-8 work-cart path")])
        .output()
        .expect("CLI starts");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("M014005"));
    fs::remove_file(cart).expect("temporary capacity work-cart removes");
}

#[test]
fn old_directory_authoring_and_migration_are_rejected() {
    assert_eq!(
        binary()
            .args(["migrate", "anything"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
    let root = std::env::temp_dir().join(format!("mod01-obsolete-project-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("cart.toml"), "format = 1").unwrap();
    assert_eq!(
        binary()
            .args(["check", root.to_str().unwrap()])
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn new_creates_the_same_single_workcart_as_a_file_or_directory() {
    let root = std::env::temp_dir().join(format!("mod01-new-workcart-{}", std::process::id()));
    let file = root.with_extension("m01w");
    assert!(
        binary()
            .args(["new", root.to_str().unwrap(), "--title", "WORKCART"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        binary()
            .args(["new", file.to_str().unwrap(), "--title", "WORKCART"])
            .output()
            .unwrap()
            .status
            .success()
    );
    let directory_source = fs::read_to_string(root.join("cart.m01w")).unwrap();
    assert_eq!(directory_source, fs::read_to_string(&file).unwrap());
    assert!(!root.join("cart.toml").exists());
    let source = binary()
        .args(["info", file.to_str().unwrap(), "--source"])
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let view: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert!(view["files"]["src/main.modl"].is_array());
    assert_eq!(view["manifest"]["hardware_revision"], 2);
    write_project_file(&root, "src/main.modl", "on draw:\n clear(0)\n").unwrap();
    assert_eq!(
        binary()
            .args(["fmt", "--check", root.to_str().unwrap()])
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );
    assert!(
        binary()
            .args(["fmt", root.to_str().unwrap()])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        binary()
            .args(["fmt", "--check", root.to_str().unwrap()])
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::remove_dir_all(root).unwrap();
    fs::remove_file(file).unwrap();
}

#[test]
fn headless_run_emits_deterministic_machine_readable_traces_for_projects_and_cartridges() {
    let project =
        std::env::temp_dir().join(format!("mod01-headless-{}-{}", std::process::id(), line!()));
    let create = binary()
        .args([
            "new",
            project.to_str().expect("UTF-8 project path"),
            "--title",
            "HEADLESS TEST",
        ])
        .output()
        .expect("CLI starts");
    assert!(create.status.success());
    let cartridge = project.with_extension("m01c");
    let pack = binary()
        .args([
            "pack",
            project.to_str().expect("UTF-8 project path"),
            "--output",
            cartridge.to_str().expect("UTF-8 cartridge path"),
        ])
        .output()
        .expect("CLI starts");
    assert!(pack.status.success());
    let input_trace = project.with_extension("input.json");
    fs::write(
        &input_trace,
        br#"{"revision":1,"frames":[{"frame":0,"controllers":[{"port":1,"buttons":["a"]}]}]}"#,
    )
    .expect("input trace writes");
    let save = project.with_extension("sav");
    fs::write(&save, br#"{"score":7}"#).expect("save image writes");
    let first = project.with_extension("project-trace.json");
    let second = project.with_extension("cartridge-trace.json");
    for (input, result) in [(&project, &first), (&cartridge, &second)] {
        let run = binary()
            .args([
                "run",
                input.to_str().expect("UTF-8 input path"),
                "--headless",
                "--frames",
                "3",
                "--seed",
                "7",
                "--input",
                input_trace.to_str().expect("UTF-8 input trace path"),
                "--save",
                save.to_str().expect("UTF-8 save path"),
                "--output",
                result.to_str().expect("UTF-8 result path"),
            ])
            .output()
            .expect("CLI starts");
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }
    let project_trace: Value =
        serde_json::from_slice(&fs::read(&first).expect("project trace reads"))
            .expect("project trace is JSON");
    let cartridge_trace: Value =
        serde_json::from_slice(&fs::read(&second).expect("cartridge trace reads"))
            .expect("cartridge trace is JSON");
    assert_eq!(project_trace, cartridge_trace);
    assert_eq!(project_trace["revision"], 1);
    assert_eq!(project_trace["summary"]["completedFrames"], 3);
    assert_eq!(project_trace["configuration"]["seed"], 7);
    assert_eq!(project_trace["frames"].as_array().map(Vec::len), Some(3));
    assert_eq!(
        project_trace["summary"]["finalFramebufferSha256"]
            .as_str()
            .map(str::len),
        Some(64)
    );
    for path in [first, second, cartridge, input_trace, save] {
        fs::remove_file(path).expect("temporary headless artifact removes");
    }
    fs::remove_dir_all(project).expect("temporary project removes");
}

#[test]
fn project_test_command_runs_modl_compile_fail_and_scripted_snapshot_tests() {
    let project =
        std::env::temp_dir().join(format!("mod01-tests-{}-{}", std::process::id(), line!()));
    let create = binary()
        .args(["new", project.to_str().expect("UTF-8 project path")])
        .output()
        .expect("CLI starts");
    assert!(create.status.success());
    write_project_file(&project, "tests/pure.modl",
        "fn twice(value: Int) -> Int:\n  return value * 2\non start:\n  assert twice(3) == 6, \"pure assertion\"\n",
    )
    .expect("pure test writes");
    write_project_file(
        &project,
        "tests/name.fail.modl",
        "// expect M013002\non start:\n  missing_name()\n",
    )
    .expect("compile-fail test writes");

    let baseline_path = project.join("baseline.json");
    let baseline = binary()
        .args([
            "run",
            project.to_str().expect("UTF-8 project path"),
            "--headless",
            "--frames",
            "2",
            "--seed",
            "9",
            "--output",
            baseline_path.to_str().expect("UTF-8 baseline path"),
        ])
        .output()
        .expect("CLI starts");
    assert!(baseline.status.success());
    let baseline: Value =
        serde_json::from_slice(&fs::read(&baseline_path).expect("headless baseline reads"))
            .expect("baseline is JSON");
    let snapshot = json!({
        "revision": 1,
        "frames": 2,
        "seed": 9,
        "input": { "revision": 1, "frames": [] },
        "expect": {
            "completedFrames": 2,
            "finalFramebufferSha256": baseline["summary"]["finalFramebufferSha256"],
            "finalStateSha256": baseline["summary"]["finalStateSha256"]
        }
    });
    write_project_file(
        &project,
        "tests/boot.m01run.json",
        serde_json::to_vec_pretty(&snapshot).expect("snapshot serializes"),
    )
    .expect("scripted test writes");
    let output = binary()
        .args(["test", project.to_str().expect("UTF-8 project path")])
        .output()
        .expect("CLI starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test result: 3 passed; 0 failed"));
    assert!(stdout.contains("pure.modl"));
    assert!(stdout.contains("name.fail.modl"));
    assert!(stdout.contains("boot.m01run.json"));
    fs::remove_file(baseline_path).expect("baseline removes");
    fs::remove_dir_all(project).expect("temporary project removes");
}

#[test]
fn watch_repacks_native_cartridges_without_a_server() {
    let project =
        std::env::temp_dir().join(format!("mod01-watch-{}-{}", std::process::id(), line!()));
    assert!(
        binary()
            .args(["new", project.to_str().unwrap()])
            .output()
            .unwrap()
            .status
            .success()
    );
    let packed = project.with_extension("watch.m01c");
    let mut child = binary()
        .args([
            "watch",
            project.to_str().unwrap(),
            "--output",
            packed.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("watch starts");
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    assert!(line.starts_with("packed "), "{line}");
    let before = fs::read(&packed).unwrap();
    write_project_file(&project, "src/main.modl", "on draw:\n  clear(1)\n").unwrap();
    let mut refreshed = false;
    for _ in 0..40 {
        thread::sleep(Duration::from_millis(50));
        if fs::read(&packed)
            .is_ok_and(|bytes| bytes != before && modl_core::decode_cartridge(&bytes).is_ok())
        {
            refreshed = true;
            break;
        }
    }
    child.kill().unwrap();
    let _ = child.wait();
    assert!(
        refreshed,
        "watch did not publish the rebuilt native cartridge"
    );
    fs::remove_dir_all(project).unwrap();
    fs::remove_file(packed).unwrap();
}

#[test]
fn bundled_cartridge_headless_replays_match_golden_machine_traces() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let golden: Value = serde_json::from_slice(
        &fs::read(root.join("tests/replays/golden.json")).expect("golden replay reads"),
    )
    .expect("golden replay is JSON");
    for id in ["cinder-circuit", "ashvault", "raster-rush"] {
        let expected = &golden["cartridges"][id];
        let frames = expected["frames"]
            .as_u64()
            .expect("frame count")
            .to_string();
        let output =
            std::env::temp_dir().join(format!("mod01-golden-{}-{id}.json", std::process::id()));
        let project = root.join("cartridges").join(id);
        let replay = root.join("tests/replays").join(format!("{id}.json"));
        let run = binary()
            .args([
                "run",
                project.to_str().expect("UTF-8 cartridge path"),
                "--headless",
                "--frames",
                &frames,
                "--input",
                replay.to_str().expect("UTF-8 replay path"),
                "--output",
                output.to_str().expect("UTF-8 output path"),
            ])
            .output()
            .expect("CLI starts");
        assert!(
            run.status.success(),
            "{id}: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let actual: Value = serde_json::from_slice(&fs::read(&output).expect("trace reads"))
            .expect("trace is JSON");
        assert_eq!(
            actual["cartridge"]["sha256"], expected["cartridgeSha256"],
            "{id}"
        );
        assert_eq!(
            actual["configuration"]["inputTraceSha256"], expected["inputTraceSha256"],
            "{id}"
        );
        for (actual_key, expected_key) in [
            ("workPeak", "workPeak"),
            ("finalFramebufferSha256", "framebufferSha256"),
            ("finalStateSha256", "stateSha256"),
            ("audioCommandsSha256", "audioCommandsSha256"),
            ("pcmSha256", "pcmSha256"),
        ] {
            assert_eq!(
                actual["summary"][actual_key], expected[expected_key],
                "{id} {actual_key}"
            );
        }
        for checkpoint in expected["checkpoints"]
            .as_array()
            .expect("checkpoint array")
        {
            let frame = usize::try_from(checkpoint["frame"].as_u64().expect("checkpoint frame"))
                .expect("checkpoint fits usize");
            assert_eq!(
                actual["frames"][frame]["frame"], checkpoint["frame"],
                "{id}"
            );
            assert_eq!(
                actual["frames"][frame]["framebufferSha256"], checkpoint["framebufferSha256"],
                "{id} frame {frame}"
            );
        }
        fs::remove_file(output).expect("temporary trace removes");
    }
}

#[test]
fn bundled_cartridges_compile_and_pack_within_capacity() {
    let cartridges = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cartridges");
    for id in [
        "cinder-circuit",
        "ashvault",
        "raster-rush",
        "mod01-service",
        "signal-4k",
        "pocket-relay",
        "hardware-gauntlet",
        "modl-tutorial",
    ] {
        let project = cartridges.join(id);
        let check = binary()
            .args(["check", project.to_str().expect("UTF-8 cartridge path")])
            .output()
            .expect("CLI starts");
        assert!(
            check.status.success(),
            "{id}: {}",
            String::from_utf8_lossy(&check.stderr)
        );
        let packed =
            std::env::temp_dir().join(format!("mod01-bundled-{}-{id}.m01c", std::process::id()));
        let pack = binary()
            .args([
                "pack",
                project.to_str().expect("UTF-8 cartridge path"),
                "--output",
                packed.to_str().expect("UTF-8 output path"),
            ])
            .output()
            .expect("CLI starts");
        assert!(
            pack.status.success(),
            "{id}: {}",
            String::from_utf8_lossy(&pack.stderr)
        );
        let size = fs::metadata(&packed)
            .expect("packed cartridge metadata")
            .len();
        assert!(size < 256 * 1024, "{id} exceeds the cartridge capacity");
        let class_limit = match id {
            "signal-4k" => 4_096,
            "pocket-relay" => 16_384,
            _ => 65_536,
        };
        assert!(
            size <= class_limit,
            "{id} misses its complete-artifact class"
        );
        if matches!(
            id,
            "signal-4k" | "pocket-relay" | "hardware-gauntlet" | "modl-tutorial"
        ) {
            let headless = binary()
                .args([
                    "run",
                    project.to_str().expect("UTF-8 cartridge path"),
                    "--headless",
                    "--frames",
                    "3",
                ])
                .output()
                .expect("showcase headless run starts");
            assert!(
                headless.status.success(),
                "{id}: {}",
                String::from_utf8_lossy(&headless.stderr)
            );
            let result: Value = serde_json::from_slice(&headless.stdout).expect("headless JSON");
            assert_eq!(result["summary"]["completedFrames"], 3, "{id}");
            assert!(result.get("fault").is_none(), "{id}: {result}");
        }
        fs::remove_file(packed).expect("temporary cartridge removes");
    }
}

fn assert_project_info(path: &Path) {
    let info = binary()
        .args(["info", path.to_str().expect("UTF-8 cartridge path")])
        .output()
        .expect("CLI starts");
    assert!(info.status.success());
    let report: serde_json::Value =
        serde_json::from_slice(&info.stdout).expect("info is structured JSON");
    assert_eq!(report["title"], "CLI TEST");
    assert_eq!(report["analysis"]["sections"]["saveAllocationBytes"], 8192);
    assert_eq!(
        report["analysis"]["runtime60Frames"]["summary"]["completedFrames"],
        52
    );
    assert_eq!(
        report["analysis"]["runtime60Frames"]["fault"]["code"],
        "M019002"
    );
    assert!(
        report["analysis"]["largestEntries"]
            .as_array()
            .is_some_and(|entries| !entries.is_empty())
    );
}

#[test]
fn new_pack_and_info_form_a_deterministic_project_workflow() {
    let project =
        std::env::temp_dir().join(format!("mod01-project-{}-{}", std::process::id(), line!()));
    assert!(
        !project.exists(),
        "temporary project path unexpectedly exists"
    );
    let create = binary()
        .args([
            "new",
            project.to_str().expect("UTF-8 project path"),
            "--title",
            "CLI TEST",
        ])
        .output()
        .expect("CLI starts");
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    write_project_file(
        &project,
        "src/math.modl",
        "fn twice(value: Int) -> Int:\n  return value * 2\n",
    )
    .expect("module writes");
    write_project_file(
        &project,
        "src/main.modl",
        "import src.math as math\nstate score: Int = 1\non update:\n  score = math.twice(score)\n",
    )
    .expect("entry writes");
    let linked = project.with_extension("linked.js");
    let build = binary()
        .args([
            "build",
            project.to_str().expect("UTF-8 project path"),
            "--output",
            linked.to_str().expect("UTF-8 JavaScript path"),
        ])
        .output()
        .expect("CLI starts");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let first = project.with_extension("first.m01c");
    let second = project.with_extension("second.m01c");
    for output in [&first, &second] {
        let pack = binary()
            .args([
                "pack",
                project.to_str().expect("UTF-8 project path"),
                "--output",
                output.to_str().expect("UTF-8 cartridge path"),
            ])
            .output()
            .expect("CLI starts");
        assert!(
            pack.status.success(),
            "{}",
            String::from_utf8_lossy(&pack.stderr)
        );
    }
    assert_eq!(
        fs::read(&first).expect("first cartridge reads"),
        fs::read(&second).expect("second cartridge reads")
    );
    let watched = project.with_extension("watched.m01c");
    let watch = binary()
        .args([
            "watch",
            project.to_str().expect("UTF-8 project path"),
            "--once",
            "--output",
            watched.to_str().expect("UTF-8 cartridge path"),
        ])
        .output()
        .expect("CLI starts");
    assert!(
        watch.status.success(),
        "{}",
        String::from_utf8_lossy(&watch.stderr)
    );
    assert_eq!(
        fs::read(&first).expect("first cartridge reads"),
        fs::read(&watched).expect("watched cartridge reads")
    );
    assert_project_info(&first);

    fs::remove_file(first).expect("first cartridge removes");
    fs::remove_file(second).expect("second cartridge removes");
    fs::remove_file(watched).expect("watched cartridge removes");
    fs::remove_file(linked.with_extension("js.map")).expect("linked source map removes");
    fs::remove_file(linked).expect("linked JavaScript removes");
    fs::remove_dir_all(project).expect("temporary project removes");
}

#[test]
fn png_export_contains_the_canonical_cartridge_and_supports_info() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cartridges/cinder-circuit");
    let stem = std::env::temp_dir().join(format!("mod01-png-{}-{}", std::process::id(), line!()));
    let raw = stem.with_extension("m01c");
    let png = stem.with_extension("m01c.png");
    for (command, output) in [("pack", &raw), ("export", &png)] {
        let mut process = binary();
        if command == "export" {
            process.args(["export", "png"]);
        } else {
            process.arg("pack");
        }
        let result = process
            .arg(&project)
            .arg("--output")
            .arg(output)
            .output()
            .expect("CLI starts");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let image = modl_core::decode_cartridge_png(&fs::read(&png).expect("PNG reads"))
        .expect("cartridge PNG decodes");
    assert_eq!(
        image.cartridge,
        fs::read(&raw).expect("raw cartridge reads")
    );
    assert_eq!(image.metadata.title, "CINDER CIRCUIT");
    let info = binary()
        .args(["info"])
        .arg(&png)
        .output()
        .expect("CLI starts");
    assert!(info.status.success());
    assert!(String::from_utf8_lossy(&info.stdout).contains("CINDER CIRCUIT"));
    fs::remove_file(raw).expect("raw cartridge removes");
    fs::remove_file(png).expect("PNG cartridge removes");
}

#[test]
fn lsp_serves_diagnostics_completion_and_symbol_navigation() {
    let uri = "file:///tmp/lsp-test.modl";
    let source = "state score: Int = 0\n\non update:\n  score += 1\n\non draw:\n  clear(0)\n";
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": { "textDocument": { "uri": uri, "languageId": "modl", "version": 1, "text": source } }
        }),
        json!({
            "jsonrpc": "2.0", "id": 2, "method": "textDocument/completion",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 3, "character": 4 } }
        }),
        json!({
            "jsonrpc": "2.0", "id": 3, "method": "textDocument/hover",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 6, "character": 4 } }
        }),
        json!({
            "jsonrpc": "2.0", "id": 4, "method": "textDocument/definition",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 3, "character": 4 } }
        }),
        json!({
            "jsonrpc": "2.0", "id": 5, "method": "textDocument/references",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 3, "character": 4 }, "context": { "includeDeclaration": true } }
        }),
        json!({
            "jsonrpc": "2.0", "id": 6, "method": "textDocument/rename",
            "params": { "textDocument": { "uri": uri }, "position": { "line": 3, "character": 4 }, "newName": "points" }
        }),
        json!({ "jsonrpc": "2.0", "id": 7, "method": "shutdown", "params": null }),
        json!({ "jsonrpc": "2.0", "method": "exit", "params": null }),
    ];
    let mut child = binary()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("LSP starts");
    {
        let input = child.stdin.as_mut().expect("LSP stdin");
        for message in messages {
            let bytes = serde_json::to_vec(&message).expect("request serializes");
            write!(input, "Content-Length: {}\r\n\r\n", bytes.len()).expect("header writes");
            input.write_all(&bytes).expect("body writes");
        }
    }
    let output = child.wait_with_output().expect("LSP exits");
    assert!(output.status.success());
    let responses = decode_lsp_messages(&output.stdout);
    let response = |id: i64| {
        responses
            .iter()
            .find(|value| value.get("id").and_then(Value::as_i64) == Some(id))
            .expect("response id exists")
    };
    assert_eq!(response(1)["result"]["serverInfo"]["name"], "mod01");
    for definition in modl_core::builtin_definitions() {
        assert!(
            response(2)["result"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["label"] == definition.name
                    && item["detail"] == definition.signature),
            "{}",
            definition.name
        );
    }
    assert!(
        response(2)["result"]
            .as_array()
            .expect("completion list")
            .iter()
            .any(|item| item["label"] == "clear")
    );
    assert!(
        response(3)["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|value| value.contains("fn clear"))
    );
    assert_eq!(response(4)["result"]["range"]["start"]["line"], 0);
    assert_eq!(
        response(5)["result"].as_array().expect("references").len(),
        2
    );
    assert_eq!(
        response(6)["result"]["changes"][uri]
            .as_array()
            .expect("rename edits")
            .len(),
        2
    );
    assert!(responses.iter().any(|value| {
        value.get("method") == Some(&Value::String("textDocument/publishDiagnostics".to_owned()))
            && value["params"]["diagnostics"] == json!([])
    }));
}

#[test]
#[allow(clippy::too_many_lines)]
fn lsp_navigates_and_renames_symbols_across_project_modules() {
    let math_uri = "file:///tmp/px-project/src/math.modl";
    let main_uri = "file:///tmp/px-project/src/main.modl";
    let math = "pub fn twice(value: Int) -> Int:\n  return value * 2\n";
    let main =
        "import src.math as math\nstate score: Int = 1\non update:\n  score = math.twice(score)\n";
    let messages = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }),
        json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": { "textDocument": { "uri": math_uri, "languageId": "modl", "version": 1, "text": math } }
        }),
        json!({
            "jsonrpc": "2.0", "method": "textDocument/didOpen",
            "params": { "textDocument": { "uri": main_uri, "languageId": "modl", "version": 1, "text": main } }
        }),
        json!({
            "jsonrpc": "2.0", "id": 2, "method": "textDocument/definition",
            "params": { "textDocument": { "uri": main_uri }, "position": { "line": 3, "character": 16 } }
        }),
        json!({
            "jsonrpc": "2.0", "id": 3, "method": "textDocument/references",
            "params": { "textDocument": { "uri": main_uri }, "position": { "line": 3, "character": 16 }, "context": { "includeDeclaration": true } }
        }),
        json!({
            "jsonrpc": "2.0", "id": 4, "method": "textDocument/rename",
            "params": { "textDocument": { "uri": main_uri }, "position": { "line": 3, "character": 16 }, "newName": "double" }
        }),
        json!({
            "jsonrpc": "2.0", "id": 5, "method": "textDocument/signatureHelp",
            "params": { "textDocument": { "uri": main_uri }, "position": { "line": 3, "character": 21 } }
        }),
        json!({
            "jsonrpc": "2.0", "id": 6, "method": "textDocument/documentSymbol",
            "params": { "textDocument": { "uri": math_uri } }
        }),
        json!({
            "jsonrpc": "2.0", "id": 7, "method": "workspace/symbol", "params": { "query": "twi" }
        }),
        json!({
            "jsonrpc": "2.0", "method": "textDocument/didChange",
            "params": { "textDocument": { "uri": math_uri, "version": 2 },
                "contentChanges": [{ "text": "private fn twice(value: Int) -> Int:\n  return value * 2\n" }] }
        }),
        json!({ "jsonrpc": "2.0", "id": 8, "method": "shutdown", "params": null }),
        json!({ "jsonrpc": "2.0", "method": "exit", "params": null }),
    ];
    let mut child = binary()
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("LSP starts");
    {
        let input = child.stdin.as_mut().expect("LSP stdin");
        for message in messages {
            let bytes = serde_json::to_vec(&message).expect("request serializes");
            write!(input, "Content-Length: {}\r\n\r\n", bytes.len()).expect("header writes");
            input.write_all(&bytes).expect("body writes");
        }
    }
    let output = child.wait_with_output().expect("LSP exits");
    assert!(output.status.success());
    let responses = decode_lsp_messages(&output.stdout);
    let response = |id: i64| {
        responses
            .iter()
            .find(|value| value.get("id").and_then(Value::as_i64) == Some(id))
            .expect("response id exists")
    };
    assert_eq!(response(2)["result"]["uri"], math_uri);
    assert_eq!(response(2)["result"]["range"]["start"]["line"], 0);
    assert_eq!(response(3)["result"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        response(4)["result"]["changes"][math_uri]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert_eq!(
        response(4)["result"]["changes"][main_uri]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    assert!(
        response(5)["result"]["signatures"][0]["label"]
            .as_str()
            .is_some_and(|label| label.starts_with("fn twice("))
    );
    assert_eq!(response(6)["result"][0]["name"], "twice");
    assert_eq!(response(7)["result"][0]["name"], "twice");
    assert!(responses.iter().any(|value| {
        value.get("method") == Some(&Value::String("textDocument/publishDiagnostics".to_owned()))
            && value["params"]["uri"] == main_uri
            && value["params"]["diagnostics"] == json!([])
    }));
    assert!(responses.iter().any(|value| {
        value.get("method") == Some(&Value::String("textDocument/publishDiagnostics".to_owned()))
            && value["params"]["uri"] == main_uri
            && value["params"]["diagnostics"]
                .as_array()
                .is_some_and(|diagnostics| !diagnostics.is_empty())
    }));
}

fn decode_lsp_messages(bytes: &[u8]) -> Vec<Value> {
    let mut messages = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let separator = bytes[cursor..]
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("LSP header terminator");
        let header_end = cursor + separator;
        let header = std::str::from_utf8(&bytes[cursor..header_end]).expect("ASCII headers");
        let length = header
            .strip_prefix("Content-Length: ")
            .expect("content length")
            .parse::<usize>()
            .expect("numeric content length");
        let body_start = header_end + 4;
        let body_end = body_start + length;
        messages.push(serde_json::from_slice(&bytes[body_start..body_end]).expect("JSON response"));
        cursor = body_end;
    }
    messages
}
