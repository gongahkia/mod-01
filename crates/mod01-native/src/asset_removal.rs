//! Confirmed, member-bound deletion through the authoritative workcart checker.
use crate::{StudioApp, View, ui_theme};
use eframe::egui;

#[derive(Clone)]
pub(super) struct Removal {
    member: String,
    cartridge: String,
    name: String,
    path: String,
    kind: modl_core::AssetKind,
    error: Option<String>,
}

impl StudioApp {
    pub(super) fn request_asset_removal(&mut self, name: &str) {
        let Some(project) = &self.project else { return };
        let Some(asset) = project.manifest.assets.get(name) else {
            return;
        };
        self.asset_remove = Some(Removal {
            member: project.member_alias().to_owned(),
            cartridge: project.manifest.id.clone(),
            name: name.to_owned(),
            path: asset.path.clone(),
            kind: asset.kind,
            error: None,
        });
    }

    fn remove_asset(&mut self, target: &Removal, context: &egui::Context) -> Result<(), String> {
        let project = self.project.as_ref().ok_or("no project is open")?;
        if project.member_alias() != target.member
            || project.manifest.id != target.cartridge
            || project
                .manifest
                .assets
                .get(&target.name)
                .is_none_or(|asset| asset.path != target.path || asset.kind != target.kind)
        {
            return Err("ASSET TARGET CHANGED / CANCEL AND SELECT IT AGAIN".to_owned());
        }
        if !self.composition_ready() {
            return Err(self.status.clone());
        }
        let original = self.project.as_ref().unwrap().clone();
        let mut candidate = original.clone();
        candidate.delete_asset(&target.name)?;
        self.install_composition(candidate);
        self.asset_sessions
            .discard_asset(context, &original, &target.name, &target.path);
        self.view = View::Assets;
        self.status = format!(
            "DELETED {}/{} IN MEMORY / SAVE TO KEEP / SAVES UNCHANGED",
            target.member, target.name
        );
        Ok(())
    }

    pub(super) fn confirm_asset_removal(&mut self, context: &egui::Context) -> bool {
        let Some(target) = self.asset_remove.clone() else {
            return false;
        };
        match self.remove_asset(&target, context) {
            Ok(()) => {
                self.asset_remove = None;
                true
            }
            Err(error) => {
                self.status.clone_from(&error);
                if let Some(target) = &mut self.asset_remove {
                    target.error = Some(error);
                }
                false
            }
        }
    }

    pub(super) fn asset_delete_button(&mut self, ui: &mut egui::Ui) {
        let name = self
            .sprite
            .as_ref()
            .map(|editor| editor.name.clone())
            .or_else(|| {
                (!self.asset_name.is_empty()
                    && !self
                        .asset_workbench
                        .as_ref()
                        .is_some_and(crate::asset_tools::AssetWorkbench::is_display))
                .then(|| self.asset_name.clone())
            });
        if let Some(name) = name.filter(|name| {
            self.project
                .as_ref()
                .is_some_and(|project| project.manifest.assets.contains_key(name))
        }) && ui
            .add_enabled(!self.running, egui::Button::new("DELETE ASSET"))
            .on_hover_text(format!(
                "Confirm deletion of {name}. References block removal; stop the game first."
            ))
            .clicked()
        {
            self.request_asset_removal(&name);
        }
    }

    pub(super) fn asset_removal_ui(&mut self, context: &egui::Context) {
        let Some(target) = self.asset_remove.clone() else {
            return;
        };
        let mut confirm = false;
        let mut cancel = false;
        let response = egui::Modal::new(egui::Id::unique("asset-removal")).show(context, |ui| {
            ui.set_width((context.content_rect().width() - 60.0).clamp(240.0, 560.0));
            ui.heading("DELETE ASSET?");
            egui::ScrollArea::vertical().max_height((context.content_rect().height() - 200.0).max(80.0)).show(ui, |ui| {
                ui.label(format!("{} / {}", target.member, target.name));
                ui.add(egui::Label::new(&target.cartridge).truncate()).on_hover_text(&target.cartridge);
                ui.label("Removes this declaration and its unshared payload. Module, test, map/music and recipe references block deletion. Auxiliary authoring files and player saves are retained.");
                ui.label("Not an asset-edit undo operation. Nothing is written to disk until SAVE; reopen the unsaved workcart to discard this change.");
                if self.running { ui.colored_label(ui_theme::AMBER, "STOP THE GAME BEFORE DELETING"); }
                if let Some(error) = &target.error { ui.colored_label(ui_theme::CORAL, error); }
            });
            ui.horizontal(|ui| {
                cancel = ui.button("CANCEL").clicked();
                confirm = ui.add_enabled(!self.running, egui::Button::new("DELETE ASSET")).clicked();
            });
        });
        if cancel || response.should_close() {
            self.asset_remove = None;
        } else if confirm {
            self.confirm_asset_removal(context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::NativeProject;
    use serde_json::json;

    fn fixture() -> (StudioApp, egui::Context, std::path::PathBuf, Vec<u8>) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/api-tour/cart.m01w");
        let disk = std::fs::read(&path).unwrap();
        let context = egui::Context::default();
        crate::ui_assets::install_font(&context);
        ui_theme::install(&context);
        let mut app = StudioApp::new_with_devices(None, &context, false);
        let project = NativeProject::open(&path).unwrap();
        app.manifest_text = project.manifest_source.clone();
        app.project = Some(project);
        app.new_asset_name = "spare".to_owned();
        app.new_asset_kind = modl_core::AssetKind::Sound;
        app.create_asset();
        (app, context, path, disk)
    }

    fn note(app: &mut StudioApp, value: u8) {
        let editor = app.asset_workbench.as_mut().unwrap();
        let mut document = editor.document.clone();
        document["note"] = json!(value);
        editor.replace_document(document);
    }

    fn add_child(app: &mut StudioApp) {
        let source = app.project.as_ref().unwrap().source().unwrap();
        let mut parsed = modl_core::parse_workcart(&source).unwrap();
        parsed.manifest.id = "native-delete-child".to_owned();
        let child = modl_core::encode_workcart(
            &parsed.manifest,
            &parsed.files,
            &parsed.recipes,
            &parsed.tests,
        )
        .unwrap();
        app.add_member_source("arena", &child);
        assert_eq!(
            app.project.as_ref().unwrap().member_alias(),
            "arena",
            "{}",
            app.status
        );
    }

    #[test]
    fn native_asset_deletion_refuses_references_running_sessions_and_stale_member_targets() {
        let (mut app, context, path, disk) = fixture();
        for (asset, reference) in [("beep", "module"), ("tiles", "room")] {
            app.open_asset(asset);
            let before = app.project.as_ref().unwrap().source().unwrap();
            app.request_asset_removal(asset);
            assert!(!app.confirm_asset_removal(&context));
            assert!(
                app.status.contains("M014028") && app.status.contains(reference),
                "{}",
                app.status
            );
            assert_eq!(app.project.as_ref().unwrap().source().unwrap(), before);
        }
        app.request_asset_removal("spare");
        app.running = true;
        assert!(!app.confirm_asset_removal(&context));
        assert!(app.status.contains("STOP THE GAME"));
        app.running = false;
        app.asset_remove = None;
        add_child(&mut app);
        app.select_member("main");
        app.request_asset_removal("spare");
        app.select_member("arena");
        let before = app.project.as_ref().unwrap().source().unwrap();
        assert!(!app.confirm_asset_removal(&context));
        assert!(app.status.contains("TARGET CHANGED"));
        assert_eq!(app.project.as_ref().unwrap().source().unwrap(), before);
        assert!(
            app.project
                .as_ref()
                .unwrap()
                .manifest
                .assets
                .contains_key("spare")
        );
        assert_eq!(std::fs::read(path).unwrap(), disk);
    }

    #[test]
    fn native_sprite_deletion_discards_runtime_payload_but_retains_authored_sidecar() {
        let (mut app, context, path, disk) = fixture();
        app.sprite_name = "qaHero".to_owned();
        app.create_sprite();
        let sprite = app.sprite.as_mut().unwrap();
        sprite.remember();
        sprite.document.frames[0].layers[0].pixels[0] ^= 255;
        sprite.dirty = true;
        app.request_asset_removal("qaHero");
        assert!(app.confirm_asset_removal(&context), "{}", app.status);
        let project = app.project.as_ref().unwrap();
        assert!(!project.manifest.assets.contains_key("qaHero"));
        assert!(!project.files.contains_key("assets/qaHero.m01g"));
        assert!(
            project.files.contains_key("editor/sprites/qaHero.m01paint"),
            "authored auxiliary source is retained"
        );
        project.game().unwrap();
        assert!(app.sprite.is_none());
        assert_eq!(std::fs::read(path).unwrap(), disk);
    }

    #[test]
    fn native_asset_deletion_preserves_other_members_unsaved_modules_auxiliary_files_and_asset_history()
     {
        let (mut app, context, path, disk) = fixture();
        add_child(&mut app);
        app.open_asset("spare");
        note(&mut app, 73);
        app.select_member("main");
        app.project.as_mut().unwrap().files.insert(
            "notes/keep.txt".to_owned(),
            b"owned auxiliary source".to_vec(),
        );
        app.project
            .as_mut()
            .unwrap()
            .editor_text
            .push_str("\n// unsaved root note\n");
        app.open_asset("spare");
        note(&mut app, 61);
        app.asset_workbench.as_mut().unwrap().undo();
        app.open_asset("beep");
        note(&mut app, 70);
        app.open_asset("spare");
        let original = app.project.as_ref().unwrap().clone();
        let before = modl_core::parse_workcart(&original.source().unwrap()).unwrap();
        app.request_asset_removal("spare");
        assert!(app.confirm_asset_removal(&context), "{}", app.status);
        let project = app.project.as_ref().unwrap();
        assert_eq!(project.member_alias(), "main");
        assert!(!project.manifest.assets.contains_key("spare"));
        assert!(!project.files.contains_key("assets/spare.m01s"));
        assert!(project.editor_text.contains("// unsaved root note"));
        assert_eq!(project.files["notes/keep.txt"], b"owned auxiliary source");
        let after = modl_core::parse_workcart(&project.source().unwrap()).unwrap();
        assert_eq!(before.bundle[0].source, after.bundle[0].source);
        assert_eq!(before.tests.len(), after.tests.len());
        for (old, new) in before.tests.iter().zip(&after.tests) {
            assert_eq!(old.path, new.path);
            assert_eq!(old.payload, new.payload);
        }
        project.game().unwrap();
        assert!(project.dirty);
        assert!(app.asset_workbench.is_none() && app.sprite.is_none());
        // Redeclare the exact previous payload in memory, without a session reset.
        app.manifest_text = original.manifest_source.clone();
        app.project = Some(original);
        app.open_asset("spare");
        assert_eq!(app.asset_workbench.as_ref().unwrap().document["note"], 60);
        app.asset_workbench.as_mut().unwrap().redo();
        assert_eq!(
            app.asset_workbench.as_ref().unwrap().document["note"],
            60,
            "deleted asset redo must not return"
        );
        app.open_asset("beep");
        assert_eq!(app.asset_workbench.as_ref().unwrap().document["note"], 70);
        app.asset_workbench.as_mut().unwrap().undo();
        assert_eq!(app.asset_workbench.as_ref().unwrap().document["note"], 69);
        app.select_member("arena");
        app.open_asset("spare");
        assert_eq!(app.asset_workbench.as_ref().unwrap().document["note"], 73);
        app.asset_workbench.as_mut().unwrap().undo();
        assert_eq!(app.asset_workbench.as_ref().unwrap().document["note"], 60);
        app.request_asset_removal("spare");
        assert!(app.confirm_asset_removal(&context), "{}", app.status);
        assert_eq!(app.project.as_ref().unwrap().member_alias(), "arena");
        assert!(
            !app.project
                .as_ref()
                .unwrap()
                .manifest
                .assets
                .contains_key("spare")
        );
        app.select_member("main");
        assert!(
            app.project
                .as_ref()
                .unwrap()
                .manifest
                .assets
                .contains_key("spare")
        );
        assert_eq!(std::fs::read(path).unwrap(), disk);
    }

    #[test]
    fn deleting_one_asset_clears_only_its_raw_text_undo_and_scopes_text_ids_by_member() {
        let (mut app, context, _, _) = fixture();
        let project = app.project.as_ref().unwrap().clone();
        let deleted = app
            .asset_sessions
            .text_id(&project, "spare", "assets/spare.m01s");
        let other = app
            .asset_sessions
            .text_id(&project, "beep", "assets/beep.m01s");
        let cursor = egui::text::CCursorRange::one(egui::text::CCursor::new(0));
        let current = (cursor, "edited".to_owned());
        for id in [deleted, other] {
            let mut state = egui::text_edit::TextEditState::default();
            let mut undoer = state.undoer();
            undoer.feed_state(0.0, &(cursor, "old".to_owned()));
            assert!(undoer.has_undo(&current));
            state.set_undoer(undoer);
            state.store(&context, id);
        }
        app.request_asset_removal("spare");
        assert!(app.confirm_asset_removal(&context));
        assert!(
            !egui::TextEdit::load_state(&context, deleted)
                .unwrap()
                .undoer()
                .has_undo(&current)
        );
        assert!(
            egui::TextEdit::load_state(&context, other)
                .unwrap()
                .undoer()
                .has_undo(&current)
        );
        add_child(&mut app);
        let child = app.project.as_ref().unwrap();
        assert_ne!(
            deleted,
            app.asset_sessions
                .text_id(child, "spare", "assets/spare.m01s")
        );
    }

    fn modal_frame(
        app: &mut StudioApp,
        context: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.asset_removal_ui(ui.ctx()),
        );
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                    shape
                        .clip_rect
                        .contains(rect.center())
                        .then(|| (text.galley.job.text.clone(), rect))
                } else {
                    None
                }
            })
            .collect();
        output.drop_without_applying_deltas();
        labels
    }

    fn modal_click(app: &mut StudioApp, context: &egui::Context, label: &str) {
        for _ in 0..3 {
            modal_frame(app, context, Vec::new());
        }
        let labels = modal_frame(app, context, Vec::new());
        let rect = labels.iter().find(|(text, _)| text == label).unwrap().1;
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0))
                .contains_rect(rect)
        );
        modal_frame(app, context, vec![egui::Event::PointerMoved(rect.center())]);
        for pressed in [true, false] {
            modal_frame(
                app,
                context,
                vec![egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        modal_frame(app, context, Vec::new());
    }

    #[test]
    fn actual_minimum_size_confirmation_can_cancel_show_references_and_delete_without_disk_writes()
    {
        let (mut app, context, path, disk) = fixture();
        app.request_asset_removal("spare");
        modal_click(&mut app, &context, "CANCEL");
        assert!(app.asset_remove.is_none());
        assert!(
            app.project
                .as_ref()
                .unwrap()
                .manifest
                .assets
                .contains_key("spare")
        );
        app.request_asset_removal("beep");
        modal_click(&mut app, &context, "DELETE ASSET");
        assert!(
            app.asset_remove
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .contains("M014028")
        );
        let target = app.asset_remove.as_mut().unwrap();
        target.error =
            Some("M014028: referenced by a long authored module/test/recipe path. ".repeat(256));
        target.name = "a".repeat(64);
        target.member = "a".repeat(32);
        app.running = true;
        modal_click(&mut app, &context, "CANCEL");
        app.running = false;
        assert!(
            app.project
                .as_ref()
                .unwrap()
                .manifest
                .assets
                .contains_key("beep")
        );
        app.request_asset_removal("spare");
        modal_click(&mut app, &context, "DELETE ASSET");
        assert!(app.asset_remove.is_none());
        assert!(
            !app.project
                .as_ref()
                .unwrap()
                .manifest
                .assets
                .contains_key("spare")
        );
        assert_eq!(std::fs::read(path).unwrap(), disk);
    }
}
