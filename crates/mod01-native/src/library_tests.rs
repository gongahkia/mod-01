//! Tests the actual library worker, UI and Studio/Node opening path with isolated preferences.
use super::*;
use crate::{StudioApp, project::NativeProject, ui_assets};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct Temporary(PathBuf);
impl Temporary {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "mod01-library-ui-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn cart(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        let mut project = NativeProject::create(&path, &format!("library-qa-{name}")).unwrap();
        project.manifest_source = project
            .manifest_source
            .replace("NEW CARTRIDGE", &name.to_uppercase())
            .replace("@author", "@library-test");
        project.save().unwrap();
        path.join("cart.m01w").canonicalize().unwrap()
    }
    fn library(&self, bundled: Vec<PathBuf>) -> Library {
        Library::new(Some(self.0.join("preferences/library.json")), bundled)
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn ready(library: &mut Library, context: &egui::Context) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        library.poll(context);
        if !library.busy && library.scan_pending.is_none() {
            break;
        }
        assert!(Instant::now() < deadline, "{}", library.status);
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn real_worker_search_favourites_labels_recents_and_missing_cards_survive_restart() {
    let temporary = Temporary::new();
    let alpha = temporary.cart("alpha-quest");
    let beta = temporary.cart("beta-race");
    let original = fs::read(&alpha).unwrap();
    let context = egui::Context::default();
    let mut library = temporary.library(vec![alpha.clone()]);
    library.register(&[alpha.clone(), beta.clone()]);
    assert_eq!(
        library.cards.len(),
        2,
        "bundled/local canonical paths are deduplicated"
    );
    ready(&mut library, &context);
    assert!(
        library
            .cards
            .iter()
            .all(|card| matches!(card.preview, Some(Ok(_))))
    );
    library.query = "alpha @library-test".to_owned();
    assert_eq!(library.visible().len(), 1);
    library.select(Some(alpha.clone()));
    library.labels = " Jam , Puzzle".to_owned();
    library.labels_dirty = true;
    library.preferences(true);
    assert!(
        library.entry(&alpha).unwrap().labels.is_empty(),
        "favourite must not commit unapplied label text"
    );
    assert!(library.labels_dirty);
    library.preferences(false);
    assert_eq!(library.entry(&alpha).unwrap().labels, ["Jam", "Puzzle"]);
    library.query = "puzzle ALPHA".to_owned();
    assert_eq!(library.visible().len(), 1);
    library.query.clear();
    library.favourites_only = true;
    assert_eq!(library.visible().len(), 1);
    library.opened(&alpha).unwrap();
    library.opened(&beta).unwrap();
    library.favourites_only = false;
    library.recent_first = true;
    assert_eq!(library.cards[library.visible()[0]].path, beta);
    let mut reopened = temporary.library(vec![alpha.clone()]);
    assert!(reopened.entry(&alpha).unwrap().favourite);
    assert_eq!(reopened.entry(&beta).unwrap().last_used, 2);
    assert_eq!(fs::read(&alpha).unwrap(), original);
    fs::remove_file(&beta).unwrap(); // Make this test-owned bookmark unavailable, not forgotten.
    reopened.refresh();
    ready(&mut reopened, &context);
    assert_eq!(reopened.cards.len(), 2);
    assert!(
        reopened
            .cards
            .iter()
            .find(|card| card.path == beta)
            .unwrap()
            .preview
            .as_ref()
            .unwrap()
            .is_err()
    );
    reopened.select(Some(beta.clone()));
    assert!(reopened.open_action(true).is_none());
    reopened.store.as_mut().unwrap().forget(&beta).unwrap();
    reopened.reconcile();
    assert_eq!(reopened.cards.len(), 1);
    let saved = fs::read(reopened.store.as_ref().unwrap().path()).unwrap();
    fs::write(reopened.store.as_ref().unwrap().path(), b"broken metadata").unwrap();
    reopened.refresh();
    assert!(!reopened.writable());
    assert_eq!(
        reopened.cards.len(),
        1,
        "reload failure retains usable in-memory bookmarks"
    );
    reopened.preferences(true);
    assert_eq!(
        fs::read(reopened.store.as_ref().unwrap().path()).unwrap(),
        b"broken metadata"
    );
    fs::write(reopened.store.as_ref().unwrap().path(), saved).unwrap();
    reopened.refresh();
    assert!(reopened.writable());
}

#[test]
fn directory_scan_is_explicit_bounded_and_does_not_follow_nested_symlinks_or_cart_assets() {
    let temporary = Temporary::new();
    let cart = temporary.cart("game");
    fs::write(
        cart.parent().unwrap().join("not-a-dependency.m01w"),
        b"ignore assets in cart folder",
    )
    .unwrap();
    fs::write(temporary.0.join("plain.txt"), b"not a cartridge").unwrap();
    fs::write(
        temporary.0.join("packed.m01c"),
        b"will be diagnosed by preview",
    )
    .unwrap();
    let mut deep = temporary.0.clone();
    for _ in 0..9 {
        deep.push("deep");
    }
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("outside-depth-limit.m01w"), b"skip").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&temporary.0, temporary.0.join("loop")).unwrap();
    let scan = scan_folder(&temporary.0).unwrap();
    assert!(scan.limited);
    assert_eq!(scan.paths.len(), 2);
    assert!(scan.paths.contains(&cart));
    #[cfg(unix)]
    assert_eq!(scan.skipped, 1);
    let context = egui::Context::default();
    let mut library = temporary.library(Vec::new());
    library.scan_pending = Some(temporary.0.clone());
    ready(&mut library, &context);
    assert_eq!(library.cards.len(), 2);
    assert!(library.status.contains("SCAN LIMIT"));
    assert!(
        library
            .cards
            .iter()
            .any(|card| matches!(card.preview, Some(Err(_))))
    );
    assert!(cart.is_file());
}

#[test]
fn selected_cover_refresh_ignores_stale_worker_results_and_has_one_image_cache() {
    let temporary = Temporary::new();
    let alpha = temporary.cart("alpha");
    let beta = temporary.cart("beta");
    let context = egui::Context::default();
    let mut library = temporary.library(Vec::new());
    library.register(&[alpha.clone(), beta.clone()]);
    library.select(Some(alpha.clone()));
    library.poll(&context);
    assert!(library.busy);
    library.select(Some(beta.clone()));
    library.refresh();
    ready(&mut library, &context);
    assert_eq!(library.cover_path.as_ref(), Some(&beta));
    assert!(library.cards.iter().all(|card| {
        card.preview
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .image
            .is_none()
    }));
    assert_eq!(library.cards.len(), 2);
}

#[test]
fn real_egui_library_keyboard_controller_and_text_focus_work_at_minimum_size() {
    let temporary = Temporary::new();
    let alpha = temporary.cart("alpha");
    let beta = temporary.cart("beta");
    let context = egui::Context::default();
    ui_theme::install(&context);
    ui_assets::install_font(&context);
    let mut library = temporary.library(Vec::new());
    library.register(&[alpha.clone(), beta.clone()]);
    ready(&mut library, &context);
    let viewport = egui::Rect::from_min_size(egui::pos2(0.0, 95.0), egui::vec2(900.0, 470.0));
    let mut pass = |events, navigation| {
        let mut action = None;
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    events,
                    ..Default::default()
                },
                |ui| {
                    action = library.ui(ui, navigation, true);
                    assert!(
                        ui.min_rect().right() <= viewport.right() + 1.0,
                        "horizontal library overflow: {:?}",
                        ui.min_rect()
                    );
                    assert!(
                        ui.min_rect().bottom() <= viewport.bottom() + 1.0,
                        "vertical library overflow: {:?}",
                        ui.min_rect()
                    );
                },
            )
            .drop_without_applying_deltas();
        action
    };
    assert!(
        pass(
            vec![],
            Navigation {
                confirm: true,
                ..Default::default()
            }
        )
        .is_none(),
        "opening gesture is not a launch"
    );
    let action = pass(
        vec![],
        Navigation {
            down: true,
            inspect: true,
            ..Default::default()
        },
    );
    assert!(
        matches!(action, Some(Action::Open(OpenRequest { path, play: false, .. })) if path == beta)
    );
    let action = pass(
        vec![egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        Navigation::default(),
    );
    assert!(
        matches!(action, Some(Action::Open(OpenRequest { path, play: true, .. })) if path == beta)
    );
    pass(
        vec![egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        Navigation {
            favourite: true,
            ..Default::default()
        },
    );
    assert!(library.entry(&beta).unwrap().favourite);
    // Clicking the actual search box then typing shortcut letters must filter, not launch.
    let mut action = None;
    context
        .run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                ..Default::default()
            },
            |ui| {
                let response = ui.text_edit_singleline(&mut library.query);
                response.request_focus();
            },
        )
        .drop_without_applying_deltas();
    context
        .run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                events: vec![
                    egui::Event::Text("x".to_owned()),
                    egui::Event::Key {
                        key: egui::Key::X,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                let response = ui.text_edit_singleline(&mut library.query);
                assert!(response.has_focus());
                action = library.ui(
                    ui,
                    Navigation {
                        confirm: true,
                        ..Default::default()
                    },
                    true,
                );
            },
        )
        .drop_without_applying_deltas();
    assert!(action.is_none());
}

#[test]
fn studio_library_preserves_dirty_work_until_save_discard_or_cancel_and_failed_open_is_safe() {
    let temporary = Temporary::new();
    let alpha = temporary.cart("alpha");
    let beta = temporary.cart("beta");
    let original = fs::read(&alpha).unwrap();
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(None, &context, false);
    app.library = temporary.library(Vec::new());
    app.open(alpha.clone());
    app.project.as_mut().unwrap().editor_text = "on draw:\n  clear(7)\n".to_owned();
    app.project.as_mut().unwrap().dirty = true;
    app.open(beta.clone());
    assert!(app.pending_library_open.is_some() && app.show_library);
    assert_eq!(
        app.project.as_ref().unwrap().workcart_path.as_ref(),
        Some(&alpha)
    );
    app.confirm_library_open(ConfirmOpen::Cancel);
    assert!(app.pending_library_open.is_none());
    assert_eq!(fs::read(&alpha).unwrap(), original);
    app.open(beta.clone());
    app.manifest_text = "invalid manifest".to_owned();
    app.confirm_library_open(ConfirmOpen::Save);
    assert!(app.pending_library_open.is_some());
    assert_eq!(fs::read(&alpha).unwrap(), original);
    app.manifest_text = app.project.as_ref().unwrap().manifest_source.clone();
    // A real filesystem save failure (destination is an existing directory), not
    // a mocked return value, must leave both the pending request and source alive.
    app.project.as_mut().unwrap().workcart_path = Some(temporary.0.clone());
    app.confirm_library_open(ConfirmOpen::Save);
    assert!(app.pending_library_open.is_some());
    assert!(
        app.project
            .as_ref()
            .unwrap()
            .editor_text
            .contains("clear(7)")
    );
    assert_eq!(fs::read(&alpha).unwrap(), original);
    app.project.as_mut().unwrap().workcart_path = Some(alpha.clone());
    app.confirm_library_open(ConfirmOpen::Save);
    assert!(app.pending_library_open.is_none());
    assert!(
        NativeProject::open(&alpha)
            .unwrap()
            .editor_text
            .contains("clear(7)")
    );
    assert_eq!(
        app.project.as_ref().unwrap().workcart_path.as_ref(),
        Some(&beta)
    );
    app.project.as_mut().unwrap().editor_text = "unsaved invalid source".to_owned();
    app.project.as_mut().unwrap().dirty = true;
    app.open(alpha.clone());
    app.confirm_library_open(ConfirmOpen::Discard);
    assert_eq!(
        app.project.as_ref().unwrap().workcart_path.as_ref(),
        Some(&alpha)
    );
    assert!(
        !NativeProject::open(&beta)
            .unwrap()
            .editor_text
            .contains("invalid")
    );
    app.open(temporary.0.join("missing.m01w"));
    assert!(app.status.contains("OPEN FAILED"));
    assert_eq!(
        app.project.as_ref().unwrap().workcart_path.as_ref(),
        Some(&alpha)
    );
}

#[test]
fn studio_launches_source_packed_and_png_through_real_node_and_bundled_source_is_an_unsaved_copy() {
    let temporary = Temporary::new();
    let source = temporary.cart("play");
    let original = fs::read(&source).unwrap();
    let project = NativeProject::open(&source).unwrap();
    let packed = project.pack().unwrap();
    let packed_path = temporary.0.join("play.m01c");
    fs::write(&packed_path, &packed).unwrap();
    let png_path = temporary.0.join("play.m01c.png");
    fs::write(
        &png_path,
        modl_core::encode_cartridge_png(
            &packed,
            &modl_core::CartridgePngMetadata {
                title: project.manifest.title.clone(),
                author: project.manifest.author.clone(),
                year: 2026,
                players: 1,
                controls: "ARROWS / Z".to_owned(),
            },
        )
        .unwrap(),
    )
    .unwrap();
    let context = egui::Context::default();
    let mut app = StudioApp::new_with_devices(None, &context, false);
    app.library = temporary.library(vec![source.clone()]);
    for path in [&source, &packed_path, &png_path] {
        app.request_library_open(OpenRequest {
            path: path.clone(),
            bundled: path == &source,
            play: true,
        });
        crate::studio_replay_tests::pump(&mut app, &context);
        app.send_input(&context, "frame");
        crate::studio_replay_tests::pump(&mut app, &context);
        assert!(app.last_pixels.iter().all(|pixel| *pixel == 1));
        assert!(app.project.as_ref().unwrap().workcart_path.is_none());
        app.enter_library();
        assert!(app.player_paused && app.show_library);
        let next = app.input_frame;
        app.advance_runtime(&context);
        assert_eq!(app.input_frame, next);
        app.library_action(Action::Close);
        assert!(!app.player_paused && !app.show_library);
        app.set_player_paused(true);
        app.enter_library();
        app.library_action(Action::Close);
        assert!(app.player_paused, "back preserves a pre-existing pause");
        app.stop();
    }
    app.request_library_open(OpenRequest {
        path: source.clone(),
        bundled: true,
        play: false,
    });
    assert!(!app.running && app.view == crate::View::Code);
    assert!(app.project.as_ref().unwrap().root.is_none());
    assert_eq!(fs::read(source).unwrap(), original);
    assert_eq!(
        app.library
            .store
            .as_ref()
            .unwrap()
            .collection()
            .entries
            .len(),
        3
    );
}
