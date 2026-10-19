//! Exercise Studio's actual input submission/acknowledgement timeline, not just its JSON codec.
use super::*;

fn fixture(debug: bool) -> (StudioApp, egui::Context, BTreeMap<u32, Value>) {
    let mut project =
        modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
    project.manifest.id = "native-replay-conformance".to_owned();
    project.files.insert("src/main.modl".to_owned(),b"state hits: Int = 0\n\non update:\n  if btnp(pad1, a):\n    hits += 1\n\non draw:\n  clear(0)\n  if hits == 1:\n    pixel(0, 0, 7)\n  if hits == 2:\n    pixel(0, 0, 9)\n".to_vec());
    let source = modl_core::encode_workcart(
        &project.manifest,
        &project.files,
        &project.recipes,
        &project.tests,
    )
    .unwrap();
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(None, &context, false);
    app.project = Some(NativeProject::from_source(source, None).unwrap());
    let mut pressed = replay::empty_input();
    pressed["controllers"][0]["buttons"]["a"] = true.into();
    let inputs = BTreeMap::from([
        (0, pressed.clone()),
        (1, replay::empty_input()),
        (2, pressed),
    ]);
    app.replay_playback = Some(inputs.clone());
    app.recording_replay = true;
    app.run_mode(debug);
    pump(&mut app, &context);
    (app, context, inputs)
}

fn pump(app: &mut StudioApp, context: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(7);
    while app.pending {
        app.process_events(context);
        assert!(Instant::now() < deadline, "{}", app.status);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(app.running && app.loaded, "{}", app.status);
    context
        .run_ui(egui::RawInput::default(), |_| {})
        .drop_without_applying_deltas();
}

fn advance_debug_to(app: &mut StudioApp, context: &egui::Context, frame: u32) {
    let mut steps = 0;
    while app.input_frame < frame {
        app.send_input(context, "step");
        pump(app, context);
        steps += 1;
        assert!(steps < 256, "debugger failed to reach frame {frame}");
    }
}

#[test]
fn release_playback_uses_frame_zero_once_and_records_each_accepted_frame_once() {
    let (mut app, context, inputs) = fixture(false);
    for frame in 0..3 {
        assert_eq!(app.input_frame, frame);
        app.send_input(&context, "frame");
        pump(&mut app, &context);
        assert_eq!(app.last_pixels[0], if frame < 2 { 7 } else { 9 });
    }
    assert_eq!(app.replay_frames, inputs.into_iter().collect::<Vec<_>>());
    assert_eq!(
        replay::decode(&replay::encode(&app.replay_frames).unwrap())
            .unwrap()
            .len(),
        3
    );
    app.stop();
}

#[test]
fn starting_recording_restarts_the_machine_and_does_not_record_loaded_playback() {
    let (mut app, context, _) = fixture(false);
    app.send_input(&context, "frame");
    pump(&mut app, &context);
    assert_eq!(app.last_pixels[0], 7);
    app.start_replay_recording();
    pump(&mut app, &context);
    assert_eq!(app.input_frame, 0);
    assert!(app.replay_playback.is_none());
    assert!(app.replay_frames.is_empty());
    assert!(app.recording_replay);
    app.send_input(&context, "frame");
    pump(&mut app, &context);
    assert_eq!(app.last_pixels[0], 0);
    assert_eq!(app.replay_frames[0], (0, replay::empty_input()));
    app.stop();
}

#[test]
fn source_steps_lock_one_input_for_the_whole_frame_and_do_not_record_boot_or_statement_duplicates()
{
    let (mut app, context, inputs) = fixture(true);
    let mut changed = false;
    let mut steps = 0;
    while app.input_frame < 3 {
        app.send_input(&context, "step");
        pump(&mut app, &context);
        steps += 1;
        assert!(steps < 256);
        if !changed && app.input_frame == 0 && app.frame_input.is_some() {
            app.replay_playback
                .as_mut()
                .unwrap()
                .insert(0, replay::empty_input());
            changed = true;
        }
    }
    assert!(changed);
    assert!(steps > 3);
    assert_eq!(app.last_pixels[0], 9);
    assert_eq!(app.replay_frames, inputs.into_iter().collect::<Vec<_>>());
    assert!(app.debug_frame_boundary);
    app.stop();
}

#[test]
fn restoring_a_real_snapshot_rewinds_input_and_recording_to_its_authoritative_next_frame() {
    let (mut app, context, _) = fixture(true);
    advance_debug_to(&mut app, &context, 3);
    app.runtime.send(json!({"type":"snapshot"})).unwrap();
    app.pending = true;
    pump(&mut app, &context);
    let snapshot = app.snapshot.clone().unwrap();
    assert_eq!(app.debug_snapshots.last().unwrap().0, 3);
    advance_debug_to(&mut app, &context, 4);
    assert_eq!(app.replay_frames.len(), 4);
    app.runtime
        .send(json!({"type":"restore","snapshot":snapshot}))
        .unwrap();
    app.pending = true;
    pump(&mut app, &context);
    assert_eq!(app.input_frame, 3);
    assert!(app.debug_booted && app.debug_frame_boundary);
    assert_eq!(app.replay_frames.len(), 3);
    assert!(app.frame_input.is_none());
    advance_debug_to(&mut app, &context, 4);
    assert_eq!(app.last_pixels[0], 9);
    assert_eq!(
        replay::decode(&replay::encode(&app.replay_frames).unwrap())
            .unwrap()
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    app.stop();
}

#[test]
fn debug_and_replay_commits_are_disposable_but_ordinary_play_still_persists() {
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(None, &context, false);
    let mut project = NativeProject::from_source(
        include_str!("../../../templates/blank/cart.m01w").to_owned(),
        None,
    )
    .unwrap();
    project.manifest.id = format!(
        "save-isolation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = save_path(&project.manifest.id).unwrap();
    assert!(!path.exists());
    app.project = Some(project);
    let bytes = vec![57_u8; 8192];
    let reply = json!({"frame":0,"workUnits":1,"saveCommit":STANDARD.encode(&bytes)});
    app.debugging = true;
    app.accept_frame(&reply, &context);
    let debug_wrote = path.exists();
    app.debugging = false;
    app.replay_playback = Some(BTreeMap::new());
    app.accept_frame(&reply, &context);
    let replay_wrote = path.exists();
    app.replay_playback = None;
    app.accept_frame(&reply, &context);
    let normal = fs::read(&path);
    // This unique path was absent before the test and is exclusively test-owned.
    if path.exists() {
        fs::remove_file(&path).unwrap();
    }
    assert!(!debug_wrote, "debugging must not persist cartridge saves");
    assert!(
        !replay_wrote,
        "loaded replay must not persist cartridge saves"
    );
    assert_eq!(normal.unwrap(), bytes);
}
