use super::*;

#[test]
fn empty_workcarts_can_author_current_assets_that_pack_through_shared_validation() {
    let context = egui::Context::default();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates/blank/cart.m01w");
    let original = fs::read(&path).unwrap();
    let mut app = StudioApp::new_with_devices(Some(path.clone()), &context, false);
    for (name, kind) in [
        ("tiles", modl_core::AssetKind::TileSet),
        ("map", modl_core::AssetKind::Map),
        ("font", modl_core::AssetKind::Font),
        ("sound", modl_core::AssetKind::Sound),
        ("music", modl_core::AssetKind::Music),
    ] {
        app.new_asset_name = name.to_owned();
        app.new_asset_kind = kind;
        app.create_asset();
        assert!(
            app.project
                .as_ref()
                .unwrap()
                .manifest
                .assets
                .contains_key(name),
            "{}",
            app.status
        );
    }
    app.sprite_name = "hero".to_owned();
    app.create_sprite();
    assert!(
        app.project
            .as_ref()
            .unwrap()
            .manifest
            .assets
            .contains_key("hero"),
        "{}",
        app.status
    );
    app.open_display();
    app.sync_asset().unwrap();
    let project = app.project.as_ref().unwrap();
    assert!(
        modl_core::assets::analyze_project_assets(&project.manifest, &project.files).is_valid()
    );
    project.pack().unwrap();
    assert_eq!(fs::read(path).unwrap(), original);
    context
        .run_ui(egui::RawInput::default(), |_| {})
        .drop_without_applying_deltas();
}

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/api-tour/cart.m01w")
}

#[test]
fn studio_switches_music_sound_sprite_and_display_without_losing_asset_undo_or_writing_disk() {
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(Some(example()), &context, false);
    let original_disk = fs::read(example()).unwrap();
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    let mut document = editor.document.clone();
    document["framesPerRow"] = json!(240);
    editor.replace_document(document);
    app.open_asset("beep");
    let music: Value =
        serde_json::from_slice(&app.project.as_ref().unwrap().files["assets/loop.m01t"]).unwrap();
    assert_eq!(music["framesPerRow"], 240);
    app.open_asset("dot");
    let editor = app.sprite.as_mut().unwrap();
    let original_pixels = editor.document.frames[0].layers[0].pixels.clone();
    editor.remember();
    editor.document.frames[0].layers[0].pixels[0] ^= 255;
    editor.dirty = true;
    let painted = editor.document.frames[0].layers[0].pixels.clone();
    app.open_display();
    assert!(app.sprite.is_none());
    assert_eq!(
        app.asset_workbench.as_ref().unwrap().document["remap"],
        json!((0..32).collect::<Vec<_>>())
    );
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    assert_eq!(editor.document["framesPerRow"], 240);
    editor.undo();
    assert_eq!(editor.document["framesPerRow"], 12);
    app.sync_asset().unwrap();
    assert!(!app.asset_workbench.as_ref().unwrap().dirty);
    app.project.as_mut().unwrap().dirty = false;
    app.sync_asset().unwrap();
    assert!(!app.project.as_ref().unwrap().dirty);
    app.open_asset("dot");
    let editor = app.sprite.as_mut().unwrap();
    assert_eq!(editor.document.frames[0].layers[0].pixels, painted);
    editor.undo();
    assert_eq!(editor.document.frames[0].layers[0].pixels, original_pixels);
    assert_eq!(fs::read(example()).unwrap(), original_disk);
    let generation = app.asset_sessions.generation();
    app.open(example());
    assert_ne!(app.asset_sessions.generation(), generation);
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    editor.undo();
    assert_eq!(editor.document["framesPerRow"], 12);
    context
        .run_ui(egui::RawInput::default(), |_| {})
        .drop_without_applying_deltas();
}

#[test]
fn studio_preserves_history_for_an_unchanged_manifest_but_never_restores_over_new_bytes() {
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(Some(example()), &context, false);
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    let mut document = editor.document.clone();
    document["framesPerRow"] = json!(240);
    editor.replace_document(document);
    app.apply_manifest();
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    assert_eq!(editor.document["framesPerRow"], 240);
    editor.undo();
    assert_eq!(editor.document["framesPerRow"], 12);
    app.open_asset("beep");
    let project = app.project.as_mut().unwrap();
    let mut changed: Value = serde_json::from_slice(&project.files["assets/loop.m01t"]).unwrap();
    changed["framesPerRow"] = json!(99);
    project.files.insert(
        "assets/loop.m01t".to_owned(),
        serde_json::to_vec(&changed).unwrap(),
    );
    app.open_asset("loop");
    let editor = app.asset_workbench.as_mut().unwrap();
    editor.undo();
    assert_eq!(editor.document["framesPerRow"], 99);
    app.asset_text = "invalid JSON".to_owned();
    app.asset_dirty = true;
    app.asset_workbench = None;
    app.open_asset("beep");
    assert_eq!(app.asset_name, "loop");
    assert_eq!(app.asset_text, "invalid JSON");
    context
        .run_ui(egui::RawInput::default(), |_| {})
        .drop_without_applying_deltas();
}
