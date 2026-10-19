use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mod01"))
}

fn game_fixture(count: usize, padding: bool) -> String {
    let mut main =
        modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
    "cli-game-main".clone_into(&mut main.manifest.id);
    main.files.insert(
        main.manifest.entry.clone(),
        include_str!("../../../tests/conformance/game-main.modl")
            .as_bytes()
            .to_vec(),
    );
    if padding {
        main.files.insert(
            "authoring/notes.txt".to_owned(),
            (0..600_000)
                .map(|index| b'a' + u8::try_from((index * 17 + index / 29) % 26).unwrap())
                .collect(),
        );
    }
    let mut children = Vec::new();
    for index in 0..count {
        let alias = if index == 0 {
            "main".to_owned()
        } else if count == 2 {
            "arena".to_owned()
        } else {
            format!("level{index}")
        };
        let mut member = main.clone();
        if index != 0 {
            member.manifest.id = format!("cli-game-member-{index}");
            member.manifest.update_rate = 30;
        }
        let code = if count == 2 {
            if index == 0 {
                include_str!("../../../tests/conformance/game-main.modl")
            } else {
                include_str!("../../../tests/conformance/game-arena.modl")
            }
            .to_owned()
        } else {
            let next = if index + 1 == count {
                "main".to_owned()
            } else {
                format!("level{}", index + 1)
            };
            format!(
                "on start:\n  save_set_int(\"visits\", save_get_int(\"visits\", 0) + 1)\non update:\n  load_cart(\"{next}\", \"\")\non draw:\n  clear({index})\n"
            )
        };
        member
            .files
            .insert(member.manifest.entry.clone(), code.into_bytes());
        if index == 0 {
            main = member;
        } else {
            children.push(modl_core::bundle::BundleSource {
                alias,
                source: modl_core::encode_workcart(&member.manifest, &member.files, &[], &[])
                    .unwrap(),
            });
        }
    }
    modl_core::workcart::encode_workcart_with_bundle(
        &main.manifest,
        &main.files,
        &[],
        &[],
        &children,
    )
    .unwrap()
}

fn game_trace(
    path: &Path,
    frames: u32,
    parameter: &str,
    input: Option<&Path>,
    saves: Option<&Path>,
) -> Value {
    let mut command = binary();
    command.arg("run").arg(path).args([
        "--headless",
        "--frames",
        &frames.to_string(),
        "--parameter",
        parameter,
    ]);
    if let Some(path) = input {
        command.arg("--input").arg(path);
    }
    if let Some(path) = saves {
        command.arg("--saves").arg(path);
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

fn info_json(path: &Path, static_only: bool, without_node: bool) -> Value {
    let mut command = binary();
    command.arg("info").arg(path);
    if static_only {
        command.arg("--static");
    }
    if without_node {
        command.env("PATH", "");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "successful analysis carries profile failures in JSON"
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn static_info_has_identical_shipping_and_per_rom_capacity_reports_for_large_source_raw_and_png_games()
 {
    let directory = std::env::temp_dir().join(format!(
        "mod01-info-large-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir(&directory).unwrap();
    let mut main = modl_core::parse_workcart(&game_fixture(2, true)).unwrap();
    main.manifest.assets.insert(
        "probe".to_owned(),
        modl_core::ProjectAsset {
            kind: modl_core::AssetKind::Sprite,
            path: "assets/probe.json".to_owned(),
        },
    );
    main.files.insert(
        "assets/probe.json".to_owned(),
        serde_json::to_vec(&json!({
            "revision":2,"kind":"sprite","width":2,"height":1,"frames":[[0,255],[254,1]]
        }))
        .unwrap(),
    );
    let source = modl_core::workcart::encode_workcart_with_bundle(
        &main.manifest,
        &main.files,
        &[],
        &[],
        &main.bundle,
    )
    .unwrap();
    let game = modl_core::game::pack_game_workcart(&source).unwrap();
    assert!(game.bytes().len() > 1024 * 1024);
    let path = directory.join("cart.m01w");
    let raw = directory.join("game.m01c");
    let png = directory.join("game.m01c.png");
    fs::write(&path, &source).unwrap();
    fs::write(&raw, game.bytes()).unwrap();
    fs::write(
        &png,
        modl_core::cartridge_png::encode_game_png(&game).unwrap(),
    )
    .unwrap();
    let expected = info_json(&path, true, true);
    assert_eq!(expected, info_json(&raw, true, true));
    assert_eq!(expected, info_json(&png, true, true));
    let analysis = &expected["analysis"];
    assert_eq!(analysis["revision"], 2);
    assert_eq!(analysis["gameIdentity"], game.identity());
    assert_eq!(analysis["shipping"]["canonicalBytes"], game.bytes().len());
    assert!(
        analysis.get("sizeClass").is_none(),
        "shipping container is not one ROM"
    );
    assert_eq!(analysis["runtime60Frames"]["status"], "not_requested");
    assert_eq!(analysis["members"].as_object().unwrap().len(), 2);
    for (alias, member) in game.catalog().cartridges() {
        let report = &analysis["members"][alias];
        assert_eq!(report["romBytes"], member.bytes.len());
        assert_eq!(report["limits"]["romBytes"], 1024 * 1024);
        assert_eq!(report["manifest"]["id"], member.manifest.id);
        let project = modl_core::unpack_cartridge_project(&member.bytes).unwrap();
        let manifest = modl_core::parse_project_manifest(&project.manifest).unwrap();
        let usage = modl_core::assets::analyze_project_assets(&manifest, &project.files);
        assert_eq!(
            report["assetUsage"],
            serde_json::to_value(usage.usage).unwrap()
        );
    }
    let root = &analysis["members"]["main"];
    assert!(root["assetUsage"]["visualBytes"].as_u64().unwrap() > 0);
    assert_ne!(
        root["assetUsage"]["visualBytes"],
        root["sections"]["visualAssetSourceBytes"]
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn info_profiles_one_real_whole_game_and_reports_unexecuted_members_without_booting_them_independently()
 {
    let path = std::env::temp_dir().join(format!(
        "mod01-info-runtime-{}-{}.m01w",
        std::process::id(),
        line!()
    ));
    let source = game_fixture(3, false);
    fs::write(&path, &source).unwrap();
    let report = info_json(&path, false, false);
    let profile = &report["analysis"]["runtime60Frames"];
    let reference = game_trace(&path, 60, "", None, None);
    assert_eq!(profile["status"], "completed");
    assert_eq!(profile["configuration"], reference["configuration"]);
    assert_eq!(profile["summary"], reference["summary"]);
    assert_eq!(
        profile["memberFrames"],
        json!({"main":20,"level1":20,"level2":20})
    );
    assert_eq!(profile["notProfiledMembers"], json!([]));
    let mut main = modl_core::parse_workcart(&source).unwrap();
    main.files.insert(
        main.manifest.entry.clone(),
        b"on draw:\n  clear(1)\n".to_vec(),
    );
    let mut unused = modl_core::parse_workcart(&main.bundle[0].source).unwrap();
    unused.files.insert(
        unused.manifest.entry.clone(),
        b"on start:\n  assert false, \"unused member must not boot\"\non draw:\n  clear(7)\n"
            .to_vec(),
    );
    main.bundle[0].source =
        modl_core::encode_workcart(&unused.manifest, &unused.files, &[], &[]).unwrap();
    let source = modl_core::workcart::encode_workcart_with_bundle(
        &main.manifest,
        &main.files,
        &[],
        &[],
        &main.bundle,
    )
    .unwrap();
    fs::write(&path, &source).unwrap();
    let report = info_json(&path, false, false);
    let profile = &report["analysis"]["runtime60Frames"];
    assert_eq!(profile["status"], "completed");
    assert_eq!(profile["memberFrames"], json!({"main":60}));
    assert_eq!(profile["notProfiledMembers"], json!(["level1", "level2"]));
    assert_eq!(report["analysis"]["members"].as_object().unwrap().len(), 3);
    fs::remove_file(path).unwrap();
}

#[test]
fn info_keeps_static_evidence_when_node_or_initial_runtime_parameters_are_unavailable() {
    let path = std::env::temp_dir().join(format!(
        "mod01-info-unavailable-{}-{}.m01w",
        std::process::id(),
        line!()
    ));
    let source = game_fixture(2, false);
    fs::write(&path, &source).unwrap();
    let report = info_json(&path, false, true);
    assert_eq!(
        report["analysis"]["runtime60Frames"]["status"],
        "unavailable"
    );
    assert!(
        report["analysis"]["runtime60Frames"]["error"]
            .as_str()
            .unwrap()
            .contains("could not start")
    );
    let report = info_json(&path, false, false);
    assert_eq!(
        report["analysis"]["runtime60Frames"]["status"],
        "unavailable"
    );
    assert!(
        report["analysis"]["runtime60Frames"]["error"]
            .as_str()
            .unwrap()
            .contains("main parameter")
    );
    assert_eq!(report["analysis"]["members"].as_object().unwrap().len(), 2);
    assert!(
        !report["analysis"]["warnings"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    fs::remove_file(path).unwrap();
}

#[test]
fn whole_games_larger_than_one_rom_pack_check_inspect_and_run_identically_from_source_or_bytes() {
    let directory = std::env::temp_dir().join(format!(
        "mod01-whole-game-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir(&directory).unwrap();
    let source = game_fixture(2, true);
    let path = directory.join("cart.m01w");
    let rom = directory.join("game.m01c");
    fs::write(&path, &source).unwrap();
    let game = modl_core::game::pack_game_workcart(&source).unwrap();
    assert!(game.bytes().len() > 1024 * 1024);
    for member in game.catalog().cartridges().values() {
        assert!(member.bytes.len() < 1024 * 1024);
    }
    for command in ["check", "pack"] {
        let mut call = binary();
        call.arg(command).arg(&path).env("PATH", "");
        if command == "pack" {
            call.arg("--output").arg(&rom);
        }
        let result = call.output().unwrap();
        assert!(
            result.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    assert_eq!(fs::read(&rom).unwrap(), game.bytes());
    let inspected = binary()
        .arg("info")
        .arg(&rom)
        .arg("--source")
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let view: Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(view["bundle"][0]["alias"], "arena");
    let trace = directory.join("input.json");
    let saves = directory.join("saves.json");
    fs::write(&trace, serde_json::to_vec(&json!({ "revision": 1, "frames": [{ "frame": 1, "controllers": [], "menuAction": 1 }, { "frame": 3, "controllers": [], "menuAction": 1 }] })).unwrap()).unwrap();
    fs::write(&saves, serde_json::to_vec(&json!({ "cli-game-main": STANDARD.encode(br#"{"visits":4}"#), "cli-game-member-1": STANDARD.encode(br#"{"visits":9}"#) })).unwrap()).unwrap();
    let result = game_trace(&path, 5, "root", Some(&trace), Some(&saves));
    assert_eq!(
        result,
        game_trace(&rom, 5, "root", Some(&trace), Some(&saves))
    );
    assert_eq!(result["game"]["identity"], game.identity());
    assert_eq!(result["revision"], 2);
    assert!(result.get("fault").is_none(), "{result}");
    assert_eq!(
        result["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|frame| frame["cartridgeAlias"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["main", "main", "arena", "arena", "main"]
    );
    assert_eq!(
        result["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|frame| frame["cartridgeFrame"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [0, 1, 0, 1, 0]
    );
    assert_eq!(
        result["game"]["members"]["main"]["bytes"],
        game.entry().bytes.len()
    );
    assert_ne!(
        result["configuration"]["initialSaves"]["cli-game-main"],
        result["summary"]["finalSaves"]["cli-game-main"]
    );
    assert_ne!(
        result["summary"]["finalSaves"]["cli-game-main"],
        result["summary"]["finalSaves"]["cli-game-member-1"]
    );
    // Independently hashed 8192-byte images: compact {"visits":6/10}, then zero padding.
    assert_eq!(
        result["summary"]["finalSaves"]["cli-game-main"],
        "a08902a957e6143c8f899136eb55e113dc1e2e753851c2360120c15858fe5aab"
    );
    assert_eq!(
        result["summary"]["finalSaves"]["cli-game-member-1"],
        "48fc1535e7a2174973efec247f4b8144b47ec04283fe9c040520e7ce9764f664"
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn headless_cycles_all_32_real_cartridges_and_does_not_persist_a_faulted_transition() {
    let directory = std::env::temp_dir().join(format!(
        "mod01-game-cycle-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("cart.m01w");
    fs::write(&path, game_fixture(32, false)).unwrap();
    let result = binary()
        .arg("run")
        .arg(&path)
        .args(["--headless", "--frames", "64"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(result.get("fault").is_none());
    assert_eq!(result["summary"]["completedFrames"], 64);
    assert_eq!(result["game"]["members"].as_object().unwrap().len(), 32);
    for (index, frame) in result["frames"].as_array().unwrap().iter().enumerate() {
        assert_eq!(frame["frame"], index);
        assert_eq!(frame["cartridgeFrame"], 0);
        assert_eq!(
            frame["cartridgeAlias"],
            if index % 32 == 0 {
                "main".to_owned()
            } else {
                format!("level{}", index % 32)
            }
        );
    }
    let images = result["summary"]["finalSaves"].as_object().unwrap();
    assert_eq!(images.len(), 32);
    assert!(images.values().all(|hash| hash == &images["cli-game-main"]));
    assert_eq!(
        images["cli-game-main"],
        "eba2288c6628220918c52fde25fefaf6508e445696d6f9c750d3f497ab733b17"
    ); // visits=2
    fs::write(&path, game_fixture(2, false)).unwrap();
    let trace = directory.join("fault.json");
    fs::write(
        &trace,
        br#"{"revision":1,"frames":[{"frame":1,"controllers":[],"menuAction":3}]}"#,
    )
    .unwrap();
    let result = binary()
        .arg("run")
        .arg(&path)
        .args([
            "--headless",
            "--frames",
            "3",
            "--parameter",
            "root",
            "--input",
        ])
        .arg(trace)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result["fault"]["cartridgeAlias"], "main");
    assert_eq!(result["fault"]["frame"], 1);
    assert_eq!(result["fault"]["cartridgeFrame"], 1);
    assert_eq!(result["frames"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["summary"]["finalSaveSha256"],
        result["frames"][0]["saveSha256"]
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn large_whole_game_png_exports_current_bytes_and_replays_identically_with_transitions() {
    let directory =
        std::env::temp_dir().join(format!("mod01-game-png-{}-{}", std::process::id(), line!()));
    fs::create_dir(&directory).unwrap();
    let source = game_fixture(2, true);
    let path = directory.join("cart.m01w");
    let png = directory.join("game.m01c.png");
    let raw = directory.join("game.m01c");
    fs::write(&path, &source).unwrap();
    let game = modl_core::game::pack_game_workcart(&source).unwrap();
    assert!(game.bytes().len() > 1024 * 1024);
    fs::write(&raw, game.bytes()).unwrap();
    let exported = binary()
        .arg("export")
        .arg("png")
        .arg(&path)
        .arg("--output")
        .arg(&png)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let bytes = fs::read(&png).unwrap();
    assert_eq!(
        bytes,
        modl_core::cartridge_png::encode_game_png(&game).unwrap()
    );
    assert_eq!(
        modl_core::decode_cartridge_png(&bytes).unwrap().cartridge,
        game.bytes()
    );
    let inspected = binary()
        .arg("info")
        .arg(&png)
        .arg("--source")
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let view: Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(view["bundle"][0]["alias"], "arena");
    let trace = directory.join("input.json");
    fs::write(&trace, serde_json::to_vec(&json!({"revision":1,"frames":[{"frame":1,"controllers":[],"menuAction":1},{"frame":3,"controllers":[],"menuAction":1}]})).unwrap()).unwrap();
    let reference = game_trace(&path, 5, "root", Some(&trace), None);
    assert_eq!(reference, game_trace(&raw, 5, "root", Some(&trace), None));
    assert_eq!(reference, game_trace(&png, 5, "root", Some(&trace), None));
    assert_eq!(reference["summary"]["completedFrames"], 5);
    assert!(reference.get("fault").is_none());
    assert_eq!(reference["frames"][2]["cartridgeAlias"], "arena");
    assert_eq!(reference["frames"][4]["cartridgeAlias"], "main");
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn default_distribution_names_are_portable_without_changing_cartridge_identity_or_bytes() {
    let directory = std::env::temp_dir().join(format!(
        "mod01-cli-portable-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir(&directory).unwrap();
    let tool = directory.join("tool");
    let host = directory.join("native-host.mjs");
    let license = directory.join("Node-LICENSE");
    let mut header = [0_u8; 128];
    header[..6].copy_from_slice(b"\x7fELF\x02\x01");
    header[18] = 62;
    fs::write(&tool, header).unwrap();
    fs::write(&host, "process.stdin; // fixture, never executed").unwrap();
    fs::write(&license, "Node license fixture").unwrap();
    for (index, id) in ["con", "con.foo", "aux", "com1", "node", "trailing."]
        .into_iter()
        .enumerate()
    {
        let case = directory.join(format!("case-{index}"));
        fs::create_dir(&case).unwrap();
        let path = case.join("cart.m01w");
        let mut project =
            modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
        id.clone_into(&mut project.manifest.id);
        let source =
            modl_core::encode_workcart(&project.manifest, &project.files, &[], &[]).unwrap();
        fs::write(&path, &source).unwrap();
        let game = modl_core::game::pack_game_workcart(&source).unwrap();
        for command in ["pack", "png", "native"] {
            let mut call = binary();
            if command == "pack" {
                call.arg("pack");
            } else {
                call.args(["export", command]);
            }
            call.arg(&path).env("PATH", "");
            if command == "native" {
                call.arg("--player")
                    .arg(&tool)
                    .arg("--node")
                    .arg(&tool)
                    .arg("--host")
                    .arg(&host)
                    .arg("--node-license")
                    .arg(&license)
                    .args(["--platform", "linux"]);
            }
            let output = call.output().unwrap();
            assert!(
                output.status.success(),
                "{id} / {command}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let dist = case.join("dist");
        let stem = format!("cart-{}", id.replace('.', "_"));
        assert_eq!(
            fs::read(dist.join(format!("{stem}.m01c"))).unwrap(),
            game.bytes()
        );
        let png = fs::read(dist.join(format!("{stem}.m01c.png"))).unwrap();
        assert_eq!(
            modl_core::decode_cartridge_png(&png).unwrap().cartridge,
            game.bytes()
        );
        let native = dist.join(format!("{stem}-native"));
        let metadata: Value =
            serde_json::from_slice(&fs::read(native.join("mod01-game.json")).unwrap()).unwrap();
        assert_eq!(metadata["cartridgeId"], id);
        assert_eq!(fs::read(native.join("game.m01c")).unwrap(), game.bytes());
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn native_export_accepts_identical_source_raw_and_png_games_without_running_node() {
    let directory = std::env::temp_dir().join(format!(
        "mod01-cli-native-game-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir(&directory).unwrap();
    let source = game_fixture(2, true);
    let game = modl_core::game::pack_game_workcart(&source).unwrap();
    let path = directory.join("cart.m01w");
    let raw = directory.join("game.m01c");
    let png = directory.join("game.m01c.png");
    fs::write(&path, &source).unwrap();
    fs::write(&raw, game.bytes()).unwrap();
    fs::write(
        &png,
        modl_core::cartridge_png::encode_game_png(&game).unwrap(),
    )
    .unwrap();
    let mut header = [0_u8; 128];
    header[..6].copy_from_slice(b"\x7fELF\x02\x01");
    header[18] = 62;
    let tool = directory.join("tool");
    let host = directory.join("native-host.mjs");
    let license = directory.join("Node-LICENSE");
    fs::write(&tool, header).unwrap();
    fs::write(&host, "process.stdin; // toolchain fixture, never executed").unwrap();
    fs::write(&license, "Node license fixture").unwrap();
    for (index, input) in [&path, &raw, &png].iter().enumerate() {
        let output = directory.join(format!("native-{index}"));
        let result = binary()
            .args(["export", "native"])
            .arg(input)
            .arg("--output")
            .arg(&output)
            .arg("--player")
            .arg(&tool)
            .arg("--node")
            .arg(&tool)
            .arg("--host")
            .arg(&host)
            .arg("--node-license")
            .arg(&license)
            .args(["--platform", "linux"])
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(fs::read(output.join("game.m01c")).unwrap(), game.bytes());
        let metadata: Value =
            serde_json::from_slice(&fs::read(output.join("mod01-game.json")).unwrap()).unwrap();
        assert_eq!(metadata["gameIdentity"], game.identity());
        assert_eq!(metadata["cartridgeId"], "cli-game-main");
        assert_eq!(
            metadata["members"]["arena"]["cartridgeId"],
            "cli-game-member-1"
        );
        assert_eq!(metadata["members"].as_object().unwrap().len(), 2);
        let inspected = binary()
            .arg("info")
            .arg(output.join("game.m01c"))
            .arg("--source")
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(inspected.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&inspected.stdout).unwrap()["bundle"][0]["alias"],
            "arena"
        );
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    assert_eq!(fs::read(&raw).unwrap(), game.bytes());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn headless_save_catalogs_and_parameters_are_bounded_and_invalid_unused_members_need_no_node() {
    let directory = std::env::temp_dir().join(format!(
        "mod01-game-invalid-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("cart.m01w");
    let saves = directory.join("saves.json");
    fs::write(&path, game_fixture(2, false)).unwrap();
    for value in [
        json!({ "arena": "" }),
        json!({ "cli-game-main": "AR==" }),
        json!({ "cli-game-main": STANDARD.encode(vec![0; 8193]) }),
        json!([]),
    ] {
        fs::write(&saves, serde_json::to_vec(&value).unwrap()).unwrap();
        let result = binary()
            .arg("run")
            .arg(&path)
            .args(["--headless", "--frames", "1", "--saves"])
            .arg(&saves)
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("could not start"));
    }
    let result = binary()
        .arg("run")
        .arg(&path)
        .args(["--headless", "--parameter", &"x".repeat(1025)])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("1024 UTF-8 bytes"));
    let mut project = modl_core::parse_workcart(&fs::read_to_string(&path).unwrap()).unwrap();
    let mut child = modl_core::parse_workcart(&project.bundle[0].source).unwrap();
    child.files.insert(
        child.manifest.entry.clone(),
        b"on draw:\n  absent_function()\n".to_vec(),
    );
    project.bundle[0].source =
        modl_core::encode_workcart(&child.manifest, &child.files, &[], &[]).unwrap();
    fs::write(
        &path,
        modl_core::workcart::encode_workcart_with_bundle(
            &project.manifest,
            &project.files,
            &[],
            &[],
            &project.bundle,
        )
        .unwrap(),
    )
    .unwrap();
    for command in ["check", "pack", "run", "export", "info"] {
        let mut call = binary();
        call.arg(command);
        if command == "export" {
            call.arg("native");
        }
        call.arg(&path).env("PATH", "");
        if command == "run" {
            call.arg("--headless");
        }
        if command == "info" {
            call.arg("--static");
        }
        let result = call.output().unwrap();
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains("arena"), "{command}: {error}");
        assert!(!error.contains("could not start"));
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn game_graphs_are_identical_from_source_raw_and_png_and_keep_member_qualified_draft_errors() {
    let directory = std::env::temp_dir().join(format!(
        "mod01-cli-game-graph-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir(&directory).unwrap();
    let source = game_fixture(2, false);
    let game = modl_core::game::pack_game_workcart(&source).unwrap();
    let source_path = directory.join("cart.m01w");
    let raw_path = directory.join("game.m01c");
    let png_path = directory.join("game.m01c.png");
    fs::write(&source_path, &source).unwrap();
    fs::write(&raw_path, game.bytes()).unwrap();
    fs::write(
        &png_path,
        modl_core::cartridge_png::encode_game_png(&game).unwrap(),
    )
    .unwrap();
    let mut expected = None;
    for path in [&source_path, &raw_path, &png_path] {
        let output = binary()
            .arg("graph")
            .arg(path)
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let graph: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            graph["members"]["arena"]["cartridgeId"],
            "cli-game-member-1"
        );
        assert_eq!(graph["members"].as_object().unwrap().len(), 2);
        assert!(
            graph["members"]["arena"]["model"]["sources"][0]["text"]
                .as_str()
                .unwrap()
                .contains("arena parameter")
        );
        if let Some(expected) = &expected {
            assert_eq!(&graph, expected);
        } else {
            expected = Some(graph);
        }
    }
    let mut main = modl_core::parse_workcart(&source).unwrap();
    let mut child = modl_core::parse_workcart(&main.bundle[0].source).unwrap();
    child.files.insert(
        child.manifest.entry.clone(),
        b"on draw:\n  missing_function()\n".to_vec(),
    );
    main.bundle[0].source =
        modl_core::encode_workcart(&child.manifest, &child.files, &[], &[]).unwrap();
    let invalid = modl_core::workcart::encode_workcart_with_bundle(
        &main.manifest,
        &main.files,
        &[],
        &[],
        &main.bundle,
    )
    .unwrap();
    fs::write(&source_path, &invalid).unwrap();
    let output = binary()
        .arg("graph")
        .arg(&source_path)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let graph: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        graph["members"]["main"]["model"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        !graph["members"]["arena"]["model"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(fs::read_to_string(&source_path).unwrap(), invalid);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn graph_covers_all_32_members_without_node() {
    let path = std::env::temp_dir().join(format!(
        "mod01-graph-32-{}-{}.m01w",
        std::process::id(),
        line!()
    ));
    fs::write(&path, game_fixture(32, false)).unwrap();
    let output = binary()
        .arg("graph")
        .arg(&path)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["members"]
            .as_object()
            .unwrap()
            .len(),
        32
    );
    fs::remove_file(path).unwrap();
}

#[test]
fn graph_is_static_original_source_compiler_data_and_does_not_need_a_runtime() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = binary()
        .arg("graph")
        .arg(root.join("examples/api-tour/cart.m01w"))
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let graph: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(graph["revision"], 1);
    assert_eq!(graph["entry"], "main");
    let model = &graph["members"]["main"]["model"];
    assert_eq!(model["sources"].as_array().unwrap().len(), 2);
    assert!(!model["edges"].as_array().unwrap().is_empty());
    assert!(
        model["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["kind"] == "Callback")
    );
    assert!(model["diagnostics"].as_array().unwrap().is_empty());
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
fn assets_check_and_pack_share_precise_reference_diagnostics_without_needing_node() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut project = modl_core::parse_workcart(
        &fs::read_to_string(root.join("examples/api-tour/cart.m01w")).unwrap(),
    )
    .unwrap();
    let mut map: Value = serde_json::from_slice(&project.files["assets/room.m01m"]).unwrap();
    map["layers"][0]["tileSet"] = json!("beep");
    project.files.insert(
        "assets/room.m01m".to_owned(),
        serde_json::to_vec(&map).unwrap(),
    );
    let mut music: Value = serde_json::from_slice(&project.files["assets/loop.m01t"]).unwrap();
    music["patterns"]["unused"] =
        json!({"rows":[[{"note":60,"sound":"missing_patch"},null,null,null,null,null,null,null]]});
    project.files.insert(
        "assets/loop.m01t".to_owned(),
        serde_json::to_vec(&music).unwrap(),
    );
    let source = modl_core::encode_workcart(
        &project.manifest,
        &project.files,
        &project.recipes,
        &project.tests,
    )
    .unwrap();
    let temporary = std::env::temp_dir().join(format!(
        "mod01-asset-report-{}-{}.m01w",
        std::process::id(),
        line!()
    ));
    fs::write(&temporary, &source).unwrap();
    let report = binary()
        .arg("assets")
        .arg(&temporary)
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(report.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&report.stdout).unwrap();
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 2);
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .all(|diagnostic| diagnostic["code"] == "M014061")
    );
    let check = binary()
        .arg("check")
        .arg(&temporary)
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(check.status.code(), Some(1));
    let diagnostics = String::from_utf8_lossy(&check.stderr);
    assert!(
        diagnostics.contains("assets/room.m01m:$.layers[0].tileSet"),
        "{diagnostics}"
    );
    assert!(diagnostics.contains("missing_patch"), "{diagnostics}");
    let packed = temporary.with_extension("m01c");
    let pack = binary()
        .arg("pack")
        .arg(&temporary)
        .arg("--output")
        .arg(&packed)
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(pack.status.code(), Some(1));
    assert!(!packed.exists());
    assert_eq!(fs::read_to_string(&temporary).unwrap(), source);
    fs::remove_file(&temporary).unwrap();
}

#[test]
fn png_import_is_atomic_source_preserving_explicit_about_replacement_and_reference_safe() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temporary = std::env::temp_dir().join(format!(
        "mod01-png-import-{}-{}",
        std::process::id(),
        line!()
    ));
    fs::create_dir(&temporary).unwrap();
    let input = temporary.join("original.m01w");
    let output = temporary.join("imported.m01w");
    let original = fs::read_to_string(root.join("examples/api-tour/cart.m01w")).unwrap();
    fs::write(&input, &original).unwrap();
    let png = root.join("crates/mod01-native/assets/kenney/tile_0028.png");
    let imported = binary()
        .args(["import", "png"])
        .arg(&input)
        .arg("art")
        .arg(&png)
        .arg("--output")
        .arg(&output)
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    assert_eq!(fs::read_to_string(&input).unwrap(), original);
    let edited = fs::read_to_string(&output).unwrap();
    let project = modl_core::parse_workcart(&edited).unwrap();
    let before = modl_core::parse_workcart(&original).unwrap();
    assert_eq!(project.recipes, before.recipes);
    assert_eq!(project.tests, before.tests);
    for (path, bytes) in &before.files {
        assert_eq!(project.files.get(path), Some(bytes));
    }
    assert!(project.files.contains_key("editor/sprites/art.m01paint"));
    let duplicate = binary()
        .args(["import", "png"])
        .arg(&output)
        .arg("art")
        .arg(&png)
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(duplicate.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("--replace"));
    assert_eq!(fs::read_to_string(&output).unwrap(), edited);
    let repeat = binary()
        .args(["import", "png"])
        .arg(&output)
        .arg("art")
        .arg(&png)
        .arg("--replace")
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        repeat.status.success(),
        "{}",
        String::from_utf8_lossy(&repeat.stderr)
    );
    assert_eq!(fs::read_to_string(&output).unwrap(), edited);
    let occupied = binary()
        .args(["import", "png"])
        .arg(&input)
        .arg("another")
        .arg(&png)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(occupied.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&output).unwrap(), edited);
    assert_png_replacement_is_reference_safe(&output, &png, &temporary, &edited);
    fs::remove_dir_all(temporary).unwrap();
}

fn assert_png_replacement_is_reference_safe(
    output: &Path,
    png: &Path,
    temporary: &Path,
    edited: &str,
) {
    let mut project = modl_core::parse_workcart(edited).unwrap();
    project.files.insert("assets/tiles.m01g".to_owned(),serde_json::to_vec(&json!({"revision":2,"kind":"tile_set","tiles":vec![vec![0;64];5],"flags":[1,2,3,4,5],"memo":"retain"})).unwrap());
    let mut room: Value = serde_json::from_slice(&project.files["assets/room.m01m"]).unwrap();
    room["layers"][0]["cells"][0] = json!(4);
    project.files.insert(
        "assets/room.m01m".to_owned(),
        serde_json::to_vec(&room).unwrap(),
    );
    let valid_before = modl_core::encode_workcart(
        &project.manifest,
        &project.files,
        &project.recipes,
        &project.tests,
    )
    .unwrap();
    modl_core::pack_workcart(&valid_before).unwrap();
    fs::write(output, &valid_before).unwrap();
    let shrink = binary()
        .args(["import", "png"])
        .arg(output)
        .arg("tiles")
        .arg(png)
        .args(["--kind", "tile-set", "--replace"])
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(shrink.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&shrink.stderr).contains("M014061"));
    assert_eq!(fs::read_to_string(output).unwrap(), valid_before);
    room["layers"][0]["cells"][0] = json!(3);
    project.files.insert(
        "assets/room.m01m".to_owned(),
        serde_json::to_vec(&room).unwrap(),
    );
    let valid = modl_core::encode_workcart(
        &project.manifest,
        &project.files,
        &project.recipes,
        &project.tests,
    )
    .unwrap();
    fs::write(output, &valid).unwrap();
    let replaced = binary()
        .args(["import", "png"])
        .arg(output)
        .arg("tiles")
        .arg(png)
        .args(["--kind", "tile-set", "--replace"])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(
        replaced.status.success(),
        "{}",
        String::from_utf8_lossy(&replaced.stderr)
    );
    let successful = fs::read_to_string(output).unwrap();
    let result = modl_core::parse_workcart(&successful).unwrap();
    let tiles: Value = serde_json::from_slice(&result.files["assets/tiles.m01g"]).unwrap();
    assert_eq!(tiles["flags"], json!([1, 2, 3, 4]));
    assert_eq!(tiles["memo"], "retain");
    let mut huge = fs::read(png).unwrap();
    huge[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    let bad = temporary.join("oversized.png");
    fs::write(&bad, huge).unwrap();
    let failed = binary()
        .args(["import", "png"])
        .arg(output)
        .arg("art")
        .arg(&bad)
        .arg("--replace")
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("M014065"));
    assert_eq!(fs::read_to_string(output).unwrap(), successful);
    assert!(!fs::read_dir(temporary).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
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
payload = '''{{"revision":2,"kind":"tile_set","tiles":[[{pixels}]],"flags":[0]}}'''

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
    let packed = modl_core::pack_workcart(&generated).expect("generated current assets pack");
    modl_core::load_cartridge_program(&packed.bytes).expect("generated cartridge loads");
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
payload = '''{{"revision":2,"kind":"sprite","width":1,"height":1,"frames":[[1]],"padding":"{padding}"}}'''
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
    assert_eq!(project_trace["revision"], 2);
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
fn watch_observes_only_the_single_workcart_even_with_output_inside_the_project() {
    struct Running(std::process::Child);
    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let project = std::env::temp_dir().join(format!(
        "mod01-watch-input-{}-{}",
        std::process::id(),
        line!()
    ));
    assert!(
        binary()
            .arg("new")
            .arg(&project)
            .output()
            .unwrap()
            .status
            .success()
    );
    let packed = project.join("live.m01c");
    let mut child = Running(
        binary()
            .arg("watch")
            .arg(&project)
            .arg("--output")
            .arg(&packed)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let output = BufReader::new(child.0.stdout.take().unwrap());
    let (send, receive) = std::sync::mpsc::channel();
    thread::spawn(move || {
        for line in output.lines().map_while(Result::ok) {
            if send.send(line).is_err() {
                break;
            }
        }
    });
    assert!(
        receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .starts_with("packed ")
    );
    assert!(
        receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .starts_with("watching ")
    );
    assert!(
        receive.recv_timeout(Duration::from_millis(350)).is_err(),
        "output must not trigger another pack"
    );
    fs::write(project.join("unrelated.modl"), "on draw:\n  clear(3)\n").unwrap();
    assert!(
        receive.recv_timeout(Duration::from_millis(350)).is_err(),
        "unembedded neighbouring source is not an input"
    );
    let before = fs::read(&packed).unwrap();
    write_project_file(&project, "src/main.modl", "on draw:\n  clear(1)\n").unwrap();
    assert!(
        receive
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .starts_with("packed ")
    );
    assert_ne!(fs::read(&packed).unwrap(), before);
    assert!(
        receive.recv_timeout(Duration::from_millis(350)).is_err(),
        "repacked output must not feed back into watch"
    );
    drop(child);
    fs::remove_dir_all(project).unwrap();
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
fn relay_station_complete_route_swaps_banks_and_returns_with_separate_committed_saves() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = root.join("cartridges/relay-station/cart.m01w");
    let input = root.join("tests/replays/relay-station.json");
    let source = fs::read(&project).unwrap();
    let trace = game_trace(&project, 110, "", Some(&input), None);
    assert!(trace.get("fault").is_none(), "{trace}");
    assert_eq!(trace["summary"]["completedFrames"], 110);
    let frames = trace["frames"].as_array().unwrap();
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame["cartridgeAlias"] == "main")
            .count(),
        6
    );
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame["cartridgeAlias"] == "vault")
            .count(),
        104
    );
    for frame in [2, 106] {
        assert_eq!(frames[frame]["cartridgeFrame"], 0);
    }
    assert_eq!(frames[105]["cartridgeAlias"], "vault");
    assert_eq!(frames[106]["cartridgeAlias"], "main");
    let peak = trace["summary"]["workPeak"].as_u64().unwrap();
    assert!(
        (21_000..=50_000).contains(&peak),
        "bank copies must be charged but stay in budget"
    );
    assert!(trace["summary"]["voicePeak"].as_u64().unwrap() > 0);
    // These saves encode station owner=1/visits=2/clears=1 and vault owner=2/visits=1/clears=1.
    assert_eq!(
        trace["summary"]["finalSaves"]["relay-station"],
        "36bc4828a5766c7cca119e57ad5a38a84d1b9cbb8a139fe7700eb237491316ac"
    );
    assert_eq!(
        trace["summary"]["finalSaves"]["relay-station-vault"],
        "6d4b9e07f8e505b8b6cd82d9ae9e952ec466e5888e1129aaf9438bddf5ad58d3"
    );
    assert_eq!(
        trace["summary"]["finalSaves"]["relay-station-night"],
        trace["configuration"]["initialSaves"]["relay-station-night"]
    );
    assert_eq!(
        trace["summary"]["finalFramebufferSha256"],
        "85eb5f16a9bd4b8941482bf67c2223dc16c1b7391c1360cfec4d9bc488996c51"
    );
    assert_eq!(
        trace["summary"]["pcmSha256"],
        "0be10db162af9fedcf4ec74fcc0cad46007f4976bb59a5ac0a0ffc12a513910f"
    );
    assert_eq!(fs::read(project).unwrap(), source);
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
        "prism-corridor",
        "relay-station",
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
            "signal-4k" | "pocket-relay" | "hardware-gauntlet" | "modl-tutorial" | "relay-station"
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
    assert_eq!(report["analysis"]["revision"], 2);
    assert_eq!(
        report["analysis"]["members"]["main"]["limits"]["saveBytes"],
        8192
    );
    assert_eq!(report["analysis"]["runtime60Frames"]["status"], "faulted");
    assert_eq!(
        report["analysis"]["runtime60Frames"]["summary"]["completedFrames"],
        52
    );
    assert_eq!(
        report["analysis"]["runtime60Frames"]["fault"]["code"],
        "M019002"
    );
    assert!(
        report["analysis"]["members"]["main"]["archive"]["largestEntries"]
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
