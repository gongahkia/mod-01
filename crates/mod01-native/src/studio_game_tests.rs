//! Current workcart → shared Rust preparation → actual Studio/native process ownership.
use super::*;
use modl_core::bundle::BundleSource;
use studio_replay_tests::{advance_debug_to, pump};

fn game_source() -> String {
    let suffix = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let member = |alias: &str, script: &str, rate| {
        let mut project =
            modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
        project.manifest.id = format!("native-game-{alias}-{suffix}");
        project.manifest.update_rate = rate;
        project
            .files
            .insert("src/main.modl".to_owned(), script.as_bytes().to_vec());
        modl_core::encode_workcart(&project.manifest, &project.files, &[], &[]).unwrap()
    };
    let main = member(
        "main",
        "state main_ticks: Int = 0\non start:\n  assert cart_parameter() == \"\" or cart_parameter() == \"returned\", \"main parameter\"\n  save_set_int(\"visits\", save_get_int(\"visits\", 0) + 1)\n  save_commit()\n  menu_item(1, \"ARENA\")\non menu(action: Int):\n  load_cart(\"arena\", \"score=7\")\non update:\n  main_ticks += 1\non draw:\n  clear(1)\n",
        60,
    );
    let arena = member(
        "arena",
        "state arena_ticks: Int = 0\non start:\n  assert cart_parameter() == \"score=7\", \"arena parameter\"\n  save_set_int(\"visits\", save_get_int(\"visits\", 0) + 1)\n  save_commit()\n  menu_item(1, \"RETURN\")\non menu(action: Int):\n  load_cart(\"main\", \"returned\")\non update:\n  arena_ticks += 1\non draw:\n  clear(7)\n  if btnp(pad1, a):\n    clear(9)\n",
        30,
    );
    let project = modl_core::parse_workcart(&main).unwrap();
    modl_core::workcart::encode_workcart_with_bundle(
        &project.manifest,
        &project.files,
        &[],
        &[],
        &[BundleSource {
            alias: "arena".to_owned(),
            source: arena,
        }],
    )
    .unwrap()
}

fn fixture(debug: bool) -> (StudioApp, egui::Context, BTreeMap<u32, Value>) {
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(None, &context, false);
    app.project = Some(NativeProject::from_source(game_source(), None).unwrap());
    let mut held = replay::empty_input();
    held["controllers"][0]["buttons"]["a"] = true.into();
    let mut transfer = held.clone();
    transfer["menuAction"] = 1.into();
    let mut back = replay::empty_input();
    back["menuAction"] = 1.into();
    let inputs = BTreeMap::from([
        (0, replay::empty_input()),
        (1, transfer),
        (2, held.clone()),
        (3, replay::empty_input()),
        (4, held),
        (5, back),
        (6, replay::empty_input()),
    ]);
    app.replay_playback = Some(inputs.clone());
    app.recording_replay = true;
    app.run_mode(debug);
    pump(&mut app, &context);
    (app, context, inputs)
}

#[test]
fn complete_game_crosses_native_release_and_source_step_boundaries_on_one_input_timeline() {
    for debug in [false, true] {
        let (mut app, context, inputs) = fixture(debug);
        for frame in 0..7 {
            if debug {
                advance_debug_to(&mut app, &context, frame + 1);
            } else {
                app.send_input(&context, "frame");
                pump(&mut app, &context);
            }
            assert_eq!(app.frame, frame);
            assert_eq!(app.input_frame, frame + 1);
            assert_eq!(
                app.last_pixels[0],
                match frame {
                    2 | 3 | 5 => 7,
                    4 => 9,
                    _ => 1,
                }
            );
            assert_eq!(
                app.active_alias,
                if (1..5).contains(&frame) {
                    "arena"
                } else {
                    "main"
                }
            );
            if frame == 1 || frame == 5 {
                assert!(!app.debug_booted, "target boot is deferred in both modes");
                assert!(app.frame_input.is_none());
            }
        }
        assert_eq!(app.replay_frames, inputs.into_iter().collect::<Vec<_>>());
        assert_eq!(app.running_members.len(), 2);
        if debug {
            assert!(
                app.debug_trace
                    .iter()
                    .any(|event| event["cartridgeAlias"] == "arena"
                        && event["sourceSpan"]["source"] == "src/main.modl")
            );
        }
        app.stop();
    }
}

#[test]
fn native_game_checkpoint_restores_alias_and_global_input_without_recording_target_boot() {
    let (mut app, context, _) = fixture(true);
    advance_debug_to(&mut app, &context, 2);
    assert_eq!(app.active_alias, "arena");
    assert!(!app.debug_booted);
    app.runtime.send(json!({"type":"snapshot"})).unwrap();
    app.pending = true;
    pump(&mut app, &context);
    let snapshot = app.snapshot.clone().unwrap();
    advance_debug_to(&mut app, &context, 7);
    app.runtime
        .send(json!({"type":"restore","snapshot":snapshot}))
        .unwrap();
    app.pending = true;
    pump(&mut app, &context);
    assert_eq!(app.active_alias, "arena");
    assert!(!app.debug_booted);
    assert_eq!(app.input_frame, 2);
    assert_eq!(app.replay_frames.len(), 2);
    advance_debug_to(&mut app, &context, 5);
    assert_eq!(app.last_pixels[0], 9);
    assert_eq!(app.replay_frames.len(), 5);
    app.stop();
}

#[test]
fn frame_commit_owner_is_the_admitted_producer_not_the_editor_or_next_active_member() {
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(None, &context, false);
    app.project = Some(NativeProject::from_source(game_source(), None).unwrap());
    let game = app.project.as_ref().unwrap().game().unwrap();
    app.running_members = game
        .catalog()
        .cartridges()
        .iter()
        .map(|(alias, cart)| (alias.clone(), cart.manifest.id.clone()))
        .collect();
    let main_id = app.running_members["main"].clone();
    let arena_id = app.running_members["arena"].clone();
    let main_path = save_path(&main_id).unwrap();
    let arena_path = save_path(&arena_id).unwrap();
    assert!(!main_path.exists() && !arena_path.exists());
    let bytes = vec![7; 8192];
    // Editing the root ID after launch must not move the running game's save namespace.
    app.project.as_mut().unwrap().manifest.id = "edited-not-running".to_owned();
    app.accept_frame(&json!({"frame":3,"cartridgeAlias":"arena","cartridgeId":arena_id,"workUnits":1,"saveCommit":STANDARD.encode(&bytes),"active":{"cartridgeAlias":"main","cartridgeId":main_id,"booted":false}}), &context);
    let stored = fs::read(&arena_path);
    let root_wrote = main_path.exists();
    // Only the uniquely named file created by this test is removed.
    if arena_path.exists() {
        fs::remove_file(&arena_path).unwrap();
    }
    assert_eq!(stored.unwrap(), bytes);
    assert!(!root_wrote);
    assert_eq!(app.active_alias, "main");
    assert!(!app.debug_booted);
    app.accept_frame(&json!({"frame":4,"cartridgeAlias":"main","cartridgeId":arena_id,"saveCommit":STANDARD.encode(&bytes)}), &context);
    assert_eq!(app.status, "INVALID RUNTIME FRAME OWNER");
    assert!(!arena_path.exists() && !main_path.exists());
}

#[test]
fn whole_native_projects_round_trip_packed_source_and_reject_invalid_unused_members() {
    let source = game_source();
    let project = NativeProject::from_source(source.clone(), None).unwrap();
    let packed = project.pack().unwrap();
    let game = modl_core::game::decode_game(&packed).unwrap();
    assert_eq!(game.source(), source);
    let command = project.load_command(true, &[]).unwrap();
    assert_eq!(command["game"]["members"].as_object().unwrap().len(), 2);
    assert_eq!(command["game"]["identity"], game.identity());
    assert_ne!(
        command["game"]["members"]["main"]["sourceMap"],
        command["game"]["members"]["arena"]["sourceMap"]
    );
    let mut broken = modl_core::parse_workcart(&source).unwrap();
    broken.bundle[0].source = broken.bundle[0]
        .source
        .replace("clear(7)", "clear(no_such_value)");
    let broken = modl_core::workcart::encode_workcart_with_bundle(
        &broken.manifest,
        &broken.files,
        &[],
        &[],
        &broken.bundle,
    )
    .unwrap();
    let broken = NativeProject::from_source(broken, None).unwrap();
    assert!(
        broken
            .load_command(false, &[])
            .unwrap_err()
            .contains("arena")
    );
    assert!(
        NativeProject::game_command(
            &game,
            false,
            &BTreeMap::from([("unknown".to_owned(), vec![])])
        )
        .is_err()
    );
}

fn exchange(bridge: &RuntimeBridge, command: Value) -> Value {
    bridge.send(command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(7);
    loop {
        if let Some(event) = bridge.poll() {
            return event;
        }
        assert!(Instant::now() < deadline, "native game reply timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn native_debug_watches_use_member_symbols_even_when_source_paths_and_symbol_ids_overlap() {
    let project = NativeProject::from_source(game_source(), None).unwrap();
    let bridge = RuntimeBridge::new(native_host_path());
    assert_eq!(
        exchange(&bridge, project.load_command(true, &[]).unwrap())["type"],
        "loaded"
    );
    let mut main = exchange(
        &bridge,
        json!({"type":"step","input":replay::empty_input(),"watches":["main_ticks","arena_ticks"]}),
    );
    for _ in 0..100 {
        if main["watches"][0]["value"] == 0 {
            break;
        }
        main = exchange(
            &bridge,
            json!({"type":"step","input":replay::empty_input(),"watches":["main_ticks","arena_ticks"]}),
        );
    }
    assert_eq!(main["cartridgeAlias"], "main");
    assert_eq!(main["event"]["sourceSpan"]["source"], "src/main.modl");
    assert_eq!(main["watches"][0]["value"], 0, "{main}");
    assert!(main["watches"][1]["error"].is_string());
    for _ in 0..100 {
        let next = exchange(
            &bridge,
            json!({"type":"step","input":replay::empty_input()}),
        );
        if next["booted"] == true {
            break;
        }
        assert_eq!(next["type"], "step", "{next}");
    }
    let mut transfer = replay::empty_input();
    transfer["menuAction"] = 1.into();
    let old = exchange(&bridge, json!({"type":"frame","input":transfer}));
    assert_eq!(old["cartridgeAlias"], "main");
    assert_eq!(old["active"]["cartridgeAlias"], "arena");
    assert_eq!(old["active"]["booted"], false);
    let mut arena = exchange(
        &bridge,
        json!({"type":"step","input":replay::empty_input(),"watches":["arena_ticks","main_ticks"],"conditions":["arena_ticks == 0"]}),
    );
    for _ in 0..100 {
        if arena["watches"][0]["value"] == 0 {
            break;
        }
        arena = exchange(
            &bridge,
            json!({"type":"step","input":replay::empty_input(),"watches":["arena_ticks","main_ticks"],"conditions":["arena_ticks == 0"]}),
        );
    }
    assert_eq!(arena["cartridgeAlias"], "arena");
    assert_eq!(arena["event"]["sourceSpan"]["source"], "src/main.modl");
    assert_eq!(arena["watches"][0]["value"], 0);
    assert!(arena["watches"][1]["error"].is_string());
    assert_eq!(arena["conditions"][0]["value"], true);
    assert_eq!(exchange(&bridge, json!({"type":"stop"}))["type"], "stopped");
}

#[test]
fn studio_preloads_all_admitted_disk_saves_and_playback_cannot_overwrite_them() {
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(None, &context, false);
    let mut project = modl_core::parse_workcart(&game_source()).unwrap();
    let assert_initial = |source: &mut modl_core::WorkcartProject, count| {
        let script = String::from_utf8(source.files["src/main.modl"].clone()).unwrap();
        source.files.insert("src/main.modl".to_owned(), script.replace("on start:\n", &format!("on start:\n  assert save_get_int(\"visits\", 0) == {count}, \"all disk saves preloaded\"\n")).into_bytes());
    };
    assert_initial(&mut project, 4);
    let mut arena = modl_core::parse_workcart(&project.bundle[0].source).unwrap();
    assert_initial(&mut arena, 9);
    project.bundle[0].source =
        modl_core::encode_workcart(&arena.manifest, &arena.files, &[], &[]).unwrap();
    let source = modl_core::workcart::encode_workcart_with_bundle(
        &project.manifest,
        &project.files,
        &[],
        &[],
        &project.bundle,
    )
    .unwrap();
    app.project = Some(NativeProject::from_source(source, None).unwrap());
    let game = app.project.as_ref().unwrap().game().unwrap();
    let main_id = &game.entry().manifest.id;
    let arena_id = &game.catalog().cartridges()["arena"].manifest.id;
    let main_path = save_path(main_id).unwrap();
    let arena_path = save_path(arena_id).unwrap();
    assert!(!main_path.exists() && !arena_path.exists());
    let initial_main = br#"{"visits":4}"#;
    let initial_arena = br#"{"visits":9}"#;
    write_save(main_id, initial_main).unwrap();
    write_save(arena_id, initial_arena).unwrap();
    let mut transfer = replay::empty_input();
    transfer["menuAction"] = 1.into();
    app.replay_playback = Some(BTreeMap::from([(1, transfer)]));
    app.run_mode(false);
    pump(&mut app, &context);
    for _ in 0..3 {
        app.send_input(&context, "frame");
        pump(&mut app, &context);
    }
    let disk_main = fs::read(&main_path);
    let disk_arena = fs::read(&arena_path);
    app.stop();
    // Both paths contain unique IDs created above solely for this test.
    fs::remove_file(&main_path).unwrap();
    fs::remove_file(&arena_path).unwrap();
    assert_eq!(app.active_alias, "arena");
    assert_eq!(app.last_pixels[0], 7);
    assert_eq!(disk_main.unwrap(), initial_main);
    assert_eq!(disk_arena.unwrap(), initial_arena);
}
