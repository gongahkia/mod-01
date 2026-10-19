//! Real Studio → packed source → Node callback/input/replay tests without physical devices.
use super::*;
use pause_menu::Action;
use studio_replay_tests::{advance_debug_to, pump};

fn fixture(debug: bool) -> (StudioApp, egui::Context) {
    let mut project =
        modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
    project.manifest.id = "native-menu-conformance".to_owned();
    project.files.insert("src/main.modl".to_owned(), b"state marked: Bool = false\non start:\n  menu_item(1, \"MARK FRAME\")\non menu(action: Int):\n  assert action == 1, \"slot\"\n  assert not btn(pad1, a), \"no selection button\"\n  assert not pointer_primary(), \"no selection pointer\"\n  marked = true\n  menu_item(1, \"MARKED\")\non draw:\n  clear(0)\n  if marked:\n    pixel(0, 0, 7)\n".to_vec());
    let source = modl_core::encode_workcart(
        &project.manifest,
        &project.files,
        &project.recipes,
        &project.tests,
    )
    .unwrap();
    let original = NativeProject::from_source(source, None).unwrap();
    let packed = original.pack().unwrap();
    modl_core::load_cartridge_program(&packed).unwrap();
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(None, &context, false);
    app.project = Some(
        NativeProject::from_source(modl_core::unpack_workcart(&packed).unwrap(), None).unwrap(),
    );
    app.recording_replay = true;
    app.run_mode(debug);
    pump(&mut app, &context);
    if debug {
        advance_debug_to(&mut app, &context, 1);
    } else {
        app.send_input(&context, "frame");
        pump(&mut app, &context);
    }
    assert_eq!(app.last_pixels[0], 0);
    assert!(app.pause_menu.has_action(1));
    (app, context)
}

#[test]
fn common_menu_freezes_real_release_and_debug_machines_and_records_one_neutral_action() {
    for debug in [false, true] {
        let (mut app, context) = fixture(debug);
        let files = app.project.as_ref().unwrap().files.clone();
        app.debug_running = debug;
        app.set_player_paused(true);
        assert!(!app.debug_running);
        for _ in 0..12 {
            app.last_tick = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
            app.advance_runtime(&context);
            app.process_events(&context);
            assert!(!app.pending && !app.runtime.has_pending());
            assert_eq!(app.input_frame, 1);
        }
        app.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(240.0, 144.0),
        ));
        context
            .run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(egui::pos2(1.0, 1.0)),
                        egui::Event::PointerButton {
                            pos: egui::pos2(1.0, 1.0),
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |_| {},
            )
            .drop_without_applying_deltas();
        app.system_menu_action(Action::Cartridge(1));
        assert!(!app.player_paused);
        assert_eq!(app.debug_running, debug);
        if debug {
            advance_debug_to(&mut app, &context, 2);
        } else {
            app.send_input(&context, "frame");
            pump(&mut app, &context);
        }
        let mut expected = replay::empty_input();
        expected["menuAction"] = 1.into();
        assert_eq!(app.replay_frames[1], (1, expected));
        assert_eq!(app.last_pixels[0], 7);
        assert!(app.pending_menu_action.is_none());
        let recorded = app.replay_frames.clone();
        let pixels = app.last_pixels.clone();
        app.recording_replay = false;
        app.replay_playback = Some(replay::decode(&replay::encode(&recorded).unwrap()).unwrap());
        app.run_mode(false);
        pump(&mut app, &context);
        for _ in 0..2 {
            app.send_input(&context, "frame");
            pump(&mut app, &context);
        }
        assert_eq!(app.last_pixels, pixels);
        assert_eq!(app.project.as_ref().unwrap().files, files);
        app.stop();
    }
}

#[test]
fn pending_source_step_and_loaded_replay_cannot_accept_a_live_menu_action() {
    let (mut app, context) = fixture(true);
    app.send_input(&context, "step");
    app.set_player_paused(true);
    app.system_menu_action(Action::Cartridge(1));
    assert!(app.player_paused && app.pending_menu_action.is_none());
    pump(&mut app, &context);
    assert!(!app.debug_frame_boundary);
    app.system_menu_action(Action::Cartridge(1));
    assert!(app.player_paused && app.pending_menu_action.is_none());
    app.set_player_paused(false);
    advance_debug_to(&mut app, &context, 2);
    app.replay_playback = Some(BTreeMap::new());
    app.set_player_paused(true);
    app.system_menu_action(Action::Cartridge(1));
    assert!(app.player_paused && app.pending_menu_action.is_none());
    let files = app.project.as_ref().unwrap().files.clone();
    app.system_menu_action(Action::Source);
    assert!(!app.running && !app.player_mode && app.view == View::Code);
    assert_eq!(app.status, "SOURCE INSPECTION / main / STOPPED");
    assert_eq!(app.project.as_ref().unwrap().files, files);
}

#[test]
fn escape_routes_through_the_same_host_pause_state_without_recording_menu_navigation() {
    let (mut app, context) = fixture(false);
    for paused in [true, false] {
        context
            .run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: false,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    app.capture_keyboard(ui.ctx());
                    app.handle_system_menu_toggle(ui.ctx());
                },
            )
            .drop_without_applying_deltas();
        assert_eq!(app.player_paused, paused);
        assert_eq!(app.input_frame, 1);
        assert_eq!(app.replay_frames.len(), 1);
    }
    app.stop();
}

#[test]
fn a_breakpoint_in_an_already_submitted_step_cannot_be_resumed_by_closing_the_menu() {
    let (mut app, context) = fixture(true);
    let project = app.project.as_ref().unwrap();
    let line = project
        .editor_text
        .lines()
        .position(|line| line == "  clear(0)")
        .unwrap()
        + 1;
    app.breakpoints.push(SourceBreakpoint {
        member: "main".to_owned(),
        source: project.manifest.entry.clone(),
        line,
        condition: String::new(),
    });
    app.debug_running = true;
    app.send_input(&context, "step");
    app.set_player_paused(true);
    assert!(app.resume_debug);
    pump(&mut app, &context);
    assert_eq!(app.status, "BREAKPOINT");
    assert!(!app.resume_debug && !app.debug_running);
    app.system_menu_action(Action::Resume);
    app.advance_runtime(&context);
    assert!(!app.player_paused && !app.pending && !app.debug_running);
    assert_eq!(app.input_frame, 1);
    app.stop();
}

#[test]
fn a_fault_in_an_already_submitted_frame_closes_pause_and_keeps_the_actual_error_visible() {
    let (mut app, context) = fixture(false);
    app.project.as_mut().unwrap().editor_text = "state count: Int = 0\non update:\n  count += 1\n  if count == 2:\n    assert false, \"fault while paused\"\non draw:\n  clear(0)\n".to_owned();
    app.run_mode(false);
    pump(&mut app, &context);
    app.send_input(&context, "frame");
    pump(&mut app, &context);
    app.send_input(&context, "frame");
    app.set_player_paused(true);
    let deadline = Instant::now() + Duration::from_secs(7);
    while app.pending {
        app.process_events(&context);
        assert!(Instant::now() < deadline, "{}", app.status);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!app.running && !app.loaded && !app.player_paused && !app.debug_running);
    assert!(app.pending_menu_action.is_none());
    assert!(app.status.contains("fault while paused"), "{}", app.status);
    assert_eq!(app.input_frame, 1);
    app.stop();
}
