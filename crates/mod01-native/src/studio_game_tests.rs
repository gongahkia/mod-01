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
        project.manifest.title = format!("NATIVE {alias}");
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

#[test]
fn studio_native_export_keeps_the_entire_game_and_mains_identity_when_a_child_is_selected() {
    let source = game_source();
    let mut project = NativeProject::from_source(source.clone(), None).unwrap();
    let main_id = project.manifest.id.clone();
    let directory = std::env::temp_dir().join(format!("mod01-native-export-{main_id}"));
    fs::create_dir(&directory).unwrap();
    let mut header = [0_u8; 128];
    header[..6].copy_from_slice(b"\x7fELF\x02\x01");
    header[18] = 62;
    let tool = directory.join("tool");
    let license = directory.join("Node-LICENSE");
    fs::write(&tool, header).unwrap();
    fs::write(&license, "Node license fixture").unwrap();
    let tools = modl_core::NativeToolchain {
        platform: modl_core::NativePlatform::Linux,
        player: tool.clone(),
        node: tool,
        host: native_host_path(),
        node_license: license,
    };
    project.select_member("arena").unwrap();
    project
        .editor_text
        .push_str("// unsaved child edit must travel in the game\n");
    let game = project.game().unwrap();
    let child_id = project.manifest.id.clone();
    let executable = project.export_native(&directory, &tools).unwrap();
    let output = directory.join(format!("cart-{main_id}-native"));
    assert!(executable.starts_with(&output));
    assert_eq!(fs::read(output.join("game.m01c")).unwrap(), game.bytes());
    assert!(!directory.join(format!("cart-{child_id}-native")).exists());
    let metadata: Value =
        serde_json::from_slice(&fs::read(output.join("mod01-game.json")).unwrap()).unwrap();
    assert_eq!(metadata["cartridgeId"], main_id);
    assert_eq!(metadata["gameIdentity"], game.identity());
    assert_eq!(metadata["members"]["arena"]["cartridgeId"], child_id);
    assert_eq!(project.member_alias(), "arena");
    assert!(project.editor_text.contains("// unsaved child edit"));
    assert!(
        project
            .export_native(&directory, &tools)
            .unwrap_err()
            .contains("already exists")
    );
    project.editor_text = "on draw:\n  absent_function()\n".to_owned();
    let invalid = directory.join("invalid");
    let error = project.export_native(&invalid, &tools).unwrap_err();
    assert!(error.contains("arena"), "{error}");
    assert!(!invalid.exists());
    assert_eq!(fs::read(output.join("game.m01c")).unwrap(), game.bytes());
    fs::remove_dir_all(directory).unwrap();
}

fn fixture(debug: bool) -> (StudioApp, egui::Context, BTreeMap<u32, Value>) {
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(None, &context, false);
    let project = NativeProject::from_source(game_source(), None).unwrap();
    app.manifest_text = project.manifest_source.clone();
    app.project = Some(project);
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
fn rejected_debug_requests_preserve_a_real_members_suspended_source_and_input_timeline() {
    let (mut app, context, inputs) = fixture(true);
    advance_debug_to(&mut app, &context, 2);
    for _ in 0..128 {
        app.send_input(&context, "step");
        pump(&mut app, &context);
        if app.debug_booted && app.frame_input.is_some() && app.debug_source.is_some() {
            break;
        }
    }
    assert_eq!(app.input_frame, 2);
    assert_eq!(app.active_alias, "arena");
    assert!(app.debug_booted && !app.debug_frame_boundary && app.frame_input.is_some());
    let state = |app: &StudioApp| {
        json!({
            "alias":app.active_alias, "frame":app.frame, "next":app.input_frame,
            "input":app.frame_input, "source":app.debug_source, "trace":app.debug_trace,
            "inspection":app.debug_inspection,"event":app.debug_event,"watches":app.watch_results,
            "booted":app.debug_booted,"boundary":app.debug_frame_boundary,
            "recording":app.replay_frames,"pixels":app.last_pixels,
        })
    };
    let before = state(&app);
    for command in [
        json!({"type":"restore","snapshot":"AA=="}),
        json!({"type":"memory","address":0x0040_0000,"length":1}),
        json!({"type":"memory-edit","address":0x0010_0000,"bytes":"AA=="}),
        json!({"type":"memory-edit","address":0,"bytes":"@@=="}),
    ] {
        app.runtime.send(command).unwrap();
        app.pending = true;
        pump(&mut app, &context);
        assert!(
            app.status.starts_with("DEBUG REQUEST REJECTED:"),
            "{}",
            app.status
        );
        assert_eq!(state(&app), before);
    }
    advance_debug_to(&mut app, &context, 7);
    assert_eq!(app.last_pixels[0], 1);
    assert_eq!(app.replay_frames, inputs.into_iter().collect::<Vec<_>>());
    app.stop();
}

#[test]
fn losing_audio_output_does_not_stop_or_rewind_the_real_game_or_its_replay_timeline() {
    let (mut app, context, _) = fixture(false);
    for _ in 0..3 {
        app.send_input(&context, "frame");
        pump(&mut app, &context);
    }
    assert_eq!(app.input_frame, 3);
    assert_eq!(app.active_alias, "arena");
    let identity = app.running_members["main"].id.clone();
    app.disable_audio_output("device disconnected during play".to_owned());
    assert!(app.audio.is_none());
    assert_eq!(
        app.audio_notice.as_deref(),
        Some("device disconnected during play")
    );
    assert!(app.loaded && app.running);
    assert_eq!(app.input_frame, 3);
    for _ in 3..7 {
        app.send_input(&context, "frame");
        pump(&mut app, &context);
    }
    assert_eq!(app.input_frame, 7);
    assert_eq!(app.replay_frames.len(), 7);
    assert_eq!(app.active_alias, "main");
    assert_eq!(app.last_pixels[0], 1);
    assert_eq!(app.running_members["main"].id, identity);
    app.stop();
}

#[test]
fn complete_game_crosses_native_release_and_source_step_boundaries_on_one_input_timeline() {
    for debug in [false, true] {
        let (mut app, context, inputs) = fixture(debug);
        app.select_member("arena");
        assert_eq!(app.project.as_ref().unwrap().member_alias(), "arena");
        assert_eq!(app.runtime_title(), "NATIVE main");
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
            assert_eq!(app.runtime_title(), format!("NATIVE {}", app.active_alias));
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
    app.running_members = RunningMember::catalog(&game).unwrap();
    let main_id = app.running_members["main"].id.clone();
    let arena_id = app.running_members["arena"].id.clone();
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

#[test]
fn member_views_preserve_unsaved_source_and_whole_game_saves_without_rewriting_untouched_children()
{
    let mut whole = modl_core::parse_workcart(&game_source()).unwrap();
    whole.bundle[0].source = format!(
        "# exact child comments\r\n{}",
        whole.bundle[0].source.replace('\n', "\r\n")
    );
    let source = format!(
        "# exact enclosing comments\n{}",
        modl_core::workcart::encode_workcart_with_bundle(
            &whole.manifest,
            &whole.files,
            &whole.recipes,
            &whole.tests,
            &whole.bundle
        )
        .unwrap()
    );
    let mut project = NativeProject::from_source(source.clone(), None).unwrap();
    project.select_member("arena").unwrap();
    assert_eq!(project.source().unwrap(), source);
    project.select_member("main").unwrap();
    assert_eq!(project.source().unwrap(), source);
    project.editor_text = project.editor_text.replace("clear(1)", "clear(2)");
    project.dirty = true;
    // No explicit sync_editor: switching must flush the visible editor itself.
    project.select_member("arena").unwrap();
    let after_root = modl_core::parse_workcart(&project.source().unwrap()).unwrap();
    assert_eq!(after_root.bundle[0].source, whole.bundle[0].source);
    assert!(
        String::from_utf8(after_root.files["src/main.modl"].clone())
            .unwrap()
            .contains("clear(2)")
    );
    project.editor_text = project.editor_text.replace("clear(7)", "clear(8)");
    let before_bad_selection = project.source().unwrap();
    assert!(project.select_member("not-present").is_err());
    assert_eq!(project.member_alias(), "arena");
    assert_eq!(project.source().unwrap(), before_bad_selection);
    let main_id = whole.manifest.id.clone();
    assert_eq!(project.game().unwrap().entry().manifest.id, main_id);
    assert_eq!(
        project.load_command(true, &[]).unwrap()["game"]["members"]
            .as_object()
            .unwrap()
            .len(),
        2
    );
    let directory = std::env::temp_dir().join(format!(
        "mod01-member-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    project.workcart_path = Some(directory.join("cart.m01w"));
    project.save().unwrap();
    let saved = NativeProject::open(&directory).unwrap();
    let saved_source = saved.source().unwrap();
    project.select_member("main").unwrap();
    assert!(project.editor_text.contains("clear(2)"));
    assert!(project.dirty);
    project.select_member("arena").unwrap();
    assert!(project.editor_text.contains("clear(8)"));
    // This directory and its one workcart were created solely by this test.
    fs::remove_dir_all(&directory).unwrap();
    let saved = modl_core::parse_workcart(&saved_source).unwrap();
    let saved_child = modl_core::parse_workcart(&saved.bundle[0].source).unwrap();
    assert!(
        String::from_utf8(saved_child.files["src/main.modl"].clone())
            .unwrap()
            .contains("clear(8)")
    );
    // A compiler-invalid draft can still switch and save; it is rejected only by check/run/pack.
    project
        .editor_text
        .push_str("\non update:\n  nonexistent_call()\n");
    project.select_member("main").unwrap();
    assert!(project.game().is_err());
    project.select_member("arena").unwrap();
    assert!(project.editor_text.contains("nonexistent_call"));
}

#[test]
fn studio_member_switches_isolate_identical_asset_paths_bytes_and_redo_history() {
    let mut main =
        modl_core::parse_workcart(include_str!("../../../examples/api-tour/cart.m01w")).unwrap();
    main.manifest.id = "member-history-main".to_owned();
    let music: Value = serde_json::from_slice(&main.files["assets/loop.m01t"]).unwrap();
    main.files.insert(
        "assets/loop.m01t".to_owned(),
        serde_json::to_vec_pretty(&music).unwrap(),
    );
    let mut child = main.clone();
    child.manifest.id = "member-history-arena".to_owned();
    let child =
        modl_core::encode_workcart(&child.manifest, &child.files, &child.recipes, &child.tests)
            .unwrap();
    let source = modl_core::workcart::encode_workcart_with_bundle(
        &main.manifest,
        &main.files,
        &main.recipes,
        &main.tests,
        &[BundleSource {
            alias: "arena".to_owned(),
            source: child,
        }],
    )
    .unwrap();
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(None, &context, false);
    let project = NativeProject::from_source(source, None).unwrap();
    app.manifest_text = project.manifest_source.clone();
    app.project = Some(project);
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    let mut edited = editor.document.clone();
    edited["framesPerRow"] = json!(240);
    editor.replace_document(edited);
    editor.undo();
    app.select_member("arena");
    assert_eq!(
        app.project.as_ref().unwrap().member_alias(),
        "arena",
        "{}",
        app.status
    );
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    editor.redo();
    assert_eq!(
        editor.document["framesPerRow"], 12,
        "another member's redo must never leak"
    );
    let mut edited = editor.document.clone();
    edited["framesPerRow"] = json!(99);
    editor.replace_document(edited);
    app.select_member("main");
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    editor.redo();
    assert_eq!(editor.document["framesPerRow"], 240);
    app.select_member("arena");
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    assert_eq!(editor.document["framesPerRow"], 99);
    editor.undo();
    assert_eq!(editor.document["framesPerRow"], 12);
    app.sync_asset().unwrap();
    app.project.as_ref().unwrap().pack().unwrap();
    app.manifest_text.push_str("\ninvalid = [\n");
    app.select_member("main");
    assert_eq!(app.project.as_ref().unwrap().member_alias(), "arena");
    assert!(app.status.starts_with("APPLY OR REVERT MANIFEST DRAFT"));
    app.manifest_text = app.project.as_ref().unwrap().manifest_source.clone();
    app.asset_workbench = None;
    app.asset_text = "invalid JSON draft".to_owned();
    app.asset_dirty = true;
    app.select_member("main");
    assert_eq!(app.project.as_ref().unwrap().member_alias(), "arena");
    assert_eq!(app.asset_text, "invalid JSON draft");
}

#[test]
fn breakpoints_use_frozen_member_source_and_navigation_never_overwrites_unsaved_edits() {
    let (mut app, context, _) = fixture(true);
    let frozen =
        String::from_utf8(app.running_members["arena"].sources["src/main.modl"].clone()).unwrap();
    let line = frozen
        .lines()
        .position(|line| line.contains("assert cart_parameter()"))
        .unwrap()
        + 1;
    app.breakpoints.push(SourceBreakpoint {
        member: "arena".to_owned(),
        source: "src/main.modl".to_owned(),
        line,
        condition: "arena_ticks == 0".to_owned(),
    });
    app.select_member("arena");
    app.project
        .as_mut()
        .unwrap()
        .editor_text
        .insert_str(0, "// unsaved line changes after launch\n");
    app.project.as_mut().unwrap().dirty = true;
    let edited = app.project.as_ref().unwrap().editor_text.clone();
    app.debug_running = true;
    let mut hit = false;
    for _ in 0..256 {
        app.send_input(&context, "step");
        pump(&mut app, &context);
        if app.active_alias == "main" {
            assert_ne!(app.status, "BREAKPOINT");
        }
        if app.status == "BREAKPOINT" {
            hit = true;
            break;
        }
    }
    assert!(hit, "{}", app.status);
    assert!(!app.debug_running);
    let (alias, path, offset) = app.debug_source.clone().unwrap();
    assert_eq!(alias, "arena");
    assert_eq!(path, "src/main.modl");
    assert_eq!(
        1 + frozen
            .as_bytes()
            .iter()
            .take(offset)
            .filter(|byte| **byte == b'\n')
            .count(),
        line
    );
    app.view = View::Player;
    app.reveal_debug_source();
    assert_eq!(app.project.as_ref().unwrap().editor_text, edited);
    assert!(app.status.starts_with("EDITED SOURCE DIFFERS"));
    assert!(matches!(app.view, View::Player));
    assert_eq!(
        app.running_members["arena"].sources["src/main.modl"],
        frozen.as_bytes()
    );
    app.project.as_mut().unwrap().editor_text = frozen;
    app.select_member("main");
    app.reveal_debug_source();
    assert_eq!(app.project.as_ref().unwrap().member_alias(), "arena");
    assert!(matches!(app.view, View::Code));
    assert_eq!(app.status, "RUNNING SOURCE arena/src/main.modl");
    app.stop();
}

#[test]
fn system_source_inspection_selects_the_active_member_not_the_last_edited_one() {
    let (mut app, context, _) = fixture(false);
    for _ in 0..3 {
        app.send_input(&context, "frame");
        pump(&mut app, &context);
    }
    assert_eq!(app.active_alias, "arena");
    assert_eq!(app.project.as_ref().unwrap().member_alias(), "main");
    app.set_player_paused(true);
    app.system_menu_action(pause_menu::Action::Source);
    assert_eq!(app.project.as_ref().unwrap().member_alias(), "arena");
    assert!(
        app.project
            .as_ref()
            .unwrap()
            .editor_text
            .contains("arena_ticks")
    );
    assert!(!app.running && !app.player_mode);
    assert!(matches!(app.view, View::Code));
    assert_eq!(app.status, "SOURCE INSPECTION / arena / STOPPED");
}

fn composition_app(source: String) -> (StudioApp, egui::Context) {
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(None, &context, false);
    let project = NativeProject::from_source(source, None).unwrap();
    app.manifest_text = project.manifest_source.clone();
    app.project = Some(project);
    (app, context)
}

#[test]
fn studio_imports_exact_local_source_and_packed_members_without_touching_inputs_or_replacing_aliases()
 {
    let (mut app, context) =
        composition_app(include_str!("../../../templates/blank/cart.m01w").to_owned());
    let mut child =
        modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
    child.manifest.id = "native-composition-import".to_owned();
    let source = format!(
        "# exact local source\r\n{}",
        modl_core::encode_workcart(&child.manifest, &child.files, &[], &[])
            .unwrap()
            .replace('\n', "\r\n")
    );
    let packed = modl_core::game::pack_game_workcart(&source)
        .unwrap()
        .into_bytes();
    let directory = std::env::temp_dir().join(format!(
        "mod01-compose-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let workcart = directory.join("local.m01w");
    let cartridge = directory.join("local.m01c");
    fs::write(&workcart, &source).unwrap();
    fs::write(&cartridge, &packed).unwrap();
    app.import_member_path("local", &workcart);
    let project = app.project.as_ref().unwrap();
    assert_eq!(project.member_alias(), "local", "{}", app.status);
    assert_eq!(
        modl_core::parse_workcart(&project.source().unwrap())
            .unwrap()
            .bundle[0]
            .source,
        source
    );
    assert!(project.dirty && project.workcart_path.is_none());
    let before = project.source().unwrap();
    app.import_member_path("local", &cartridge);
    assert_eq!(app.project.as_ref().unwrap().source().unwrap(), before);
    assert!(app.status.contains("M014070"));
    app.add_member_source("nested", &before);
    assert_eq!(app.project.as_ref().unwrap().source().unwrap(), before);
    assert!(app.status.contains("M014073"));
    app.remove_member("main", &context);
    assert_eq!(app.project.as_ref().unwrap().source().unwrap(), before);
    app.remove_member("local", &context);
    assert_eq!(app.project.as_ref().unwrap().members().len(), 1);
    assert_eq!(app.project.as_ref().unwrap().member_alias(), "main");
    app.import_member_path("packed", &cartridge);
    let project = app.project.as_ref().unwrap();
    assert_eq!(project.member_alias(), "packed", "{}", app.status);
    assert_eq!(project.game().unwrap().catalog().cartridges().len(), 2);
    assert_eq!(
        modl_core::parse_workcart(&project.source().unwrap())
            .unwrap()
            .bundle[0]
            .source,
        source
    );
    let source_after = fs::read_to_string(&workcart).unwrap();
    let bytes_after = fs::read(&cartridge).unwrap();
    // The directory and these two fixture files were created solely by this test.
    fs::remove_dir_all(&directory).unwrap();
    assert_eq!(source_after, source);
    assert_eq!(bytes_after, packed);
}

#[test]
fn studio_composition_refuses_running_or_invalid_drafts_and_referenced_removal_transactionally() {
    let (mut app, context) = composition_app(game_source());
    let before = app.project.as_ref().unwrap().source().unwrap();
    app.remove_member("arena", &context);
    assert!(app.status.contains("M014075"));
    assert_eq!(app.project.as_ref().unwrap().source().unwrap(), before);
    app.running = true;
    app.add_blank_member();
    assert!(app.status.starts_with("STOP THE GAME"));
    assert_eq!(app.project.as_ref().unwrap().source().unwrap(), before);
    app.running = false;
    app.manifest_text.push_str("\ninvalid = [\n");
    app.add_blank_member();
    assert!(app.status.starts_with("APPLY OR REVERT MANIFEST DRAFT"));
    assert_eq!(app.project.as_ref().unwrap().source().unwrap(), before);
    app.manifest_text = app.project.as_ref().unwrap().manifest_source.clone();
    app.member_alias_input = "bonus".to_owned();
    app.member_id_input = "native-bonus".to_owned();
    app.add_blank_member();
    assert_eq!(
        app.project.as_ref().unwrap().member_alias(),
        "bonus",
        "{}",
        app.status
    );
    assert_eq!(app.project.as_ref().unwrap().manifest.id, "native-bonus");
    app.remove_member("bonus", &context);
    let project = app.project.as_ref().unwrap();
    assert_eq!(project.member_alias(), "main");
    assert_eq!(project.members().len(), 2);
    assert_eq!(
        modl_core::parse_workcart(&project.source().unwrap())
            .unwrap()
            .bundle,
        modl_core::parse_workcart(&before).unwrap().bundle
    );
}

#[test]
fn removing_and_reimporting_an_alias_cannot_resurrect_deleted_asset_history_or_breakpoints() {
    let (mut app, context) =
        composition_app(include_str!("../../../templates/blank/cart.m01w").to_owned());
    let mut child =
        modl_core::parse_workcart(include_str!("../../../examples/api-tour/cart.m01w")).unwrap();
    child.manifest.id = "composition-history-member".to_owned();
    let child =
        modl_core::encode_workcart(&child.manifest, &child.files, &child.recipes, &child.tests)
            .unwrap();
    app.add_member_source("child", &child);
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    let mut changed = editor.document.clone();
    changed["framesPerRow"] = json!(99);
    editor.replace_document(changed);
    app.breakpoints.push(SourceBreakpoint {
        member: "child".to_owned(),
        source: "src/main.modl".to_owned(),
        line: 1,
        condition: String::new(),
    });
    app.remove_member("child", &context);
    assert!(app.breakpoints.is_empty());
    assert_eq!(
        app.project.as_ref().unwrap().member_alias(),
        "main",
        "{}",
        app.status
    );
    app.add_member_source("child", &child);
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    assert_eq!(editor.document["framesPerRow"], 12);
    editor.undo();
    editor.redo();
    assert_eq!(
        editor.document["framesPerRow"], 12,
        "removed member history must never return"
    );
    app.project.as_ref().unwrap().pack().unwrap();
}

#[test]
fn native_png_export_uses_main_identity_and_large_png_games_boot_and_transition_in_the_actual_host()
{
    let mut whole = modl_core::parse_workcart(&game_source()).unwrap();
    let notes = (0..600_000)
        .map(|index| b'a' + u8::try_from((index * 17 + index / 29) % 26).unwrap())
        .collect::<Vec<_>>();
    whole
        .files
        .insert("authoring/notes.txt".to_owned(), notes.clone());
    let mut arena = modl_core::parse_workcart(&whole.bundle[0].source).unwrap();
    arena.files.insert("authoring/notes.txt".to_owned(), notes);
    whole.bundle[0].source =
        modl_core::encode_workcart(&arena.manifest, &arena.files, &[], &[]).unwrap();
    let source = modl_core::workcart::encode_workcart_with_bundle(
        &whole.manifest,
        &whole.files,
        &[],
        &[],
        &whole.bundle,
    )
    .unwrap();
    let mut project = NativeProject::from_source(source, None).unwrap();
    let game = project.game().unwrap();
    assert!(game.bytes().len() > 1024 * 1024);
    project.select_member("arena").unwrap();
    let (stem, png) = project.cartridge_png().unwrap();
    assert_eq!(stem, game.file_stem());
    assert_eq!(
        png,
        modl_core::cartridge_png::encode_game_png(&game).unwrap()
    );
    assert_eq!(
        modl_core::decode_cartridge_png(&png)
            .unwrap()
            .metadata
            .title,
        "NATIVE main"
    );
    let path = std::env::temp_dir().join(format!(
        "mod01-native-game-png-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, &png).unwrap();
    let reopened = NativeProject::open(&path).unwrap();
    assert_eq!(reopened.pack().unwrap(), game.bytes());
    let bridge = RuntimeBridge::new(native_host_path());
    let loaded = exchange(&bridge, reopened.load_command(false, &[]).unwrap());
    assert_eq!(loaded["type"], "loaded", "{loaded}");
    assert_eq!(loaded["identity"], game.identity());
    let mut transfer = replay::empty_input();
    transfer["menuAction"] = 1.into();
    let old = exchange(&bridge, json!({"type":"frame","input":transfer}));
    assert_eq!(old["cartridgeAlias"], "main");
    assert_eq!(old["active"]["cartridgeAlias"], "arena");
    let child = exchange(
        &bridge,
        json!({"type":"frame","input":replay::empty_input()}),
    );
    assert_eq!(child["cartridgeAlias"], "arena");
    assert_eq!(child["frame"], 1);
    assert_eq!(
        STANDARD.decode(child["pixels"].as_str().unwrap()).unwrap()[0],
        7
    );
    assert_eq!(exchange(&bridge, json!({"type":"stop"}))["type"], "stopped");
    assert_eq!(fs::read(&path).unwrap(), png);
    fs::remove_file(path).unwrap();
}
