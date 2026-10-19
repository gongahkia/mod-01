// Sprite dimensions, screen coordinates and PCM samples are validated before these conversions.
// The egui panels are intentionally kept as contiguous UI functions for stateful interaction.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::struct_excessive_bools,
    clippy::similar_names,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::needless_pass_by_value,
    clippy::float_cmp,
    clippy::chunks_exact_to_as_chunks,
    clippy::manual_is_multiple_of,
    clippy::manual_midpoint,
    clippy::collapsible_if,
    clippy::semicolon_if_nothing_returned,
    clippy::assigning_clones
)]

mod asset_meter;
mod asset_removal;
mod asset_sessions;
mod asset_tools;
mod audio;
mod export_names;
mod gamepad_controller;
mod keyboard_controller;
mod launch;
mod library;
mod library_preview;
mod library_store;
mod live_audio;
mod pause_menu;
mod player;
mod playground;
mod project;
mod replay;
mod runtime;
mod save_manager;
mod source_editor;
mod sprite;
#[cfg(test)]
mod studio_asset_tests;
#[cfg(test)]
mod studio_game_tests;
#[cfg(test)]
mod studio_menu_tests;
#[cfg(test)]
mod studio_replay_tests;
mod syntax;
mod touch_controller;
mod tracker;
mod ui_assets;
mod ui_theme;

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use asset_tools::AssetWorkbench;
use audio::AudioOutput;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use directories::ProjectDirs;
use eframe::egui::{self, Color32, ColorImage, RichText, TextureHandle, TextureOptions};
use gilrs::Gilrs;
use project::NativeProject;
use runtime::RuntimeBridge;
use serde_json::{Value, json};
use sprite::{DEFAULT_AXIS, SpriteDocument, SpriteEditor, Tool};
use ui_assets::PixelCursors;

const SCREEN_WIDTH: usize = 240;
const SCREEN_HEIGHT: usize = 144;
#[derive(Clone, Copy)]
enum AudioPreviewMode {
    Song(usize),
    Pattern,
    Note,
}
const fn manual_topics() -> [(&'static str, &'static str); 15] {
    [
        ("START", include_str!("../../../docs/TUTORIAL.md")),
        ("MODL LANGUAGE", include_str!("../../../docs/MODL.md")),
        ("API", include_str!("../../../docs/API.md")),
        (
            "MATH / TEXT / BITS",
            include_str!("../../../docs/STANDARD_LIBRARY.md"),
        ),
        ("SOURCE TOOLS", include_str!("../../../docs/EDITOR.md")),
        ("PLAYGROUND", include_str!("../../../docs/PLAYGROUND.md")),
        ("SOFTWARE 3D", include_str!("../../../docs/SOFTWARE_3D.md")),
        ("ASSETS", include_str!("../../../docs/ASSETS.md")),
        ("HARDWARE", include_str!("../../../docs/HARDWARE.md")),
        ("LIMITS", include_str!("../../../docs/LIMITS.md")),
        ("DEBUGGING", include_str!("../../../docs/DEBUGGING.md")),
        ("PAUSE ACTIONS", include_str!("../../../docs/MENU.md")),
        ("LOCAL LIBRARY", include_str!("../../../docs/LIBRARY.md")),
        (
            "CARTRIDGES",
            include_str!("../../../docs/CARTRIDGE_FORMAT.md"),
        ),
        ("STUDIO", include_str!("../../../docs/STUDIO.md")),
    ]
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Code,
    Sprites,
    Assets,
    Player,
    Manifest,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DebugTab {
    Source,
    State,
    Tasks,
    Memory,
    Audio,
    Profile,
}

struct SourceBreakpoint {
    member: String,
    source: String,
    line: usize,
    condition: String,
}

struct RunningMember {
    id: String,
    title: String,
    entry: String,
    sources: BTreeMap<String, Vec<u8>>,
}

impl RunningMember {
    fn catalog(game: &modl_core::game::PackedGame) -> Result<BTreeMap<String, Self>, String> {
        game.catalog()
            .cartridges()
            .iter()
            .map(|(alias, cart)| {
                let view = modl_core::unpack_cartridge_project(&cart.bytes)
                    .map_err(|error| error.to_string())?;
                Ok((
                    alias.clone(),
                    Self {
                        id: cart.manifest.id.clone(),
                        title: cart.manifest.title.clone(),
                        entry: cart.manifest.entry.clone(),
                        sources: view
                            .files
                            .into_iter()
                            .filter(|(path, _)| path.ends_with(".modl"))
                            .collect(),
                    },
                ))
            })
            .collect()
    }
}

struct StudioApp {
    cursors: PixelCursors,
    library: library::Library,
    show_library: bool,
    library_was_paused: bool,
    pending_library_open: Option<library::OpenRequest>,
    project: Option<NativeProject>,
    manifest_text: String,
    view: View,
    runtime: RuntimeBridge,
    preview_runtime: RuntimeBridge,
    live_audio: live_audio::AudioPreview,
    playground: playground::Playground,
    show_playground: bool,
    audio: Option<AudioOutput>,
    audio_notice: Option<String>,
    gamepads: Option<Gilrs>,
    texture: Option<TextureHandle>,
    sprite: Option<SpriteEditor>,
    modl_highlighter: syntax::ModlHighlighter,
    source_editor: source_editor::SourceEditor,
    sprite_texture: Option<TextureHandle>,
    sprite_name: String,
    asset_name: String,
    asset_path: String,
    asset_text: String,
    asset_dirty: bool,
    asset_workbench: Option<AssetWorkbench>,
    asset_sessions: asset_sessions::AssetSessions,
    asset_meter: asset_meter::AssetMeter,
    color_hex: String,
    sprite_painting: bool,
    last_sprite_point: Option<(i32, i32)>,
    selection_anchor: Option<(usize, usize)>,
    running: bool,
    player_mode: bool,
    viewport_mode: Option<(bool, bool)>,
    player_paused: bool,
    touch_controller: touch_controller::TouchController,
    keyboard_controller: keyboard_controller::KeyboardController,
    gamepad_controller: gamepad_controller::GamepadController,
    pause_menu: pause_menu::PauseMenu,
    pause_navigation: gamepad_controller::Navigation,
    pending_menu_action: Option<u8>,
    resume_debug: bool,
    debugging: bool,
    snapshot: Option<String>,
    debug_event: String,
    debug_source: Option<(String, String, usize)>,
    debug_inspection: Value,
    debug_trace: Vec<Value>,
    debug_tab: DebugTab,
    debug_running: bool,
    breakpoints: Vec<SourceBreakpoint>,
    breakpoint_line: usize,
    breakpoint_condition: String,
    watch_input: String,
    watches: Vec<String>,
    watch_results: Vec<Value>,
    memory_address: u32,
    memory_length: usize,
    memory_view: String,
    memory_edit: String,
    debug_snapshots: Vec<(u32, String)>,
    last_pixels: Vec<u8>,
    recording_gif: bool,
    gif_frames: Vec<Vec<u8>>,
    recording_replay: bool,
    replay_frames: Vec<(u32, Value)>,
    replay_playback: Option<BTreeMap<u32, Value>>,
    preview_pending: bool,
    preview_cancelling: bool,
    preview_name: String,
    loaded: bool,
    running_members: BTreeMap<String, RunningMember>,
    active_alias: String,
    pending: bool,
    frame: u32,
    input_frame: u32,
    frame_input: Option<Value>,
    debug_booted: bool,
    debug_frame_boundary: bool,
    work: u32,
    last_tick: Instant,
    screen_rect: Option<egui::Rect>,
    status: String,
    new_path: String,
    new_id: String,
    show_new: bool,
    show_new_asset: bool,
    show_members: bool,
    member_alias_input: String,
    member_id_input: String,
    member_remove: Option<String>,
    asset_remove: Option<asset_removal::Removal>,
    new_asset_name: String,
    new_asset_kind: modl_core::AssetKind,
    show_manual: bool,
    manual_query: String,
    manual_topic: usize,
    show_explorer: bool,
    explorer_tab: String,
    explorer_data: Option<Value>,
    show_save_manager: bool,
    confirm_save_action: Option<&'static str>,
}

impl StudioApp {
    fn new(start: Option<PathBuf>, context: &egui::Context) -> Self {
        Self::new_with_devices(start, context, true)
    }

    fn new_with_devices(start: Option<PathBuf>, context: &egui::Context, devices: bool) -> Self {
        let host_path = native_host_path();
        let (audio, audio_notice) = if devices {
            match AudioOutput::new() {
                Ok(audio) => (Some(audio), None),
                Err(error) => (None, Some(error)),
            }
        } else {
            (None, None)
        };
        let mut app = Self {
            cursors: PixelCursors::new(context),
            library: library::Library::new(
                devices
                    .then(|| {
                        ProjectDirs::from("", "MOD-01", "Native Studio")
                            .map(|directories| directories.data_dir().join("library.json"))
                    })
                    .flatten(),
                bundled_samples(),
            ),
            show_library: true,
            library_was_paused: false,
            pending_library_open: None,
            project: None,
            manifest_text: String::new(),
            view: View::Code,
            runtime: RuntimeBridge::new(host_path.clone()),
            preview_runtime: RuntimeBridge::new(host_path),
            live_audio: live_audio::AudioPreview::new(),
            playground: playground::Playground::new(),
            show_playground: false,
            audio,
            audio_notice,
            gamepads: devices.then(|| Gilrs::new().ok()).flatten(),
            texture: None,
            sprite: None,
            modl_highlighter: syntax::ModlHighlighter::default(),
            source_editor: source_editor::SourceEditor::default(),
            sprite_texture: None,
            sprite_name: "hero".to_owned(),
            asset_name: String::new(),
            asset_path: String::new(),
            asset_text: String::new(),
            asset_dirty: false,
            asset_workbench: None,
            asset_sessions: asset_sessions::AssetSessions::default(),
            asset_meter: asset_meter::AssetMeter::default(),
            color_hex: "ED7B69FF".to_owned(),
            sprite_painting: false,
            last_sprite_point: None,
            selection_anchor: None,
            running: false,
            player_mode: false,
            viewport_mode: None,
            player_paused: false,
            touch_controller: touch_controller::TouchController::default(),
            keyboard_controller: keyboard_controller::KeyboardController::default(),
            gamepad_controller: gamepad_controller::GamepadController::default(),
            pause_menu: pause_menu::PauseMenu::default(),
            pause_navigation: gamepad_controller::Navigation::default(),
            pending_menu_action: None,
            resume_debug: false,
            debugging: false,
            snapshot: None,
            debug_event: String::new(),
            debug_source: None,
            debug_inspection: Value::Null,
            debug_trace: Vec::new(),
            debug_tab: DebugTab::Source,
            debug_running: false,
            breakpoints: Vec::new(),
            breakpoint_line: 1,
            breakpoint_condition: String::new(),
            watch_input: String::new(),
            watches: Vec::new(),
            watch_results: Vec::new(),
            memory_address: 0,
            memory_length: 64,
            memory_view: String::new(),
            memory_edit: String::new(),
            debug_snapshots: Vec::new(),
            last_pixels: Vec::new(),
            recording_gif: false,
            gif_frames: Vec::new(),
            recording_replay: false,
            replay_frames: Vec::new(),
            replay_playback: None,
            preview_pending: false,
            preview_cancelling: false,
            preview_name: String::new(),
            loaded: false,
            running_members: BTreeMap::new(),
            active_alias: String::new(),
            pending: false,
            frame: 0,
            input_frame: 0,
            frame_input: None,
            debug_booted: false,
            debug_frame_boundary: true,
            work: 0,
            last_tick: Instant::now(),
            screen_rect: None,
            status: "MOD-01 NATIVE STUDIO / READY".to_owned(),
            new_path: String::new(),
            new_id: "new-cart".to_owned(),
            show_new: false,
            show_new_asset: false,
            show_members: false,
            member_alias_input: "level-1".to_owned(),
            member_id_input: "new-level".to_owned(),
            member_remove: None,
            asset_remove: None,
            new_asset_name: "new_asset".to_owned(),
            new_asset_kind: modl_core::AssetKind::TileSet,
            show_manual: false,
            manual_query: String::new(),
            manual_topic: 0,
            show_explorer: false,
            explorer_tab: "TOKENS".to_owned(),
            explorer_data: None,
            show_save_manager: false,
            confirm_save_action: None,
        };
        if let Some(path) = start {
            app.open(path);
        }
        app
    }

    fn open(&mut self, path: PathBuf) {
        self.request_library_open(library::OpenRequest {
            path,
            bundled: false,
            play: false,
        });
    }

    fn recover_revision(&mut self, path: PathBuf) {
        let Some(previous) = self.project.as_ref() else {
            return;
        };
        let root = previous.root.clone();
        let workcart_path = previous.workcart_path.clone();
        match NativeProject::open(&path) {
            Ok(mut project) => {
                self.stop();
                project.root = root;
                project.workcart_path = workcart_path;
                project.dirty = true;
                self.manifest_text = project.manifest_source.clone();
                self.project = Some(project);
                self.reset_asset_editors();
                self.view = View::Code;
                self.status = "RECOVERY REVISION LOADED; SAVE TO APPLY".to_owned();
            }
            Err(error) => self.status = error,
        }
    }

    fn create(&mut self) {
        match NativeProject::create(&PathBuf::from(&self.new_path), &self.new_id) {
            Ok(project) => {
                self.stop();
                self.manifest_text = project.manifest_source.clone();
                self.status = format!("CREATED {}", project.manifest.id);
                self.project = Some(project);
                self.reset_asset_editors();
                self.show_new = false;
                self.show_library = false;
                self.view = View::Code;
            }
            Err(error) => self.status = error,
        }
    }

    fn poll_audio_output(&mut self) {
        let failure = self.audio.as_ref().and_then(AudioOutput::failure);
        if let Some(error) = failure {
            self.disable_audio_output(error);
        } else if let Some(warning) = self.audio.as_ref().and_then(AudioOutput::take_warning) {
            self.audio_notice = Some(warning);
        }
    }

    fn disable_audio_output(&mut self, error: String) {
        if let Some(audio) = self.audio.take() {
            audio.clear();
        }
        self.audio_notice = Some(error);
        self.status = "AUDIO OUTPUT FAILED / SILENT".to_owned();
    }

    fn audio_indicator(&self, ui: &mut egui::Ui) {
        ui.scope(|ui| {
            ui_theme::led(
                ui,
                self.audio.is_some(),
                if self.audio.is_some() {
                    "AUD"
                } else {
                    "SILENT"
                },
            )
        })
        .response
        .on_hover_text(self.audio_message());
    }

    fn audio_message(&self) -> &str {
        self.audio_notice
            .as_deref()
            .unwrap_or(if self.audio.is_some() {
                "Native audio output available."
            } else {
                "No audio output device. Synth, playback and replay continue silently."
            })
    }

    fn apply_manifest(&mut self) {
        let requested_manifest = self.manifest_text.clone();
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty) {
            if let Err(error) = self.save_sprite() {
                self.status = error;
                return;
            }
        }
        match modl_core::parse_project_manifest(&requested_manifest) {
            Ok(manifest) => {
                self.cache_asset_editors();
                let Some(project) = self.project.as_mut() else {
                    return;
                };
                project.manifest_source = requested_manifest.clone();
                self.manifest_text = requested_manifest;
                project.manifest = manifest;
                project.dirty = true;
                self.sprite = None;
                self.asset_name.clear();
                self.asset_dirty = false;
                self.status = "MANIFEST APPLIED".to_owned();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn select_member(&mut self, alias: &str) {
        let Some(project) = &self.project else {
            return;
        };
        if alias == project.member_alias() {
            return;
        }
        if self.manifest_text != project.manifest_source {
            self.status = "APPLY OR REVERT MANIFEST DRAFT BEFORE SWITCHING CARTRIDGES".to_owned();
            return;
        }
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty)
            && let Err(error) = self.save_sprite()
        {
            self.status = error;
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        let mut candidate = project.clone();
        if let Err(error) = candidate.select_member(alias) {
            self.status = error;
            return;
        }
        self.cache_asset_editors();
        self.asset_meter = asset_meter::AssetMeter::default();
        self.source_editor.dismiss_completion();
        self.manifest_text = candidate.manifest_source.clone();
        self.project = Some(candidate);
        self.live_audio.stop();
        self.view = View::Code;
        self.status = format!("EDITING {alias} / SAVE AND RUN KEEP THE WHOLE GAME");
    }

    fn member_picker(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            return;
        };
        let selected = project.member_alias().to_owned();
        let members = project.members();
        let mut next = selected.clone();
        ui_theme::legend(ui, format!("GAME / {} CARTRIDGE(S)", members.len()));
        egui::ComboBox::from_id_salt("game-member")
            .selected_text(&selected)
            .width(ui.available_width() - 12.0)
            .show_ui(ui, |ui| {
                for (alias, id) in members {
                    ui.selectable_value(&mut next, alias.clone(), alias)
                        .on_hover_text(id);
                }
            });
        if next != selected {
            self.select_member(&next);
        }
        if ui.small_button("MANAGE MEMBERS").clicked() {
            self.show_members = true;
        }
    }

    fn composition_ready(&mut self) -> bool {
        if self.running {
            self.status = "STOP THE GAME BEFORE CHANGING ITS COMPOSITION".to_owned();
            return false;
        }
        let Some(project) = &self.project else {
            return false;
        };
        if self.manifest_text != project.manifest_source {
            self.status = "APPLY OR REVERT MANIFEST DRAFT BEFORE CHANGING COMPOSITION".to_owned();
            return false;
        }
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return false;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty)
            && let Err(error) = self.save_sprite()
        {
            self.status = error;
            return false;
        }
        true
    }

    fn install_composition(&mut self, project: NativeProject) {
        self.cache_asset_editors();
        self.asset_meter = asset_meter::AssetMeter::default();
        self.source_editor.dismiss_completion();
        self.manifest_text = project.manifest_source.clone();
        self.project = Some(project);
        self.live_audio.stop();
        self.view = View::Code;
    }

    fn add_member_source(&mut self, alias: &str, source: &str) {
        if !self.composition_ready() {
            return;
        }
        let Some(mut candidate) = self.project.clone() else {
            return;
        };
        match candidate.add_member(alias, source) {
            Ok(()) => {
                self.install_composition(candidate);
                self.status = format!("ADDED {alias} / SAVE KEEPS THE WHOLE GAME");
            }
            Err(error) => self.status = error,
        }
    }

    fn import_member_path(&mut self, alias: &str, path: &std::path::Path) {
        let source = NativeProject::open(path).and_then(|project| project.source());
        match source {
            Ok(source) => self.add_member_source(alias, &source),
            Err(error) => self.status = error,
        }
    }

    fn add_blank_member(&mut self) {
        let source = (|| {
            let mut project =
                modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w"))
                    .map_err(|error| error.to_string())?;
            project.manifest.id = self.member_id_input.clone();
            project.manifest.title = "NEW MEMBER".to_owned();
            modl_core::encode_workcart(&project.manifest, &project.files, &[], &[])
                .map_err(|error| error.to_string())
        })();
        match source {
            Ok(source) => self.add_member_source(&self.member_alias_input.clone(), &source),
            Err(error) => self.status = error,
        }
    }

    fn remove_member(&mut self, alias: &str, context: &egui::Context) {
        if !self.composition_ready() {
            return;
        }
        let Some(mut candidate) = self.project.clone() else {
            return;
        };
        let mut removed = candidate.clone();
        if let Err(error) = removed
            .select_member(alias)
            .and_then(|()| candidate.remove_member(alias))
        {
            self.status = error;
            return;
        }
        self.install_composition(candidate);
        self.asset_sessions.discard_member(alias);
        self.source_editor.forget_member(context, &removed);
        self.breakpoints.retain(|point| point.member != alias);
        self.member_remove = None;
        self.status = format!("REMOVED {alias} FROM GAME / PLAYER SAVES UNCHANGED");
    }

    fn members_window(&mut self, context: &egui::Context) {
        if !self.show_members {
            return;
        }
        let mut open = true;
        egui::Window::new("GAME CARTRIDGES").open(&mut open).default_size([580.0, 480.0])
            .max_height((context.content_rect().height() - 80.0).max(220.0)).vscroll(true).show(context, |ui| {
            ui.label("Local aliases are embedded source, never filesystem paths or downloads.");
            if self.running { ui.colored_label(ui_theme::AMBER, "STOP THE GAME TO ADD OR REMOVE MEMBERS"); }
            let members = self.project.as_ref().map_or_else(Vec::new, NativeProject::members);
            egui::ScrollArea::vertical().id_salt("member-roster").max_height(180.0).show(ui, |ui| {
                for (alias, id) in &members {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(alias).color(ui_theme::AMBER)); ui.label(id);
                        if ui.small_button("EDIT").clicked() { self.select_member(alias); }
                        if ui.add_enabled(!self.running && alias != "main", egui::Button::new("REMOVE")).clicked() { self.member_remove = Some(alias.clone()); }
                    });
                }
            });
            ui.separator();
            ui.label(format!("{}/32 CARTRIDGES INCLUDING MAIN", members.len()));
            ui.add_enabled_ui(!self.running && members.len() < 32, |ui| {
                ui.horizontal_wrapped(|ui| { ui.label("NEW LOCAL ALIAS"); ui.add(egui::TextEdit::singleline(&mut self.member_alias_input).char_limit(32).desired_width(190.0)); });
                ui.label("Import preserves the cartridge's ID; duplicate IDs and nested games are refused.");
                if ui.button("IMPORT LOCAL WORKCART / CARTRIDGE").clicked() && let Some(path) = rfd::FileDialog::new().add_filter("Single MOD-01 cartridge", &["m01w", "m01c", "png"]).pick_file() {
                    self.import_member_path(&self.member_alias_input.clone(), &path);
                }
                ui.horizontal_wrapped(|ui| { ui.label("BLANK MEMBER ID"); ui.add(egui::TextEdit::singleline(&mut self.member_id_input).char_limit(64).desired_width(190.0)); });
                if ui.button("ADD BLANK MEMBER").clicked() { self.add_blank_member(); }
            });
            if let Some(alias) = self.member_remove.clone() {
                ui.separator();
                ui.colored_label(ui_theme::CORAL, format!("REMOVE {alias} FROM THIS GAME?"));
                ui.label("Unsaved source in this member is removed too. No input file or player save is deleted. References/computed request targets may block removal.");
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(!self.running, egui::Button::new("CONFIRM REMOVE")).clicked() { self.remove_member(&alias, context); }
                    if ui.button("CANCEL").clicked() { self.member_remove = None; }
                });
            }
            ui.separator();
            ui.label(&self.status);
        });
        self.show_members = open;
        if !open {
            self.member_remove = None;
        }
    }

    fn save(&mut self) {
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty) {
            if let Err(error) = self.save_sprite() {
                self.status = error;
                return;
            }
        }
        if self
            .project
            .as_ref()
            .is_some_and(|project| project.root.is_none() && project.workcart_path.is_none())
        {
            self.save_as();
            return;
        }
        let Some(project) = self.project.as_mut() else {
            return;
        };
        project.sync_editor();
        match project.save() {
            Ok(()) => {
                project.dirty = false;
                self.status = "PROJECT SAVED".to_owned();
            }
            Err(error) => self.status = error,
        }
    }

    fn save_as(&mut self) {
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty) {
            if let Err(error) = self.save_sprite() {
                self.status = error;
                return;
            }
        }
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let occupied = match fs::read_dir(&path) {
            Ok(mut entries) => entries.next().is_some(),
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        if occupied {
            self.status = "SAVE AS NEEDS AN EMPTY FOLDER".to_owned();
            return;
        }
        let Some(project) = self.project.as_mut() else {
            return;
        };
        project.sync_editor();
        let mut folder_project = project.clone();
        folder_project.workcart_path = Some(path.join("cart.m01w"));
        folder_project.root = Some(path);
        match folder_project.save() {
            Ok(()) => {
                folder_project.dirty = false;
                *project = folder_project;
                self.status = "PROJECT SAVED".to_owned();
            }
            Err(error) => self.status = error,
        }
    }

    fn check(&mut self) {
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty) {
            if let Err(error) = self.save_sprite() {
                self.status = error;
                return;
            }
        }
        let Some(project) = self.project.as_mut() else {
            return;
        };
        project.sync_editor();
        self.status = project.check().unwrap_or_else(|error| error);
    }

    fn open_explorer(&mut self) {
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty)
            && let Err(error) = self.save_sprite()
        {
            self.status = error;
            return;
        }
        let Some(project) = self.project.as_mut() else {
            return;
        };
        project.sync_editor();
        match modl_core::compile_project(
            &project.manifest_source,
            &project.files,
            modl_core::CompileMode::Release,
        ) {
            Ok(compilation) => {
                let mut data = serde_json::to_value(compilation).unwrap_or_default();
                data["packedBytes"] = json!(project.pack().map_or(0, |bytes| bytes.len()));
                self.explorer_data = Some(data);
                self.show_explorer = true;
                self.explorer_tab = "TOKENS".to_owned();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn help_windows(&mut self, context: &egui::Context) {
        if self.show_manual {
            let mut open = true;
            egui::Window::new("MODL/1 MANUAL")
                .open(&mut open)
                .default_size([900.0, 650.0])
                .show(context, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("SEARCH");
                        ui.text_edit_singleline(&mut self.manual_query);
                    });
                    let topics = manual_topics();
                    ui.columns(2, |columns| {
                        egui::ScrollArea::vertical().show(&mut columns[0], |ui| {
                            for (index, (title, body)) in topics.iter().enumerate() {
                                if !self.manual_query.is_empty()
                                    && !format!("{title} {body}")
                                        .to_lowercase()
                                        .contains(&self.manual_query.to_lowercase())
                                {
                                    continue;
                                }
                                if ui
                                    .selectable_label(self.manual_topic == index, *title)
                                    .clicked()
                                {
                                    self.manual_topic = index;
                                }
                            }
                        });
                        egui::ScrollArea::both().show(&mut columns[1], |ui| {
                            let (title, body) = topics[self.manual_topic.min(topics.len() - 1)];
                            ui.heading(title);
                            ui.add(egui::Label::new(body).wrap());
                        });
                    });
                });
            self.show_manual = open;
        }
        if self.show_explorer {
            let mut open = true;
            egui::Window::new("COMPILER EXPLORER")
                .open(&mut open)
                .default_size([1000.0, 700.0])
                .show(context, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for tab in [
                            "GRAPH", "TOKENS", "AST", "TYPED", "IR", "JS", "MAP", "DIAG", "SIZE",
                        ] {
                            if ui.selectable_label(self.explorer_tab == tab, tab).clicked() {
                                self.explorer_tab = tab.to_owned();
                            }
                        }
                    });
                    if let Some(data) = &self.explorer_data {
                        let selected = match self.explorer_tab.as_str() {
                            "GRAPH" => &data["generated"]["relationships"],
                            "TOKENS" => &data["analysis"]["tokens"],
                            "AST" => &data["analysis"]["module"],
                            "TYPED" => &data["analysis"]["symbols"],
                            "IR" => &data["analysis"]["ir"],
                            "JS" => &data["generated"]["javascript"],
                            "MAP" => &data["generated"]["source_map_json"],
                            "DIAG" => &data["analysis"]["diagnostics"],
                            "SIZE" => &data["packedBytes"],
                            _ => &Value::Null,
                        };
                        let output = selected.as_str().map_or_else(
                            || serde_json::to_string_pretty(selected).unwrap_or_default(),
                            str::to_owned,
                        );
                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.code(output);
                        });
                    }
                });
            self.show_explorer = open;
        }
        self.save_manager_window(context);
    }

    fn save_manager_window(&mut self, context: &egui::Context) {
        if !self.show_save_manager {
            return;
        }
        let Some(id) = self
            .project
            .as_ref()
            .map(|project| project.manifest.id.clone())
        else {
            self.show_save_manager = false;
            return;
        };
        let bytes = read_save(&id);
        let schema = read_save_schema(&id);
        let mut action = None;
        let mut open = true;
        egui::Window::new("SAVE MANAGER")
            .open(&mut open)
            .default_size([520.0, 260.0])
            .show(context, |ui| {
                ui.heading(&id);
                match (&bytes, &schema) {
                    (Ok(bytes), Ok(schema)) => {
                        ui.label(format!("SCHEMA {schema} / {} OF 8192 BYTES", bytes.len()));
                        ui.label("Current .m01save export / checksum verified import");
                    }
                    (Err(error), _) | (_, Err(error)) => {
                        ui.label(error);
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("EXPORT .M01SAVE").clicked() {
                        action = Some("export");
                    }
                    if ui.button("IMPORT .M01SAVE").clicked() {
                        action = Some("import");
                    }
                    if ui
                        .button(if self.confirm_save_action == Some("reset") {
                            "CONFIRM RESET"
                        } else {
                            "RESET"
                        })
                        .clicked()
                    {
                        action = Some("reset");
                    }
                    if ui
                        .button(if self.confirm_save_action == Some("delete") {
                            "CONFIRM DELETE"
                        } else {
                            "DELETE"
                        })
                        .clicked()
                    {
                        action = Some("delete");
                    }
                });
                ui.label("Reset and delete retain a local recovery copy.");
            });
        self.show_save_manager = open;
        if let Some(action) = action {
            self.manage_save(&id, action);
        }
    }

    fn manage_save(&mut self, id: &str, action: &'static str) {
        match action {
            "export" => {
                let result = read_save(id).and_then(|bytes| {
                    let schema = read_save_schema(id)?;
                    let stem = modl_core::cartridge::cartridge_file_stem(id)
                        .map_err(|error| error.to_string())?;
                    save_manager::export(id, schema, &bytes).map(|bytes| (stem, bytes))
                });
                match result {
                    Ok((stem, bytes)) => {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name(format!("{stem}.m01save"))
                            .add_filter("MOD-01 save", &["m01save"])
                            .save_file()
                        {
                            self.status = fs::write(path, bytes).map_or_else(
                                |error| error.to_string(),
                                |()| "SAVE IMAGE EXPORTED".to_owned(),
                            );
                        }
                    }
                    Err(error) => self.status = error,
                }
            }
            "import" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("MOD-01 save", &["m01save"])
                    .pick_file()
                {
                    let result = save_manager::import_file(id, &path);
                    self.status = match result.and_then(|(schema, bytes)| {
                        self.stop();
                        backup_save(id)?;
                        write_save(id, &bytes)?;
                        write_save_schema(id, schema)
                    }) {
                        Ok(()) => "SAVE IMAGE IMPORTED / CHECKSUM VERIFIED".to_owned(),
                        Err(error) => error,
                    };
                }
                self.confirm_save_action = None;
            }
            "reset" | "delete" => {
                if self.confirm_save_action != Some(action) {
                    self.confirm_save_action = Some(action);
                    self.status =
                        format!("CONFIRM {} / RECOVERY COPY RETAINED", action.to_uppercase());
                    return;
                }
                self.stop();
                self.status = match backup_save(id).and_then(|()| {
                    if action == "reset" {
                        write_save(id, &[])
                    } else {
                        let path = save_path(id)?;
                        match fs::remove_file(path) {
                            Ok(()) => clear_save_schema(id),
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                clear_save_schema(id)
                            }
                            Err(error) => Err(error.to_string()),
                        }
                    }
                }) {
                    Ok(()) => format!(
                        "SAVE {}",
                        if action == "reset" {
                            "RESET"
                        } else {
                            "DELETED"
                        }
                    ),
                    Err(error) => error,
                };
                self.confirm_save_action = None;
            }
            _ => {}
        }
    }

    fn pack(&mut self) {
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty) {
            if let Err(error) = self.save_sprite() {
                self.status = error;
                return;
            }
        }
        let Some(project) = self.project.as_mut() else {
            return;
        };
        project.sync_editor();
        match project.game() {
            Ok(game) => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(format!("{}.m01c", game.file_stem()))
                    .add_filter("MOD-01 cartridge", &["m01c"])
                    .save_file()
                {
                    self.status = match fs::write(path, game.bytes()) {
                        Ok(()) => format!("PACKED {} BYTES", game.bytes().len()),
                        Err(error) => error.to_string(),
                    };
                }
            }
            Err(error) => self.status = error,
        }
    }

    fn export_native_player(&mut self) {
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty)
            && let Err(error) = self.save_sprite()
        {
            self.status = error;
            return;
        }
        let Some(parent) = rfd::FileDialog::new()
            .set_title("Choose a parent directory for a NEW native game folder")
            .pick_folder()
        else {
            return;
        };
        let Some(project) = self.project.as_mut() else {
            return;
        };
        project.sync_editor();
        let result = (|| -> Result<PathBuf, String> {
            let player = std::env::current_exe().map_err(|error| error.to_string())?;
            let tools = modl_core::NativeToolchain::discover(
                Some(&player),
                None,
                Some(&native_host_path()),
                None,
            )
            .map_err(|error| error.to_string())?;
            project.export_native(&parent, &tools)
        })();
        self.status = match result {
            Ok(path) => format!("EXPORTED {}", path.display()),
            Err(error) => error,
        };
    }

    fn export_cartridge_png(&mut self) {
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty)
            && let Err(error) = self.save_sprite()
        {
            self.status = error;
            return;
        }
        let Some(project) = self.project.as_mut() else {
            return;
        };
        project.sync_editor();
        let (stem, png) = match project.cartridge_png() {
            Ok(png) => png,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("{stem}.m01c.png"))
            .save_file()
        {
            self.status = match fs::write(path, png) {
                Ok(()) => "CARTRIDGE PNG EXPORTED".to_owned(),
                Err(error) => error.to_string(),
            };
        }
    }

    fn capture_png(&mut self, scale: u32) {
        let Some(original) = indexed_image(&self.last_pixels) else {
            self.status = "RUN A CARTRIDGE BEFORE CAPTURING".to_owned();
            return;
        };
        let image = if scale == 1 {
            original
        } else {
            image::imageops::resize(
                &original,
                SCREEN_WIDTH as u32 * scale,
                SCREEN_HEIGHT as u32 * scale,
                image::imageops::FilterType::Nearest,
            )
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("mod01-screen-{scale}x.png"))
            .save_file()
        else {
            return;
        };
        self.status = match image.save_with_format(path, image::ImageFormat::Png) {
            Ok(()) => "SCREEN PNG SAVED".to_owned(),
            Err(error) => error.to_string(),
        };
    }

    fn finish_gif(&mut self) {
        self.recording_gif = false;
        if self.gif_frames.is_empty() {
            self.status = "NO GIF FRAMES CAPTURED".to_owned();
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("mod01-capture.gif")
            .save_file()
        else {
            return;
        };
        self.status = (|| -> Result<String, String> {
            let file = fs::File::create(path).map_err(|error| error.to_string())?;
            let mut encoder = image::codecs::gif::GifEncoder::new(file);
            encoder
                .set_repeat(image::codecs::gif::Repeat::Infinite)
                .map_err(|error| error.to_string())?;
            let frames = self
                .gif_frames
                .iter()
                .filter_map(|pixels| indexed_image(pixels))
                .map(|image| {
                    image::Frame::from_parts(
                        image,
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(1000, 30),
                    )
                });
            encoder
                .encode_frames(frames)
                .map_err(|error| error.to_string())?;
            Ok(format!("GIF SAVED / {} FRAMES", self.gif_frames.len()))
        })()
        .unwrap_or_else(|error| error);
    }

    fn finish_replay(&mut self) {
        self.recording_replay = false;
        let bytes = match replay::encode(&self.replay_frames) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("mod01-input.m01rec")
            .save_file()
        else {
            return;
        };
        self.status = match fs::write(path, bytes) {
            Ok(()) => format!("REPLAY SAVED / {} FRAMES", self.replay_frames.len()),
            Err(error) => error.to_string(),
        };
    }

    fn start_replay_recording(&mut self) {
        let debug = self.running && self.debugging;
        self.stop();
        self.replay_playback = None;
        self.replay_frames.clear();
        self.recording_replay = true;
        self.run_mode(debug);
        if !self.running {
            self.recording_replay = false;
        }
    }

    fn load_replay(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("MOD-01 replay", &["m01rec"])
            .pick_file()
        else {
            return;
        };
        self.status = match replay::read(&path) {
            Ok(frames) => {
                self.replay_playback = Some(frames);
                self.recording_replay = false;
                self.run();
                if self.running {
                    "REPLAY LOADED".to_owned()
                } else {
                    self.status.clone()
                }
            }
            Err(error) => error,
        };
    }

    fn selected_audio_document(&self) -> Result<Value, String> {
        self.asset_workbench.as_ref().map_or_else(
            || serde_json::from_str(&self.asset_text).map_err(|error| error.to_string()),
            |editor| Ok(editor.document.clone()),
        )
    }

    fn start_live_audio(&mut self, mode: AudioPreviewMode) {
        let prepared = (|| {
            let project = self.project.as_ref().ok_or("no project is open")?;
            let document = self.selected_audio_document()?;
            let cursor = self
                .asset_workbench
                .as_ref()
                .and_then(AssetWorkbench::tracker_cursor);
            let pattern = if matches!(mode, AudioPreviewMode::Pattern) {
                Some(
                    cursor
                        .as_ref()
                        .ok_or("select a tracker pattern")?
                        .0
                        .as_str(),
                )
            } else {
                None
            };
            let bank = live_audio::project_bank(project, &self.asset_name, &document, pattern)?;
            let note = if matches!(mode, AudioPreviewMode::Note) {
                if document["kind"] == "music" {
                    let (pattern, row, channel) = cursor.as_ref().ok_or("select a tracker cell")?;
                    let cell = &document["patterns"][pattern]["rows"][*row][*channel];
                    let sound = cell["sound"]
                        .as_str()
                        .ok_or("select a non-empty tracker cell")?
                        .to_owned();
                    let note = cell["note"]
                        .as_f64()
                        .ok_or("tracker note must be numeric")?;
                    let volume = cell.get("volume").map_or(Ok(1.0), |value| {
                        value.as_f64().ok_or("tracker volume must be numeric")
                    })?;
                    Some((sound, note, volume))
                } else {
                    Some((
                        self.asset_name.clone(),
                        document["note"]
                            .as_f64()
                            .ok_or("patch note must be numeric")?,
                        1.0,
                    ))
                }
            } else {
                None
            };
            Ok::<_, String>((bank, note, cursor.map_or(0, |cursor| cursor.1)))
        })();
        match prepared {
            Ok((bank, Some((sound, note, volume)), _)) => {
                self.live_audio.audition(bank, &sound, note, volume)
            }
            Ok((bank, None, row)) => self.live_audio.play(
                bank,
                &self.asset_name,
                if let AudioPreviewMode::Song(order) = mode {
                    order
                } else {
                    0
                },
                if matches!(mode, AudioPreviewMode::Pattern) {
                    row
                } else {
                    0
                },
            ),
            Err(error) => self.status = format!("AUDIO PREVIEW ERROR: {error}"),
        }
    }

    fn render_asset_wav(&mut self) {
        if self.preview_pending || self.preview_cancelling {
            return;
        }
        let prepared = (|| {
            let project = self.project.as_ref().ok_or("no project is open")?;
            let selected = self.selected_audio_document()?;
            let bank = live_audio::project_bank(project, &self.asset_name, &selected, None)?;
            let asset = bank
                .iter()
                .find(|asset| asset["name"] == self.asset_name)
                .cloned()
                .ok_or("selected audio asset is missing")?;
            let sounds = bank
                .into_iter()
                .filter(|asset| asset["kind"] == "sound")
                .collect::<Vec<_>>();
            Ok::<_, String>((asset, sounds))
        })();
        let (asset, sounds) = match prepared {
            Ok(values) => values,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let kind = asset["kind"].clone();
        self.preview_pending = true;
        self.preview_name = self.asset_name.clone();
        self.status = "RENDERING PRODUCTION SYNTH WAV".to_owned();
        if let Err(error) = self
            .preview_runtime
            .send(json!({"type":"render-wav", "kind":kind, "asset":asset, "sounds":sounds}))
        {
            self.status = error;
            self.preview_pending = false;
        }
    }

    fn process_preview(&mut self, context: &egui::Context) {
        while let Some(event) = self.preview_runtime.poll() {
            match event["type"].as_str() {
                Some("wav") if self.preview_pending => {
                    self.preview_pending = false;
                    let Some(bytes) = event["bytes"]
                        .as_str()
                        .and_then(|encoded| STANDARD.decode(encoded).ok())
                    else {
                        self.status = "INVALID WAV RESPONSE".to_owned();
                        continue;
                    };
                    if bytes.len() < 44 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
                        self.status = "INVALID WAV HEADER".to_owned();
                        continue;
                    }
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(export_names::asset(&self.preview_name, "wav"))
                        .save_file()
                    {
                        self.status = match fs::write(path, bytes) {
                            Ok(()) => "WAV SAVED".to_owned(),
                            Err(error) => error.to_string(),
                        };
                    }
                }
                Some("error") if self.preview_pending => {
                    self.preview_pending = false;
                    self.status = event["message"]
                        .as_str()
                        .unwrap_or("WAV EXPORT FAILED")
                        .to_owned();
                }
                Some("stopped") => self.preview_cancelling = false,
                _ => {}
            }
        }
        if self.preview_pending || self.preview_cancelling {
            context.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn run(&mut self) {
        self.run_mode(false);
    }

    fn run_mode(&mut self, debug: bool) {
        self.show_library = false;
        self.touch_controller.clear();
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty) {
            if let Err(error) = self.save_sprite() {
                self.status = error;
                return;
            }
        }
        let Some(project) = self.project.as_mut() else {
            return;
        };
        project.sync_editor();
        let game = match project.game() {
            Ok(game) => game,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let members = match RunningMember::catalog(&game) {
            Ok(members) => members,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let saves = members
            .values()
            .map(|member| read_save(&member.id).map(|save| (member.id.clone(), save)))
            .collect::<Result<BTreeMap<_, _>, _>>();
        let saves = match saves {
            Ok(saves) => saves,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let command = match NativeProject::game_command(&game, debug, &saves) {
            Ok(command) => command,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if let Some(audio) = &self.audio {
            audio.clear();
        }
        self.texture = None;
        self.screen_rect = None;
        match self.runtime.send(command) {
            Ok(()) => {
                self.running_members = members;
                self.active_alias = "main".to_owned();
                self.running = true;
                self.keyboard_controller.quarantine();
                self.gamepad_controller.quarantine();
                self.pause_menu = pause_menu::PauseMenu::default();
                self.pending_menu_action = None;
                self.resume_debug = false;
                self.player_paused = false;
                self.debugging = debug;
                self.snapshot = None;
                self.debug_event.clear();
                self.debug_source = None;
                self.debug_inspection = Value::Null;
                self.debug_trace.clear();
                self.watch_results.clear();
                self.debug_running = false;
                self.debug_snapshots.clear();
                self.loaded = false;
                self.pending = true;
                self.frame = 0;
                self.input_frame = 0;
                self.frame_input = None;
                self.debug_booted = !debug;
                self.debug_frame_boundary = true;
                if self.recording_replay {
                    self.replay_frames.clear();
                }
                self.last_pixels.clear();
                self.status = if debug {
                    "LOADING DEBUG CARTRIDGE"
                } else {
                    "LOADING CARTRIDGE"
                }
                .to_owned();
                self.view = View::Player;
            }
            Err(error) => self.status = error,
        }
    }

    fn stop(&mut self) {
        self.keyboard_controller.quarantine();
        self.gamepad_controller.quarantine();
        self.pending_menu_action = None;
        self.resume_debug = false;
        self.pause_menu = pause_menu::PauseMenu::default();
        self.touch_controller.clear();
        self.live_audio.stop();
        if self.preview_pending && !self.preview_cancelling {
            self.preview_pending = false;
            self.preview_cancelling = true;
            let _ = self.preview_runtime.send(json!({"type":"stop"}));
        }
        self.running = false;
        self.player_paused = false;
        self.debug_running = false;
        self.loaded = false;
        self.pending = false;
        self.frame_input = None;
        if let Some(audio) = &self.audio {
            audio.clear();
        }
        let _ = self.runtime.send(json!({"type":"stop"}));
    }

    fn set_player_paused(&mut self, paused: bool) {
        if paused && !self.player_paused {
            self.resume_debug = self.debug_running;
            self.debug_running = false;
            self.pause_menu.reset_selection();
        } else if !paused && self.player_paused {
            self.debug_running = self.resume_debug;
            self.resume_debug = false;
        }
        self.player_paused = paused;
        self.touch_controller.clear();
        self.keyboard_controller.quarantine();
        self.gamepad_controller.quarantine();
        if let Some(audio) = &self.audio {
            audio.clear();
        }
        self.last_tick = Instant::now();
    }

    fn process_events(&mut self, context: &egui::Context) {
        while let Some(event) = self.runtime.poll() {
            self.pending = false;
            if let Some(items) = event.get("menuItems") {
                if let Err(error) = self.pause_menu.update_items(items) {
                    self.status = error;
                }
            }
            match event["type"].as_str() {
                Some("loaded") => {
                    self.accept_active(&event);
                    self.loaded = true;
                    self.status = format!(
                        "RUNNING {}",
                        event["cartridgeId"].as_str().unwrap_or("CARTRIDGE")
                    );
                    self.last_tick = Instant::now()
                        .checked_sub(Duration::from_millis(17))
                        .unwrap_or_else(Instant::now);
                }
                Some("frame") => self.accept_frame(&event, context),
                Some("step") => {
                    if event["frame"].is_object() {
                        self.accept_frame(&event["frame"], context);
                        self.status = format!("DEBUG FRAME {} COMPLETE", self.frame);
                    } else if event["booted"] == true {
                        self.accept_active(&event);
                        self.debug_booted = true;
                        self.debug_frame_boundary = true;
                        self.status = "DEBUG BOOT COMPLETE".to_owned();
                    } else if event["event"].is_object() {
                        self.accept_active(&event);
                        self.debug_inspection = event["inspection"].clone();
                        self.watch_results =
                            event["watches"].as_array().cloned().unwrap_or_default();
                        let mut source_event = event["event"].clone();
                        source_event["cartridgeAlias"] = event["cartridgeAlias"].clone();
                        source_event["cartridgeId"] = event["cartridgeId"].clone();
                        let alias = event["cartridgeAlias"].as_str().unwrap_or("").to_owned();
                        let path = event["event"]["sourceSpan"]["source"]
                            .as_str()
                            .or_else(|| {
                                self.running_members
                                    .get(&alias)
                                    .map(|member| member.entry.as_str())
                            })
                            .unwrap_or("")
                            .to_owned();
                        let offset =
                            event["event"]["sourceSpan"]["start"].as_u64().unwrap_or(0) as usize;
                        self.debug_source = Some((alias, path, offset));
                        self.debug_event = serde_json::to_string_pretty(&source_event)
                            .unwrap_or_else(|_| "debug event".to_owned());
                        self.debug_trace.push(source_event);
                        if self.debug_trace.len() > 4096 {
                            self.debug_trace.remove(0);
                        }
                        self.status = "PAUSED AT SOURCE STEP".to_owned();
                        if self.hit_breakpoint(&event, &event["conditions"]) {
                            self.debug_running = false;
                            self.resume_debug = false;
                            self.status = "BREAKPOINT".to_owned();
                        }
                    }
                }
                Some("snapshot") => {
                    self.snapshot = event["snapshot"].as_str().map(str::to_owned);
                    if let Some(snapshot) = &self.snapshot {
                        self.debug_snapshots.push((
                            event["frame"]
                                .as_u64()
                                .unwrap_or(u64::from(self.input_frame))
                                as u32,
                            snapshot.clone(),
                        ));
                        if self.debug_snapshots.len() > 120 {
                            self.debug_snapshots.remove(0);
                        }
                    }
                    self.status = "DEBUG SNAPSHOT CAPTURED".to_owned();
                }
                Some("restored") => {
                    self.accept_active(&event);
                    self.debug_source = None;
                    self.debug_event.clear();
                    self.debug_inspection = Value::Null;
                    self.watch_results.clear();
                    self.touch_controller.clear();
                    self.keyboard_controller.quarantine();
                    self.gamepad_controller.quarantine();
                    self.pending_menu_action = None;
                    self.resume_debug = false;
                    self.input_frame = event["frame"].as_u64().unwrap_or(0) as u32;
                    self.frame = self.input_frame.saturating_sub(1);
                    self.frame_input = None;
                    self.debug_booted = event["booted"] == true;
                    self.debug_frame_boundary = true;
                    self.debug_running = false;
                    self.replay_frames
                        .retain(|(frame, _)| *frame < self.input_frame);
                    self.status = "DEBUG SNAPSHOT RESTORED".to_owned();
                    if let Some(audio) = &self.audio {
                        audio.clear();
                    }
                }
                Some("stopped") => {}
                Some("memory") => {
                    self.memory_view = event["bytes"]
                        .as_str()
                        .and_then(|encoded| STANDARD.decode(encoded).ok())
                        .map_or_else(
                            || "INVALID MEMORY RESPONSE".to_owned(),
                            |bytes| {
                                bytes
                                    .chunks(16)
                                    .enumerate()
                                    .map(|(row, bytes)| {
                                        format!(
                                            "{:06X}: {}",
                                            self.memory_address as usize + row * 16,
                                            bytes
                                                .iter()
                                                .map(|byte| format!("{byte:02X}"))
                                                .collect::<Vec<_>>()
                                                .join(" ")
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            },
                        );
                }
                Some("memory-edited") => self.status = "MEMORY UPDATED".to_owned(),
                Some("error") => {
                    if event["recoverable"] == true && self.running && self.loaded {
                        // Rejected debugger operations did not mutate the VM. Keep the
                        // suspended source/input/replay/menu state; pause auto-stepping
                        // so the user can inspect the diagnostic before resuming.
                        self.debug_running = false;
                        self.resume_debug = false;
                        self.status = format!(
                            "DEBUG REQUEST REJECTED: {}",
                            event["message"]
                                .as_str()
                                .unwrap_or("invalid debugger request")
                        );
                        continue;
                    }
                    if let Some(alias) = event["active"]["cartridgeAlias"].as_str()
                        && let Some(member) = self.running_members.get(alias)
                        && event["sourceSpan"].is_object()
                    {
                        let path = event["sourceSpan"]["source"]
                            .as_str()
                            .unwrap_or(&member.entry)
                            .to_owned();
                        self.debug_source = Some((
                            alias.to_owned(),
                            path,
                            event["sourceSpan"]["start"].as_u64().unwrap_or(0) as usize,
                        ));
                    }
                    self.set_player_paused(false);
                    self.debug_running = false;
                    self.resume_debug = false;
                    self.pending_menu_action = None;
                    self.status = event["message"]
                        .as_str()
                        .unwrap_or("runtime error")
                        .to_owned();
                    self.running = false;
                    self.loaded = false;
                }
                _ => self.status = format!("UNEXPECTED RUNTIME MESSAGE: {}", event["type"]),
            }
        }
        self.pending = self.runtime.has_pending();
    }

    fn hit_breakpoint(&self, event: &Value, conditions: &Value) -> bool {
        let alias = event["cartridgeAlias"].as_str().unwrap_or("");
        let Some(member) = self.running_members.get(alias) else {
            return false;
        };
        let source_event = &event["event"];
        let source = source_event["sourceSpan"]["source"]
            .as_str()
            .unwrap_or(&member.entry);
        let offset = source_event["sourceSpan"]["start"].as_u64().unwrap_or(0) as usize;
        let Some(bytes) = member.sources.get(source) else {
            return false;
        };
        let line = 1 + bytes
            .iter()
            .take(offset)
            .filter(|byte| **byte == b'\n')
            .count();
        self.breakpoints.iter().enumerate().any(|(index, point)| {
            point.member == alias
                && point.source == source
                && point.line == line
                && (point.condition.is_empty()
                    || conditions[index]["value"] == true
                    || conditions[index]["error"].is_string())
        })
    }

    fn reveal_debug_source(&mut self) {
        let Some((alias, path, offset)) = self.debug_source.clone() else {
            return;
        };
        self.debug_running = false;
        self.resume_debug = false;
        let previous_view = self.view;
        self.select_member(&alias);
        let Some(project) = self
            .project
            .as_mut()
            .filter(|project| project.member_alias() == alias)
        else {
            return;
        };
        project.sync_editor();
        let frozen = self
            .running_members
            .get(&alias)
            .and_then(|member| member.sources.get(&path));
        if frozen.is_none() || project.files.get(&path) != frozen {
            self.view = previous_view;
            self.status =
                "EDITED SOURCE DIFFERS FROM RUNNING BUILD / READ-ONLY SOURCE SHOWN IN DEBUGGER"
                    .to_owned();
            return;
        }
        match self.source_editor.reveal(project, &path, offset) {
            Ok(()) => {
                self.view = View::Code;
                self.status = format!("RUNNING SOURCE {alias}/{path}");
            }
            Err(error) => self.status = error,
        }
    }

    fn accept_frame(&mut self, event: &Value, context: &egui::Context) {
        let alias = event["cartridgeAlias"].as_str().unwrap_or("");
        let id = event["cartridgeId"].as_str().unwrap_or("");
        if self
            .running_members
            .get(alias)
            .map(|member| member.id.as_str())
            != Some(id)
        {
            self.status = "INVALID RUNTIME FRAME OWNER".to_owned();
            self.running = false;
            self.loaded = false;
            return;
        }
        self.accept_active(event);
        if let Some(items) = event.get("menuItems") {
            if let Err(error) = self.pause_menu.update_items(items) {
                self.status = error;
            }
        }
        self.frame = event["frame"].as_u64().unwrap_or(0) as u32;
        self.input_frame = self.frame.saturating_add(1);
        self.debug_frame_boundary = true;
        if let Some(input) = self.frame_input.take()
            && self.recording_replay
            && self.replay_frames.len() < 36000
        {
            self.replay_frames.push((self.frame, input));
        }
        self.work = event["workUnits"].as_u64().unwrap_or(0) as u32;
        if let Some(encoded) = event["pixels"].as_str() {
            if let Ok(pixels) = STANDARD.decode(encoded) {
                if pixels.len() == SCREEN_WIDTH * SCREEN_HEIGHT {
                    self.last_pixels = pixels.clone();
                    if self.recording_gif && self.frame % 2 == 0 && self.gif_frames.len() < 150 {
                        self.gif_frames.push(pixels.clone());
                    }
                    let mut rgba = Vec::with_capacity(pixels.len() * 4);
                    let palette = sprite::palette();
                    for index in pixels {
                        let color = palette[usize::from(index)];
                        rgba.extend_from_slice(&[color[0], color[1], color[2], 255]);
                    }
                    let image =
                        ColorImage::from_rgba_unmultiplied([SCREEN_WIDTH, SCREEN_HEIGHT], &rgba);
                    if let Some(texture) = self.texture.as_mut() {
                        texture.set(image, TextureOptions::NEAREST);
                    } else {
                        self.texture = Some(context.load_texture(
                            "mod01-screen",
                            image,
                            TextureOptions::NEAREST,
                        ));
                    }
                }
            }
        }
        if !self.player_paused
            && let (Some(audio), Some(left), Some(right)) =
                (&self.audio, event["left"].as_str(), event["right"].as_str())
        {
            if let Err(error) = audio.push(left, right) {
                self.status = format!("AUDIO: {error}");
            }
        }
        if !self.debugging
            && self.replay_playback.is_none()
            && let Some(encoded) = event["saveCommit"].as_str()
        {
            if let Ok(bytes) = STANDARD.decode(encoded) {
                if let Err(error) = write_save(id, &bytes) {
                    self.status = error;
                }
            }
        }
    }

    fn accept_active(&mut self, event: &Value) {
        let active = &event["active"];
        let alias = active["cartridgeAlias"].as_str().unwrap_or("");
        let id = active["cartridgeId"].as_str().unwrap_or("");
        if self
            .running_members
            .get(alias)
            .map(|member| member.id.as_str())
            == Some(id)
        {
            if self.active_alias != alias && !self.debugging {
                self.status = format!("RUNNING {alias} / {id}");
            }
            self.active_alias = alias.to_owned();
            self.debug_booted = active["booted"] == true;
        }
    }

    fn runtime_title(&self) -> String {
        self.running
            .then(|| {
                self.running_members
                    .get(&self.active_alias)
                    .map(|member| &member.title)
            })
            .flatten()
            .cloned()
            .or_else(|| {
                self.project
                    .as_ref()
                    .map(|project| project.manifest.title.clone())
            })
            .unwrap_or_else(|| "MOD-01".to_owned())
    }

    fn send_frame(&mut self, context: &egui::Context) {
        if !self.running
            || !self.loaded
            || self.pending
            || self.debugging
            || self.player_paused
            || self.view != View::Player
        {
            return;
        }
        if self.last_tick.elapsed() < Duration::from_micros(16_667) {
            return;
        }
        self.last_tick = Instant::now();
        self.send_input(context, "frame");
    }

    fn step(&mut self, context: &egui::Context) {
        if self.running && self.loaded && !self.pending && self.debugging && !self.player_paused {
            self.send_input(context, "step");
        }
    }

    fn debugger_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            for (tab, name) in [
                (DebugTab::Source, "SOURCE"),
                (DebugTab::State, "STATE"),
                (DebugTab::Tasks, "TASKS"),
                (DebugTab::Memory, "MEMORY"),
                (DebugTab::Audio, "AUDIO"),
                (DebugTab::Profile, "PROFILE"),
            ] {
                if ui.selectable_label(self.debug_tab == tab, name).clicked() {
                    self.debug_tab = tab;
                }
            }
        });
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| match self.debug_tab {
            DebugTab::Source => {
                let source = self.project.as_ref().map_or("", |project| project.selected_file.as_str()).to_owned();
                let member = self.project.as_ref().map_or("", |project| project.member_alias()).to_owned();
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("BREAKPOINT {member}/{source}:"));
                    ui.add(egui::DragValue::new(&mut self.breakpoint_line).range(1..=100_000));
                    ui.add_sized([150.0, 22.0], egui::TextEdit::singleline(&mut self.breakpoint_condition).hint_text("CONDITION"));
                    if ui.button("ADD").clicked() && !source.is_empty() && self.breakpoints.len() < 32
                        && !self.breakpoints.iter().any(|point| point.member == member && point.source == source && point.line == self.breakpoint_line) {
                        self.breakpoints.push(SourceBreakpoint { member: member.clone(), source: source.clone(), line: self.breakpoint_line, condition: self.breakpoint_condition.trim().chars().take(256).collect() });
                    }
                });
                let mut remove = None;
                for (index, point) in self.breakpoints.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("{}/{}:{}{}", point.member, point.source, point.line,
                            if point.condition.is_empty() { String::new() } else { format!(" IF {}", point.condition) }));
                        if ui.button("REMOVE").clicked() { remove = Some(index); }
                    });
                }
                if let Some(index) = remove { self.breakpoints.remove(index); }
                if let Some((alias, path, offset)) = self.debug_source.clone() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("RUNNING SOURCE {alias}/{path}"));
                        if ui.button("OPEN EDITABLE SOURCE").clicked() { self.reveal_debug_source(); }
                    });
                    if let Some(bytes) = self.running_members.get(&alias).and_then(|member| member.sources.get(&path)) {
                        let line = bytes.iter().take(offset).filter(|byte| **byte == b'\n').count();
                        let text = String::from_utf8_lossy(bytes);
                        for (index, text) in text.lines().enumerate().skip(line.saturating_sub(2)).take(5) {
                            ui.label(RichText::new(format!("{} {:05} {text}", if index == line { ">" } else { " " }, index + 1)).monospace().color(if index == line { ui_theme::AMBER } else { ui_theme::CREAM }));
                        }
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("WATCH");
                    ui.add_sized([190.0, 22.0], egui::TextEdit::singleline(&mut self.watch_input).hint_text("EXPRESSION"));
                    if ui.button("ADD").clicked() {
                        let expression = self.watch_input.trim().to_owned();
                        if !expression.is_empty() && expression.len() <= 256 && !self.watches.contains(&expression) {
                            self.watches.push(expression);
                            if self.watches.len() > 4 { self.watches.remove(0); }
                            self.watch_input.clear();
                        }
                    }
                });
                let mut remove_watch = None;
                for (index, expression) in self.watches.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let rendered = self.watch_results.get(index).map_or_else(
                            || "PENDING".to_owned(),
                            |result| result["error"].as_str().map_or_else(
                                || result["value"].to_string(), |error| format!("ERROR {error}")),
                        );
                        ui.label(format!("{expression} = {rendered}"));
                        if ui.button("REMOVE").clicked() { remove_watch = Some(index); }
                    });
                }
                if let Some(index) = remove_watch {
                    self.watches.remove(index);
                    self.watch_results.clear();
                }
                ui.label(if self.debug_event.is_empty() { "Press STEP or CONTINUE." } else { &self.debug_event });
            }
            DebugTab::State => {
                ui.label(serde_json::to_string_pretty(&self.debug_inspection["state"]).unwrap_or_default());
            }
            DebugTab::Tasks => {
                ui.label(serde_json::to_string_pretty(&self.debug_inspection["tasks"]).unwrap_or_default());
                ui.label(serde_json::to_string_pretty(&self.debug_inspection["callStack"]).unwrap_or_default());
            }
            DebugTab::Memory => {
                ui.horizontal(|ui| {
                    ui.label("ADDRESS"); ui.add(egui::DragValue::new(&mut self.memory_address).range(0..=0x003f_ffff).hexadecimal(6, false, true));
                    ui.label("BYTES"); ui.add(egui::DragValue::new(&mut self.memory_length).range(1..=256));
                    if ui.button("READ").clicked() {
                        self.pending = self.runtime.send(json!({"type":"memory","address":self.memory_address,"length":self.memory_length})).is_ok();
                    }
                });
                ui.label(&self.memory_view);
                ui.horizontal(|ui| {
                    ui.label("WRITE HEX"); ui.text_edit_singleline(&mut self.memory_edit);
                    if ui.button("WRITE").clicked() {
                        let compact = self.memory_edit.chars().filter(|character| !character.is_whitespace()).collect::<String>();
                        if compact.len() % 2 == 0 && compact.len() <= 512 {
                            let parsed = (0..compact.len()).step_by(2).map(|index| u8::from_str_radix(&compact[index..index+2],16)).collect::<Result<Vec<_>,_>>();
                            match parsed {
                                Ok(bytes) => self.pending = self.runtime.send(json!({"type":"memory-edit","address":self.memory_address,"bytes":STANDARD.encode(bytes)})).is_ok(),
                                Err(error) => self.status = error.to_string(),
                            }
                        } else { self.status = "HEX WRITE NEEDS 1-256 BYTES".to_owned(); }
                    }
                });
            }
            DebugTab::Audio => { ui.label("Native PCM output is active; voice inspection follows the runtime state."); }
            DebugTab::Profile => {
                ui.label(format!("FRAME {} / WORK {} / {} SOURCE EVENTS", self.frame, self.work, self.debug_trace.len()));
                for event in self.debug_trace.iter().rev().take(8) { ui.label(format!("{}", event["sourceSpan"])); }
                ui.label("SAVED DEBUG SNAPSHOTS");
                for (frame, snapshot) in self.debug_snapshots.iter().rev() {
                    if ui.button(format!("RESTORE BEFORE FRAME {frame}")).clicked() {
                        self.pending = self.runtime.send(json!({"type":"restore","snapshot":snapshot})).is_ok();
                        self.debug_running = false;
                    }
                }
            }
        });
    }

    fn capture_keyboard(&mut self, context: &egui::Context) {
        let enabled = self.running
            && self.loaded
            && !self.show_library
            && !self.player_paused
            && (self.player_mode || self.view == View::Player)
            && self.replay_playback.is_none();
        self.keyboard_controller.capture(context, enabled);
        self.gamepad_controller.capture(
            context,
            &mut self.gamepads,
            enabled && !context.text_edit_focused(),
        );
    }

    fn send_input(&mut self, context: &egui::Context, kind: &str) {
        self.capture_keyboard(context);
        let pointer = context.input(|input| {
            let position = input.pointer.hover_pos();
            if let (Some(position), Some(rect)) = (position, self.screen_rect) {
                if rect.contains(position) {
                    return json!({
                        "x": (((position.x - rect.left()) / rect.width() * SCREEN_WIDTH as f32).floor() as u16).min((SCREEN_WIDTH - 1) as u16),
                        "y": (((position.y - rect.top()) / rect.height() * SCREEN_HEIGHT as f32).floor() as u16).min((SCREEN_HEIGHT - 1) as u16),
                        "primary": input.pointer.primary_down(),
                        "secondary": input.pointer.secondary_down(), "inside": true,
                    });
                }
            }
            json!({"x":0,"y":0,"primary":false,"secondary":false,"inside":false})
        });
        let mut live_input = replay::empty_input();
        live_input["pointer"] = pointer;
        self.keyboard_controller.apply_to(&mut live_input);
        self.gamepad_controller.apply_to(&mut live_input);
        self.touch_controller.apply_to(&mut live_input);
        if let Some(action) = self.pending_menu_action {
            live_input = replay::empty_input();
            live_input["menuAction"] = action.into();
        }
        let input = self.frame_input.clone().unwrap_or_else(|| {
            self.replay_playback.as_ref().map_or_else(
                || live_input.clone(),
                |frames| {
                    frames
                        .get(&self.input_frame)
                        .cloned()
                        .unwrap_or_else(replay::empty_input)
                },
            )
        });
        let command = if kind == "step" {
            json!({"type":kind,"input":input,"watches":self.watches,
                "conditions":self.breakpoints.iter().map(|point| if point.member == self.active_alias { point.condition.as_str() } else { "" }).collect::<Vec<_>>()})
        } else {
            json!({"type":kind,"input":input})
        };
        if let Err(error) = self.runtime.send(command) {
            self.status = error;
            self.running = false;
        } else {
            if self.frame_input.is_none() && (kind != "step" || self.debug_booted) {
                self.frame_input = Some(input);
                self.pending_menu_action = None;
                self.touch_controller.consume();
                self.keyboard_controller.consume();
                self.gamepad_controller.consume();
            }
            if kind == "step" {
                self.debug_frame_boundary = false;
            }
            self.pending = true;
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("MOD-01").size(23.0).color(ui_theme::AMBER));
            ui.separator();
            ui_theme::legend(ui, "COLOR DEVELOPMENT UNIT");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui_theme::legend(ui, "REV. 02  /  1999");
            });
        });
        ui.add_space(3.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("LIBRARY").clicked() {
                self.enter_library();
            }
            ui.menu_button("PROJECT", |ui| {
                if ui.button("OPEN PROJECT").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.open(path);
                    }
                    ui.close();
                }
                if ui.button("OPEN CARTRIDGE / WORKCART").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("MOD-01 cartridge or workcart", &["m01c", "png", "m01w"])
                        .pick_file()
                    {
                        self.open(path);
                    }
                    ui.close();
                }
                if ui.button("NEW CARTRIDGE").clicked() {
                    self.show_new = true;
                    ui.close();
                }
                ui.separator();
                ui.add_enabled_ui(self.project.is_some(), |ui| {
                    if ui.button("SAVE").clicked() {
                        self.save();
                        ui.close();
                    }
                    if ui.button("SAVE AS").clicked() {
                        self.save_as();
                        ui.close();
                    }
                    let revisions = self
                        .project
                        .as_ref()
                        .and_then(|project| project.revisions().ok())
                        .unwrap_or_default();
                    ui.menu_button("RECOVER", |ui| {
                        if revisions.is_empty() {
                            ui.label("NO LOCAL REVISIONS");
                        }
                        for path in revisions {
                            let label = path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("REVISION")
                                .to_owned();
                            if ui.button(label).clicked() {
                                self.recover_revision(path);
                                ui.close();
                            }
                        }
                    });
                });
            });
            ui.add_enabled_ui(self.project.is_some(), |ui| {
                ui.menu_button("BUILD", |ui| {
                    if ui.button("CHECK SOURCE").clicked() {
                        self.check();
                        ui.close();
                    }
                    if ui.button("PACK CARTRIDGE").clicked() {
                        self.pack();
                        ui.close();
                    }
                });
            });
            ui.menu_button("TOOLS", |ui| {
                if ui.button("MODL PLAYGROUND / REPL").clicked() {
                    self.show_playground = true;
                    self.source_editor.dismiss_completion();
                    self.playground.focus();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.project.is_some(),
                        egui::Button::new("COMPILER EXPLORER"),
                    )
                    .clicked()
                {
                    self.open_explorer();
                    ui.close();
                }
                if ui
                    .add_enabled(self.project.is_some(), egui::Button::new("SAVE MANAGER"))
                    .clicked()
                {
                    self.show_save_manager = true;
                    self.confirm_save_action = None;
                    ui.close();
                }
            });
            ui.add_enabled_ui(self.project.is_some(), |ui| {
                ui.menu_button("EXPORT", |ui| {
                    if ui.button("NATIVE GAME").clicked() {
                        self.export_native_player();
                        ui.close();
                    }
                    if ui.button("CARTRIDGE PNG").clicked() {
                        self.export_cartridge_png();
                        ui.close();
                    }
                });
                ui.menu_button("CAPTURE", |ui| {
                    for scale in 1..=4 {
                        if ui.button(format!("SCREEN PNG {scale}x")).clicked() {
                            self.capture_png(scale);
                            ui.close();
                        }
                    }
                    if ui
                        .button(if self.recording_gif {
                            "FINISH GIF"
                        } else {
                            "RECORD GIF"
                        })
                        .clicked()
                    {
                        if self.recording_gif {
                            self.finish_gif();
                        } else {
                            self.gif_frames.clear();
                            self.recording_gif = true;
                        }
                        ui.close();
                    }
                    if ui
                        .button(if self.recording_replay {
                            "SAVE REPLAY"
                        } else {
                            "RECORD REPLAY"
                        })
                        .clicked()
                    {
                        if self.recording_replay {
                            self.finish_replay();
                        } else {
                            self.start_replay_recording();
                        }
                        ui.close();
                    }
                    if ui.button("LOAD REPLAY").clicked() {
                        self.load_replay();
                        ui.close();
                    }
                    if self.replay_playback.is_some() && ui.button("CLEAR REPLAY").clicked() {
                        self.replay_playback = None;
                        self.status = "LIVE INPUT RESTORED".to_owned();
                        ui.close();
                    }
                });
                if ui.button("RUN").clicked() {
                    self.run();
                }
                if ui.button("DEBUG").clicked() {
                    self.run_mode(true);
                }
            });
            if ui.button("HELP").clicked() {
                self.show_manual = true;
            }
            if self.running && ui.button("MENU").clicked() {
                self.set_player_paused(!self.player_paused);
            }
            ui.separator();
            if ui
                .add_enabled(
                    self.running || self.live_audio.active() || self.preview_pending,
                    egui::Button::new("STOP"),
                )
                .clicked()
            {
                self.stop();
            }
            if self.debugging && self.running {
                if ui
                    .add_enabled(
                        self.loaded && !self.pending,
                        egui::Button::new(if self.debug_running {
                            "PAUSE"
                        } else {
                            "CONTINUE"
                        }),
                    )
                    .clicked()
                {
                    self.debug_running = !self.debug_running;
                    if self.debug_running {
                        self.step(ui.ctx());
                    }
                }
                if ui
                    .add_enabled(self.loaded && !self.pending, egui::Button::new("STEP"))
                    .clicked()
                {
                    self.step(ui.ctx());
                }
                if ui
                    .add_enabled(
                        self.loaded && !self.pending && self.debug_frame_boundary,
                        egui::Button::new("SNAPSHOT"),
                    )
                    .on_disabled_hover_text("Snapshots require a completed frame or boot boundary")
                    .clicked()
                {
                    self.pending = self.runtime.send(json!({"type":"snapshot"})).is_ok();
                }
                if ui
                    .add_enabled(
                        self.loaded && !self.pending && self.snapshot.is_some(),
                        egui::Button::new("RESTORE"),
                    )
                    .clicked()
                {
                    if let Some(snapshot) = &self.snapshot {
                        self.pending = self
                            .runtime
                            .send(json!({"type":"restore","snapshot":snapshot}))
                            .is_ok();
                    }
                }
            }
        });
    }

    fn sync_asset(&mut self) -> Result<(), String> {
        if let Some(workbench) = &self.asset_workbench {
            if workbench.dirty {
                self.asset_text = workbench.source()?;
                self.asset_dirty = true;
            }
        }
        if !self.asset_dirty {
            return Ok(());
        }
        let _: Value = serde_json::from_str(&self.asset_text)
            .map_err(|error| format!("{}: {error}", self.asset_path))?;
        let project = self.project.as_mut().ok_or("no project is open")?;
        if project.files.get(&self.asset_path).map(Vec::as_slice)
            != Some(self.asset_text.as_bytes())
        {
            project
                .files
                .insert(self.asset_path.clone(), self.asset_text.as_bytes().to_vec());
            project.dirty = true;
        }
        self.asset_dirty = false;
        if let Some(workbench) = self.asset_workbench.as_mut() {
            workbench.dirty = false;
        }
        Ok(())
    }

    // Discard project-local UI state when replacing the current cartridge.
    fn reset_asset_editors(&mut self) {
        self.show_members = false;
        self.member_remove = None;
        self.asset_sessions.clear();
        self.asset_remove = None;
        self.asset_meter = asset_meter::AssetMeter::default();
        self.asset_workbench = None;
        self.sprite = None;
        self.asset_name.clear();
        self.asset_path.clear();
        self.asset_text.clear();
        self.asset_dirty = false;
        self.sprite_texture = None;
        self.sprite_painting = false;
        self.last_sprite_point = None;
        self.selection_anchor = None;
    }

    // Call only after current buffers have been synchronized into project memory.
    fn cache_asset_editors(&mut self) {
        let Some(project) = &self.project else { return };
        let mut retained = true;
        if let Some(editor) = self.asset_workbench.take() {
            retained &= self.asset_sessions.stash_structured(
                project,
                &self.asset_name,
                &self.asset_path,
                editor,
            );
        }
        if let Some(editor) = self.sprite.take() {
            retained &= self.asset_sessions.stash_sprite(project, editor);
        }
        self.asset_name.clear();
        self.asset_path.clear();
        self.asset_text.clear();
        self.asset_dirty = false;
        self.sprite_texture = None;
        self.sprite_painting = false;
        self.last_sprite_point = None;
        self.selection_anchor = None;
        if !retained {
            self.status =
                "ASSET RETAINED IN PROJECT / SESSION EXCEEDS HISTORY CACHE LIMIT".to_owned();
        }
    }

    fn asset_meter_ui(&mut self, ui: &mut egui::Ui) {
        let now = Instant::now();
        if self.asset_meter.due(now)
            && let Some(project) = &self.project
        {
            let overlay = if let Some(editor) = self.sprite.as_ref().filter(|editor| editor.dirty) {
                let source = serde_json::to_vec(&json!({"revision":2,"kind":"sprite","width":editor.document.width,"height":editor.document.height,"frames":editor.document.indexed_frames()})).expect("indexed sprite is serializable");
                Some(asset_meter::Overlay {
                    path: editor.asset_path.clone(),
                    source,
                    declaration: Some((
                        editor.name.clone(),
                        if editor.document.frames.len() > 1 {
                            modl_core::AssetKind::Animation
                        } else {
                            modl_core::AssetKind::Sprite
                        },
                    )),
                })
            } else if let Some(editor) = self.asset_workbench.as_ref().filter(|editor| editor.dirty)
            {
                Some(asset_meter::Overlay {
                    path: self.asset_path.clone(),
                    source: editor
                        .source()
                        .expect("asset JSON is serializable")
                        .into_bytes(),
                    declaration: None,
                })
            } else if self.asset_dirty {
                Some(asset_meter::Overlay {
                    path: self.asset_path.clone(),
                    source: self.asset_text.as_bytes().to_vec(),
                    declaration: None,
                })
            } else {
                None
            };
            self.asset_meter.refresh(project, overlay, now);
        }
        if let Some(name) = self.asset_meter.ui(ui) {
            if name == "DISPLAY" {
                self.open_display();
            } else if name != "BANK" {
                self.open_asset(&name);
            }
        }
        ui.separator();
    }

    fn import_asset_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "MOD-01 asset",
                &["m01g", "m01m", "m01f", "m01s", "m01t", "m01p", "json"],
            )
            .pick_file()
        else {
            return;
        };
        let source = match fs::read_to_string(&path) {
            Ok(source) if source.len() <= 16 * 1024 * 1024 => source,
            Ok(_) => {
                self.status = "ASSET FILE EXCEEDS 16 MIB".to_owned();
                return;
            }
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let Some(current) = self.asset_workbench.as_ref() else {
            self.status = "OPEN A STRUCTURED ASSET FIRST".to_owned();
            return;
        };
        let parsed: Value = match serde_json::from_str(&source) {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let expected = if self.asset_name == "DISPLAY" {
            "display".to_owned()
        } else {
            serde_json::to_value(current.kind)
                .unwrap_or_default()
                .as_str()
                .unwrap_or("")
                .to_owned()
        };
        if parsed["kind"].as_str() != Some(&expected) {
            self.status = format!("ASSET KIND MUST BE {expected}");
            return;
        }
        if let Some(current) = self.asset_workbench.as_mut() {
            current.replace_document(parsed);
            self.asset_text = source;
            self.asset_dirty = true;
            self.status = format!("IMPORTED {} / UNDO AVAILABLE", path.display());
        }
    }

    fn export_asset_file(&mut self) {
        let source = match &self.asset_workbench {
            Some(editor) => match editor.source() {
                Ok(source) => source,
                Err(error) => {
                    self.status = error;
                    return;
                }
            },
            None => self.asset_text.clone(),
        };
        let name = export_names::source(&self.asset_name, &self.asset_path);
        if let Some(path) = rfd::FileDialog::new().set_file_name(name).save_file() {
            self.status = match fs::write(&path, source) {
                Ok(()) => format!("EXPORTED {}", path.display()),
                Err(error) => error.to_string(),
            };
        }
    }

    fn import_tile_png(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG atlas", &["png"])
            .pick_file()
        else {
            return;
        };
        let bytes = match modl_core::asset_import::read_png_source(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        self.status = match self.asset_workbench.as_mut() {
            Some(workbench) => match workbench.import_tile_png(&bytes) {
                Ok(()) => {
                    self.asset_dirty = true;
                    "TILE ATLAS IMPORTED".to_owned()
                }
                Err(error) => error,
            },
            None => "OPEN A TILE SET FIRST".to_owned(),
        };
    }

    fn export_tile_png(&mut self) {
        let Some(workbench) = &self.asset_workbench else {
            return;
        };
        let png = match workbench.export_tile_png(&sprite::palette()) {
            Ok(png) => png,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(export_names::asset(&self.asset_name, "png"))
            .add_filter("PNG atlas", &["png"])
            .save_file()
        {
            self.status = match fs::write(path, png) {
                Ok(()) => "TILE ATLAS EXPORTED".to_owned(),
                Err(error) => error.to_string(),
            };
        }
    }

    fn open_asset(&mut self, name: &str) {
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty) {
            if let Err(error) = self.save_sprite() {
                self.status = error;
                return;
            }
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Some(asset) = project.manifest.assets.get(name) else {
            return;
        };
        if matches!(
            asset.kind,
            modl_core::AssetKind::Sprite | modl_core::AssetKind::Animation
        ) {
            self.load_sprite(name);
            return;
        }
        let Some(bytes) = project.files.get(&asset.path) else {
            self.status = format!("ASSET FILE {} IS MISSING", asset.path);
            return;
        };
        if self.asset_name == name && self.asset_path == asset.path {
            self.view = View::Assets;
            return;
        }
        let asset_path = asset.path.clone();
        let kind = asset.kind;
        match String::from_utf8(bytes.clone()) {
            Ok(source) => {
                let editor = self
                    .asset_sessions
                    .take_structured(project, name, &asset_path, Some(kind))
                    .or_else(|| AssetWorkbench::new(kind, &source).ok());
                self.cache_asset_editors();
                self.asset_name = name.to_owned();
                self.asset_path = asset_path;
                self.asset_text = source;
                self.asset_dirty = false;
                self.asset_workbench = editor;
                self.view = View::Assets;
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn open_display(&mut self) {
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty)
            && let Err(error) = self.save_sprite()
        {
            self.status = error;
            return;
        }
        let Some(project) = self.project.as_mut() else {
            return;
        };
        let path = project
            .manifest
            .display
            .clone()
            .unwrap_or_else(|| "assets/display.m01p".to_owned());
        if project.manifest.display.is_none() {
            let mut manifest: toml::Value = match toml::from_str(&project.manifest_source) {
                Ok(value) => value,
                Err(error) => {
                    self.status = error.to_string();
                    return;
                }
            };
            let Some(table) = manifest.as_table_mut() else {
                self.status = "MANIFEST MUST BE A TABLE".to_owned();
                return;
            };
            table.insert("display".to_owned(), toml::Value::String(path.clone()));
            project.manifest_source = match toml::to_string_pretty(&manifest) {
                Ok(value) => value,
                Err(error) => {
                    self.status = error.to_string();
                    return;
                }
            };
            project.manifest.display = Some(path.clone());
            project.dirty = true;
            self.manifest_text = project.manifest_source.clone();
        }
        let source = project.files.entry(path.clone()).or_insert_with(|| {
            serde_json::to_vec(&json!({
                "revision":2,"kind":"display",
                "remap":(0..32).collect::<Vec<_>>(),"raster":[]
            }))
            .unwrap_or_default()
        });
        let source = match String::from_utf8(source.clone()) {
            Ok(source) => source,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        if self.asset_name == "DISPLAY"
            && self.asset_path == path
            && self
                .asset_workbench
                .as_ref()
                .is_some_and(AssetWorkbench::is_display)
        {
            self.view = View::Assets;
            return;
        }
        let editor = self
            .asset_sessions
            .take_structured(project, "DISPLAY", &path, None)
            .or_else(|| AssetWorkbench::new_display(&source).ok());
        self.cache_asset_editors();
        self.asset_workbench = editor;
        self.asset_text = source;
        self.asset_name = "DISPLAY".to_owned();
        self.asset_path = path;
        self.asset_dirty = false;
        self.view = View::Assets;
    }

    fn create_asset(&mut self) {
        let name = self.new_asset_name.trim().to_owned();
        if name.is_empty()
            || name.len() > 64
            || !name.as_bytes()[0].is_ascii_alphabetic()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.status = "ASSET NAME MUST BE A MODL IDENTIFIER".to_owned();
            return;
        }
        let Some(project) = self.project.as_mut() else {
            return;
        };
        if project.manifest.assets.contains_key(&name) {
            self.status = "ASSET NAME ALREADY EXISTS".to_owned();
            return;
        }
        let (extension, value) = match self.new_asset_kind {
            modl_core::AssetKind::TileSet => (
                "m01g",
                json!({"revision":2,"kind":"tile_set","tiles":vec![vec![0;64]],"flags":[0]}),
            ),
            modl_core::AssetKind::Map => {
                let tile_set = project
                    .manifest
                    .assets
                    .iter()
                    .find(|(_, asset)| asset.kind == modl_core::AssetKind::TileSet)
                    .map_or_else(|| "tiles".to_owned(), |(name, _)| name.clone());
                if !project.manifest.assets.contains_key(&tile_set) {
                    self.status = "CREATE A TILE SET BEFORE A MAP".to_owned();
                    return;
                }
                (
                    "m01m",
                    json!({"revision":1,"kind":"map","layers":[{"width":30,"height":18,"cells":vec![0;540],"tileSet":tile_set}]}),
                )
            }
            modl_core::AssetKind::Font => (
                "m01f",
                json!({"revision":1,"kind":"font","glyphWidth":5,"glyphHeight":7,"baseline":6,"advanceX":6,"advanceY":8,"missingGlyph":63,"glyphs":[{"code":63,"pixels":vec![0;35]}]}),
            ),
            modl_core::AssetKind::Sound => (
                "m01s",
                json!({"revision":1,"kind":"sound","waveform":"pulse","note":60,"durationFrames":18,"volume":0.6,"pan":0,"duty":0.5,"envelope":{"attackFrames":1,"decayFrames":3,"sustainLevel":0.65,"releaseFrames":4},"pitch":{"slideSemitonesPerFrame":0,"vibratoDepthSemitones":0,"vibratoPeriodFrames":0}}),
            ),
            modl_core::AssetKind::Music => (
                "m01t",
                json!({"revision":1,"kind":"music","framesPerRow":6,"order":["00"],"patterns":{"00":{"rows":vec![vec![Value::Null;8];16]}},"loop":true}),
            ),
            modl_core::AssetKind::Sprite | modl_core::AssetKind::Animation => {
                self.status = "CREATE SPRITES IN THE SPRITE EDITOR".to_owned();
                return;
            }
        };
        let path = format!("assets/{name}.{extension}");
        let mut manifest: toml::Value = match toml::from_str(&project.manifest_source) {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let Some(assets) = manifest
            .get_mut("assets")
            .and_then(toml::Value::as_table_mut)
        else {
            self.status = "MANIFEST HAS NO ASSETS TABLE".to_owned();
            return;
        };
        let mut declaration = toml::map::Map::new();
        declaration.insert(
            "kind".to_owned(),
            toml::Value::String(
                serde_json::to_value(self.new_asset_kind)
                    .unwrap_or_default()
                    .as_str()
                    .unwrap_or("")
                    .to_owned(),
            ),
        );
        declaration.insert("path".to_owned(), toml::Value::String(path.clone()));
        assets.insert(name.clone(), toml::Value::Table(declaration));
        let source = match toml::to_string_pretty(&manifest) {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let parsed = match modl_core::parse_project_manifest(&source) {
            Ok(value) => value,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        project.manifest_source = source;
        project.manifest = parsed;
        project
            .files
            .insert(path, serde_json::to_vec(&value).unwrap_or_default());
        project.dirty = true;
        self.manifest_text = project.manifest_source.clone();
        self.show_new_asset = false;
        self.open_asset(&name);
        self.status = format!("CREATED {name}");
    }

    fn load_sprite(&mut self, name: &str) {
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty) {
            if let Err(error) = self.save_sprite() {
                self.status = error;
                return;
            }
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        if self
            .sprite
            .as_ref()
            .is_some_and(|editor| editor.name == name)
        {
            self.view = View::Sprites;
            return;
        }
        let Some(asset) = project.manifest.assets.get(name) else {
            return;
        };
        let sidecar = format!("editor/sprites/{name}.m01paint");
        let palette = sprite::palette();
        let editor = if let Some(editor) = self.asset_sessions.take_sprite(project, name) {
            Ok(editor)
        } else if let Some(source) = project.files.get(&sidecar) {
            serde_json::from_slice::<SpriteDocument>(source)
                .map_err(|error| error.to_string())
                .and_then(|document| {
                    SpriteEditor::new(name.to_owned(), asset.path.clone(), document)
                })
        } else if let Some(source) = project.files.get(&asset.path) {
            SpriteEditor::from_indexed(name.to_owned(), asset.path.clone(), source, &palette)
        } else {
            Err(format!("sprite asset '{}' is missing", asset.path))
        };
        match editor {
            Ok(editor) => {
                self.cache_asset_editors();
                self.sprite = Some(editor);
                self.asset_name.clear();
                self.sprite_texture = None;
                self.view = View::Sprites;
                self.status = format!("EDITING {name}");
            }
            Err(error) => self.status = error,
        }
    }

    fn create_sprite(&mut self) {
        if let Err(error) = self.sync_asset() {
            self.status = error;
            return;
        }
        if self.sprite.as_ref().is_some_and(|sprite| sprite.dirty) {
            if let Err(error) = self.save_sprite() {
                self.status = error;
                return;
            }
        }
        let name = self.sprite_name.trim().to_owned();
        if name.is_empty()
            || name.len() > 64
            || !name.as_bytes()[0].is_ascii_alphabetic()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.status = "SPRITE NAME MUST BE A MODL IDENTIFIER".to_owned();
            return;
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        if project.manifest.assets.contains_key(&name) {
            self.status = "ASSET NAME ALREADY EXISTS".to_owned();
            return;
        }
        let path = format!("assets/{name}.m01g");
        let editor = SpriteEditor::new(
            name.clone(),
            path,
            SpriteDocument::new(DEFAULT_AXIS, DEFAULT_AXIS),
        );
        match editor {
            Ok(editor) => {
                self.cache_asset_editors();
                self.sprite = Some(editor);
                self.sprite_texture = None;
                if let Err(error) = self.save_sprite() {
                    self.status = error;
                } else {
                    self.status = format!("CREATED {name} / 64x64");
                }
            }
            Err(error) => self.status = error,
        }
    }

    fn save_sprite(&mut self) -> Result<(), String> {
        let editor = self.sprite.as_mut().ok_or("no sprite is open")?;
        editor.document.validate()?;
        let name = editor.name.clone();
        let asset_path = editor.asset_path.clone();
        let sidecar = format!("editor/sprites/{name}.m01paint");
        let frames = editor.document.indexed_frames();
        let asset = json!({
            "revision": 2,
            "kind": "sprite",
            "width": editor.document.width,
            "height": editor.document.height,
            "frames": frames,
        });
        let mut asset_bytes = serde_json::to_vec(&asset).map_err(|error| error.to_string())?;
        asset_bytes.push(b'\n');
        let mut source = serde_json::to_vec(&editor.document).map_err(|error| error.to_string())?;
        source.push(b'\n');
        let project = self.project.as_mut().ok_or("no project is open")?;
        let mut manifest: toml::Value = toml::from_str(&project.manifest_source)
            .map_err(|error| format!("invalid manifest: {error}"))?;
        manifest
            .as_table_mut()
            .ok_or("manifest must be a table")?
            .insert("hardware_revision".to_owned(), toml::Value::Integer(2));
        let assets = manifest
            .get_mut("assets")
            .and_then(toml::Value::as_table_mut)
            .ok_or("manifest has no assets table")?;
        let mut declaration = toml::map::Map::new();
        declaration.insert(
            "kind".to_owned(),
            toml::Value::String(
                if editor.document.frames.len() > 1 {
                    "animation"
                } else {
                    "sprite"
                }
                .to_owned(),
            ),
        );
        declaration.insert("path".to_owned(), toml::Value::String(asset_path.clone()));
        assets.insert(name.clone(), toml::Value::Table(declaration));
        let manifest_source =
            toml::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
        let parsed = modl_core::parse_project_manifest(&manifest_source)
            .map_err(|error| error.to_string())?;
        project.files.insert(asset_path, asset_bytes);
        project.files.insert(sidecar, source);
        project.manifest_source = manifest_source;
        project.manifest = parsed;
        project.dirty = true;
        editor.dirty = false;
        self.manifest_text = project.manifest_source.clone();
        Ok(())
    }

    fn sprite_ui(&mut self, ui: &mut egui::Ui) {
        let names: Vec<String> = self.project.as_ref().map_or_else(Vec::new, |project| {
            project
                .manifest
                .assets
                .iter()
                .filter(|(_, asset)| {
                    matches!(
                        asset.kind,
                        modl_core::AssetKind::Sprite | modl_core::AssetKind::Animation
                    )
                })
                .map(|(name, _)| name.clone())
                .collect()
        });
        ui.horizontal(|ui| {
            ui.heading("SPRITE STUDIO");
            ui.separator();
            for name in names {
                let selected = self
                    .sprite
                    .as_ref()
                    .is_some_and(|sprite| sprite.name == name);
                if ui.selectable_label(selected, &name).clicked() {
                    self.load_sprite(&name);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("New sprite");
            ui.add_sized(
                [150.0, 30.0],
                egui::TextEdit::singleline(&mut self.sprite_name),
            );
            if ui.button("CREATE 64x64").clicked() {
                self.create_sprite();
            }
            if self.sprite.is_some() && ui.button("SAVE SPRITE").clicked() {
                self.status = match self.save_sprite() {
                    Ok(()) => "SPRITE SAVED TO PROJECT".to_owned(),
                    Err(error) => error,
                };
            }
        });
        ui.separator();
        let Some(editor) = self.sprite.as_mut() else {
            ui.add_space(24.0);
            egui::Frame::new()
                .fill(ui_theme::CASE_LIGHT)
                .stroke(egui::Stroke::new(1.0, ui_theme::EDGE))
                .inner_margin(egui::Margin::same(18))
                .show(ui, |ui| {
                    ui_theme::legend(ui, "SPRITE BANK / EMPTY SLOT");
                    ui.add_space(7.0);
                    ui.heading("READY FOR NEW ART");
                    ui.label("Create a 64x64 sprite above, or select one from the asset bank.");
                    ui.label(
                        RichText::new("FRAMES  /  LAYERS  /  256 COLORS").color(ui_theme::CYAN),
                    );
                });
            return;
        };
        let controls_height = ui.available_height();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(272.0);
                ui.set_min_height(controls_height);
                egui::ScrollArea::vertical()
                    .id_salt("sprite-controls")
                    .max_height(controls_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(260.0);
                        ui.label(
                            RichText::new(format!(
                                "{}  /  FRAME {}/{}",
                                editor.name.to_uppercase(),
                                editor.frame + 1,
                                editor.document.frames.len()
                            ))
                            .strong(),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("< PREV").clicked() {
                                editor.frame = (editor.frame + editor.document.frames.len() - 1)
                                    % editor.document.frames.len();
                                editor.layer = editor
                                    .layer
                                    .min(editor.document.frames[editor.frame].layers.len() - 1);
                            }
                            if ui.button("NEXT >").clicked() {
                                editor.frame = (editor.frame + 1) % editor.document.frames.len();
                                editor.layer = editor
                                    .layer
                                    .min(editor.document.frames[editor.frame].layers.len() - 1);
                            }
                            if ui.button("+ FRAME").clicked() {
                                editor.add_frame();
                            }
                            if ui.button("- FRAME").clicked() {
                                editor.delete_frame();
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.button("UNDO").clicked() {
                                editor.undo();
                            }
                            if ui.button("REDO").clicked() {
                                editor.redo();
                            }
                        });
                        ui.separator();
                        ui.label("TOOLS");
                        for (tool, title) in [
                            (Tool::Pencil, "PENCIL"),
                            (Tool::Eraser, "ERASER"),
                            (Tool::Eyedropper, "PICK"),
                            (Tool::Fill, "FILL"),
                            (Tool::Select, "SELECT"),
                        ] {
                            if ui.selectable_label(editor.tool == tool, title).clicked() {
                                editor.tool = tool;
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.label("BRUSH");
                            for brush in [1, 2, 4, 8] {
                                if ui
                                    .selectable_label(editor.brush == brush, brush.to_string())
                                    .clicked()
                                {
                                    editor.brush = brush;
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut editor.onion, "ONION");
                            ui.checkbox(&mut editor.show_grid, "GRID");
                        });
                        ui.horizontal(|ui| {
                            let mut width = editor.document.width;
                            let mut height = editor.document.height;
                            ui.label("SIZE");
                            let changed_w = ui
                                .add(egui::DragValue::new(&mut width).range(1..=128).speed(1))
                                .changed();
                            ui.label("x");
                            let changed_h = ui
                                .add(egui::DragValue::new(&mut height).range(1..=128).speed(1))
                                .changed();
                            if changed_w || changed_h {
                                editor.resize(width, height);
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.button("PNG IN").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("PNG", &["png"])
                                    .pick_file()
                                {
                                    match modl_core::asset_import::read_png_source(&path)
                                        .map_err(|error| error.to_string())
                                        .and_then(|bytes| editor.import_png(&bytes))
                                    {
                                        Ok(()) => {
                                            self.status =
                                                "PNG IMPORTED INTO CURRENT LAYER".to_owned()
                                        }
                                        Err(error) => self.status = error,
                                    }
                                }
                            }
                            if ui.button("PNG OUT").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .set_file_name(export_names::asset(&editor.name, "png"))
                                    .save_file()
                                {
                                    self.status = match editor.export_png().and_then(|bytes| {
                                        fs::write(path, bytes).map_err(|error| error.to_string())
                                    }) {
                                        Ok(()) => "PNG EXPORTED".to_owned(),
                                        Err(error) => error,
                                    };
                                }
                            }
                        });
                        ui.separator();
                        ui.label("LAYER");
                        ui.horizontal(|ui| {
                            if ui.button("+").clicked() {
                                editor.add_layer();
                            }
                            if ui.button("-").clicked() {
                                editor.delete_layer();
                            }
                            if ui.button("UP").clicked() {
                                editor.move_layer(1);
                            }
                            if ui.button("DN").clicked() {
                                editor.move_layer(-1);
                            }
                        });
                        let count = editor.document.frames[editor.frame].layers.len();
                        for index in (0..count).rev() {
                            let current = &editor.document.frames[editor.frame].layers[index];
                            let mut name = current.name.clone();
                            let mut visible = current.visible;
                            let mut locked = current.locked;
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(
                                        editor.layer == index,
                                        format!("{}", index + 1),
                                    )
                                    .clicked()
                                {
                                    editor.layer = index;
                                }
                                ui.add_sized([112.0, 22.0], egui::TextEdit::singleline(&mut name));
                                ui.checkbox(&mut visible, "V").on_hover_text("Visible");
                                ui.checkbox(&mut locked, "L").on_hover_text("Lock painting");
                            });
                            if (!name.is_empty() && name.len() <= 64 && name != current.name)
                                || visible != current.visible
                                || locked != current.locked
                            {
                                editor.remember();
                                let layer = &mut editor.document.frames[editor.frame].layers[index];
                                if !name.is_empty() && name.len() <= 64 {
                                    layer.name = name;
                                }
                                layer.visible = visible;
                                layer.locked = locked;
                                editor.dirty = true;
                            }
                        }
                        ui.separator();
                        ui.label("COLOR / RGBA");
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(&mut self.color_hex);
                            if ui.button("SET").clicked() {
                                if let Ok(value) =
                                    u32::from_str_radix(self.color_hex.trim_start_matches('#'), 16)
                                {
                                    editor.color = value.to_be_bytes();
                                } else {
                                    self.status = "USE 8 HEX DIGITS: RRGGBBAA".to_owned();
                                }
                            }
                        });
                        egui::ScrollArea::vertical()
                            .max_height(180.0)
                            .show(ui, |ui| {
                                let palette = sprite::palette();
                                egui::Grid::new("sprite-palette").spacing([3.0, 3.0]).show(
                                    ui,
                                    |ui| {
                                        for (index, rgb) in palette.iter().enumerate() {
                                            let fill = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                                            if ui
                                                .add_sized(
                                                    [36.0, 30.0],
                                                    egui::Button::new("").fill(fill),
                                                )
                                                .on_hover_text(if index == 0 {
                                                    "TRANSPARENT / ERASER".to_owned()
                                                } else {
                                                    format!("COLOR {index}")
                                                })
                                                .clicked()
                                            {
                                                if index == 0 {
                                                    editor.tool = Tool::Eraser;
                                                } else {
                                                    editor.tool = Tool::Pencil;
                                                    editor.color = [rgb[0], rgb[1], rgb[2], 255];
                                                    self.color_hex = format!(
                                                        "{:02X}{:02X}{:02X}FF",
                                                        rgb[0], rgb[1], rgb[2]
                                                    );
                                                }
                                            }
                                            if index % 6 == 5 {
                                                ui.end_row();
                                            }
                                        }
                                    },
                                );
                            });
                        if editor.selection.is_some() {
                            ui.horizontal(|ui| {
                                if ui.button("RECOLOR").clicked() {
                                    editor.clear_or_recolor_selection(false);
                                }
                                if ui.button("ERASE").clicked() {
                                    editor.clear_or_recolor_selection(true);
                                }
                                if ui.button("CLEAR SELECT").clicked() {
                                    editor.selection = None;
                                }
                            });
                        }
                    });
            });
            ui.separator();
            paint_sprite_canvas(
                ui,
                editor,
                &mut self.sprite_texture,
                &mut self.sprite_painting,
                &mut self.last_sprite_point,
                &mut self.selection_anchor,
            );
        });
    }
}

impl eframe::App for StudioApp {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = root.ctx().clone();
        self.sync_viewport_mode(&context);
        self.poll_audio_output();
        self.capture_keyboard(&context);
        self.handle_system_menu_toggle(&context);
        self.process_events(&context);
        self.process_preview(&context);
        if (self.player_mode || self.view != View::Assets) && self.live_audio.active() {
            self.live_audio.stop();
        }
        self.live_audio.poll(&context, self.audio.as_ref());
        self.playground.poll(&context);
        if !self.player_mode {
            self.touch_controller.clear();
        }
        if self.player_mode {
            self.player_ui(root);
            self.sync_viewport_mode(&context);
            return;
        }
        egui::Panel::top("toolbar")
            .frame(ui_theme::case_frame())
            .show(root, |ui| self.toolbar(ui));
        egui::Panel::bottom("status")
            .frame(ui_theme::case_frame())
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui_theme::led(ui, true, "PWR");
                    ui_theme::led(ui, self.running && self.loaded, "EXEC");
                    self.audio_indicator(ui);
                    ui.separator();
                    let status_color =
                        if self.status.contains("ERROR") || self.status.contains("FAILED") {
                            ui_theme::CORAL
                        } else {
                            ui_theme::CREAM
                        };
                    ui.label(RichText::new(&self.status).color(status_color));
                    if let Some(project) = &self.project {
                        ui.separator();
                        ui.label(
                            RichText::new(format!(
                                "{}{}",
                                project.manifest.id,
                                if project.dirty { " *" } else { "" }
                            ))
                            .color(ui_theme::AMBER),
                        );
                    }
                    if self.running {
                        ui.separator();
                        ui.label(
                            RichText::new(format!(
                                "FRAME {:06}  WORK {:06}",
                                self.frame, self.work
                            ))
                            .color(ui_theme::CYAN),
                        );
                    }
                });
            });
        if self.show_new {
            let mut open = self.show_new;
            egui::Window::new("NEW CARTRIDGE")
                .open(&mut open)
                .show(&context, |ui| {
                    ui.label("Project directory");
                    ui.text_edit_singleline(&mut self.new_path);
                    if ui.button("CHOOSE FOLDER").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.new_path = path.to_string_lossy().to_string();
                        }
                    }
                    ui.label("Cartridge ID");
                    ui.text_edit_singleline(&mut self.new_id);
                    if ui.button("CREATE").clicked() {
                        self.create();
                    }
                });
            self.show_new = open && self.show_new;
        }
        if self.show_new_asset {
            let mut open = true;
            egui::Window::new("NEW ASSET")
                .open(&mut open)
                .show(&context, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.new_asset_name);
                    egui::ComboBox::from_id_salt("new-asset-kind")
                        .selected_text(format!("{:?}", self.new_asset_kind))
                        .show_ui(ui, |ui| {
                            for kind in [
                                modl_core::AssetKind::TileSet,
                                modl_core::AssetKind::Map,
                                modl_core::AssetKind::Font,
                                modl_core::AssetKind::Sound,
                                modl_core::AssetKind::Music,
                            ] {
                                ui.selectable_value(
                                    &mut self.new_asset_kind,
                                    kind,
                                    format!("{kind:?}"),
                                );
                            }
                        });
                    if ui.button("CREATE").clicked() {
                        self.create_asset();
                    }
                });
            self.show_new_asset &= open;
        }
        self.help_windows(&context);
        self.members_window(&context);
        if self.show_playground {
            self.playground
                .ui(&context, &mut self.show_playground, self.project.as_ref());
        }
        if self.show_library || self.project.is_none() {
            let mut action = None;
            let navigation = self.gamepad_controller.take_navigation();
            egui::CentralPanel::default()
                .frame(ui_theme::screen_frame())
                .show(root, |ui| {
                    ui.add_enabled_ui(self.pending_library_open.is_none(), |ui| {
                        action = self.library.ui(ui, navigation, self.project.is_some());
                    });
                });
            if let Some(action) = action {
                self.library_action(action);
            }
            self.library_confirmation(&context);
            self.cursors.paint(&context);
            return;
        }
        egui::Panel::left("project")
            .frame(ui_theme::case_frame())
            .default_size(218.0)
            .show(root, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("project-bank")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui_theme::legend(ui, "01 / WORKSPACE");
                        ui.heading("PROJECT");
                        self.member_picker(ui);
                        ui.separator();
                        for (view, title) in [
                            (View::Code, "01  CODE"),
                            (View::Sprites, "02  SPRITES"),
                            (View::Assets, "03  ASSETS"),
                            (View::Player, "04  PLAYER"),
                            (View::Manifest, "05  MANIFEST"),
                        ] {
                            let selected = self.view == view;
                            if ui
                                .add_sized(
                                    [ui.available_width(), 40.0],
                                    egui::Button::new(title).selected(selected),
                                )
                                .clicked()
                            {
                                self.view = view;
                            }
                        }
                        ui.add_space(10.0);
                        ui.separator();
                        if let Some(project) = self.project.as_mut() {
                            ui_theme::legend(ui, "SOURCE / MODL/1");
                            for path in project.source_paths() {
                                if ui
                                    .add_sized(
                                        [ui.available_width(), 40.0],
                                        egui::Button::new(&path)
                                            .selected(project.selected_file == path),
                                    )
                                    .clicked()
                                {
                                    match project.select_source(&path) {
                                        Ok(()) => self.view = View::Code,
                                        Err(error) => self.status = error,
                                    }
                                }
                            }
                        }
                        ui.separator();
                        ui_theme::legend(ui, "ASSET BANK");
                        if ui.button("+ NEW ASSET").clicked() {
                            self.show_new_asset = true;
                        }
                        if ui
                            .add_sized(
                                [ui.available_width(), 40.0],
                                egui::Button::new("DISPLAY / PALETTE")
                                    .selected(self.asset_name == "DISPLAY"),
                            )
                            .clicked()
                        {
                            self.open_display();
                        }
                        let names: Vec<String> =
                            self.project.as_ref().map_or_else(Vec::new, |project| {
                                project.manifest.assets.keys().cloned().collect()
                            });
                        for name in names {
                            let selected = self.asset_name == name
                                || self
                                    .sprite
                                    .as_ref()
                                    .is_some_and(|sprite| sprite.name == name);
                            if ui
                                .add_sized(
                                    [ui.available_width(), 40.0],
                                    egui::Button::new(&name).selected(selected),
                                )
                                .clicked()
                            {
                                self.open_asset(&name);
                            }
                        }
                    });
            });
        egui::CentralPanel::default()
            .frame(ui_theme::screen_frame())
            .show(root, |ui| match self.view {
                View::Code => {
                    if let Some(project) = self.project.as_mut() {
                        self.source_editor
                            .show(ui, project, &mut self.modl_highlighter);
                    }
                }
                View::Sprites => { self.asset_meter_ui(ui); self.asset_delete_button(ui); self.sprite_ui(ui); }
                View::Assets => {
                    self.asset_meter_ui(ui);
                    self.asset_delete_button(ui);
                    egui::ScrollArea::vertical().id_salt("asset-workspace").show(ui, |ui| {
                    if self.asset_name.is_empty() {
                        ui.heading("ASSETS");
                        ui.label("Choose a map, font, sound or music asset from the project list.");
                    } else {
                        ui.heading(&self.asset_name);
                        ui.label(&self.asset_path);
                        let tile_sets = self.project.as_ref().map_or_else(Vec::new, |project| {
                            project
                                .manifest
                                .assets
                                .iter()
                                .filter_map(|(name, asset)| {
                                    (asset.kind == modl_core::AssetKind::TileSet)
                                        .then(|| project.files.get(&asset.path))
                                        .flatten()
                                        .and_then(|bytes| serde_json::from_slice(bytes).ok())
                                        .map(|value| (name.clone(), value))
                                })
                                .collect::<Vec<_>>()
                        });
                        ui.horizontal(|ui| {
                            if ui.button("APPLY JSON").clicked() {
                                self.status = match self.sync_asset() {
                                    Ok(()) => "ASSET APPLIED".to_owned(),
                                    Err(error) => error,
                                };
                            }
                            if ui.button("IMPORT ASSET").clicked() {
                                self.import_asset_file();
                            }
                            if ui.button("EXPORT ASSET").clicked() {
                                self.export_asset_file();
                            }
                        });
                        if self
                            .asset_workbench
                            .as_ref()
                            .is_some_and(|editor| editor.kind == modl_core::AssetKind::TileSet)
                        {
                            ui.horizontal(|ui| {
                                if ui.button("IMPORT TILE PNG").clicked() {
                                    self.import_tile_png();
                                }
                                if ui.button("EXPORT TILE PNG").clicked() {
                                    self.export_tile_png();
                                }
                            });
                        }
                        if self
                            .project
                            .as_ref()
                            .and_then(|project| project.manifest.assets.get(&self.asset_name))
                            .is_some_and(|asset| {
                                matches!(
                                    asset.kind,
                                    modl_core::AssetKind::Sound | modl_core::AssetKind::Music
                                )
                            })
                        {
                            ui.horizontal_wrapped(|ui| {
                                let music = self
                                    .project
                                    .as_ref()
                                    .and_then(|project| {
                                        project.manifest.assets.get(&self.asset_name)
                                    })
                                    .is_some_and(|asset| asset.kind == modl_core::AssetKind::Music);
                                if music && ui.button("LIVE SONG").clicked() {
                                    self.start_live_audio(AudioPreviewMode::Song(0));
                                }
                                if music && ui.button("PATTERN FROM CURSOR").clicked() {
                                    self.start_live_audio(AudioPreviewMode::Pattern);
                                }
                                if ui
                                    .button(if music {
                                        "AUDITION CELL"
                                    } else {
                                        "PREVIEW PATCH"
                                    })
                                    .clicked()
                                {
                                    self.start_live_audio(AudioPreviewMode::Note);
                                }
                                if ui
                                    .add_enabled(
                                        self.live_audio.active(),
                                        egui::Button::new("STOP AUDIO"),
                                    )
                                    .clicked()
                                {
                                    self.live_audio.stop();
                                }
                                if ui
                                    .add_enabled(
                                        !self.preview_pending && !self.preview_cancelling,
                                        egui::Button::new("EXPORT WAV"),
                                    )
                                    .on_hover_text("Offline stereo PCM16, at most 8 MiB (about 43.7 seconds, including a sound's release). Longer exports are rejected, never truncated; shorten the patch or music order.")
                                    .clicked()
                                {
                                    self.render_asset_wav();
                                }
                            });
                            self.live_audio.telemetry_ui(ui);
                        }
                        ui.collapsing("EDIT RAW JSON", |ui| {
                            if let Some(workbench) = &self.asset_workbench {
                                if ui.button("SYNC SOURCE").clicked() {
                                    self.asset_text = workbench.source().unwrap_or_default();
                                }
                            }
                            if ui
                                .add(
                                    egui::TextEdit::multiline(&mut self.asset_text)
                                        .id(self.project.as_ref().map_or_else(|| egui::Id::unique("no-asset-json"), |project| self.asset_sessions.text_id(project, &self.asset_name, &self.asset_path)))
                                        .desired_width(f32::INFINITY)
                                        .font(egui::TextStyle::Monospace)
                                        .code_editor()
                                        .desired_rows(12),
                                )
                                .changed()
                            {
                                self.asset_dirty = true;
                                self.asset_workbench = None;
                            }
                        });
                        let sounds = self.project.as_ref().map_or_else(Vec::new, |project| {
                            project.manifest.assets.iter().filter(|(_, asset)| asset.kind == modl_core::AssetKind::Sound).map(|(name, _)| name.clone()).collect::<Vec<_>>()
                        });
                        let position = self.live_audio.playhead(&self.asset_name);
                        let mut tracker_actions = Vec::new();
                        if let Some(workbench) = self.asset_workbench.as_mut() {
                            ui.separator();
                            if workbench.ui(ui, &sprite::palette(), &tile_sets, &sounds, position.as_ref()) {
                                self.asset_dirty = true;
                            }
                            tracker_actions = workbench.take_tracker_actions();
                        }
                        for action in tracker_actions {
                            match action {
                                tracker::Action::Pattern => self.start_live_audio(AudioPreviewMode::Pattern),
                                tracker::Action::Song(order) => self.start_live_audio(AudioPreviewMode::Song(order)),
                                tracker::Action::Stop => self.live_audio.stop(),
                                tracker::Action::Audition { sound, note, volume } => {
                                    let prepared = self.project.as_ref().ok_or_else(|| "no project is open".to_owned()).and_then(|project| {
                                        let document = self.selected_audio_document()?;
                                        live_audio::project_bank(project, &self.asset_name, &document, None)
                                    });
                                    match prepared {
                                        Ok(bank) => self.live_audio.audition(bank, &sound, f64::from(note), volume),
                                        Err(error) => self.status = format!("AUDIO PREVIEW ERROR: {error}"),
                                    }
                                }
                                tracker::Action::Undo | tracker::Action::Redo => unreachable!("history is handled by the asset workbench"),
                            }
                        }
                    }
                    });
                }
                View::Player => {
                    if ui.button("OPEN GAME VIEW").clicked() {
                        self.player_mode = true;
                    }
                    ui_theme::legend(ui, "DISPLAY / 240 x 144");
                    ui.heading("CARTRIDGE PLAYER");
                    ui.label(
                        RichText::new("ARROWS + Z X A S  /  Q W  /  ENTER").color(ui_theme::MUTED),
                    );
                    ui.add_space(12.0);
                    if let Some(texture) = &self.texture {
                        let mut available = ui.available_size();
                        if self.debugging {
                            available.y = (available.y - 210.0).max(144.0);
                        }
                        let scale = ((available.x - 12.0) / SCREEN_WIDTH as f32)
                            .min((available.y - 12.0) / SCREEN_HEIGHT as f32)
                            .floor()
                            .max(1.0);
                        let size =
                            egui::vec2(SCREEN_WIDTH as f32 * scale, SCREEN_HEIGHT as f32 * scale);
                        ui.horizontal(|ui| {
                            ui.add_space((ui.available_width() - size.x - 12.0).max(0.0) * 0.5);
                            let (bezel, _) = ui.allocate_exact_size(
                                size + egui::vec2(12.0, 12.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(bezel, 1.0, ui_theme::CASE_LIGHT);
                            ui.painter().rect_stroke(
                                bezel,
                                1.0,
                                egui::Stroke::new(2.0, ui_theme::EDGE),
                                egui::StrokeKind::Inside,
                            );
                            let image_rect =
                                egui::Rect::from_min_size(bezel.min + egui::vec2(6.0, 6.0), size);
                            let response = ui
                                .put(image_rect, egui::Image::new((texture.id(), size)))
                                .on_hover_cursor(egui::CursorIcon::Crosshair);
                            self.screen_rect = Some(response.rect);
                        });
                    } else {
                        ui.label(
                            RichText::new(if self.running {
                                "BOOTING CARTRIDGE..."
                            } else {
                                "NO CARTRIDGE RUNNING  /  PRESS RUN"
                            })
                            .color(ui_theme::CYAN),
                        );
                    }
                    if self.debugging {
                        self.debugger_ui(ui);
                    }
                }
                View::Manifest => {
                    ui.heading("CARTRIDGE METADATA");
                    if ui.button("APPLY MANIFEST").clicked() {
                        self.apply_manifest();
                    }
                    ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(&mut self.manifest_text)
                            .font(egui::TextStyle::Monospace)
                            .code_editor(),
                    );
                }
            });
        self.system_menu_ui(&context);
        self.asset_removal_ui(&context);
        self.advance_runtime(&context);
        self.cursors.paint(&context);
        self.sync_viewport_mode(&context);
        if self.running {
            context.request_repaint_after(Duration::from_millis(4));
        }
    }
}

fn native_host_path() -> PathBuf {
    if let Ok(path) = std::env::var("MOD01_NATIVE_HOST") {
        return PathBuf::from(path);
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            let bundled = directory.join("native-host.mjs");
            if bundled.exists() {
                return bundled;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/runtime/standalone/native-host.mjs")
}

fn bundled_samples() -> Vec<PathBuf> {
    let override_path = std::env::var_os("MOD01_SAMPLES").map(PathBuf::from);
    let packaged = std::env::current_exe().ok().and_then(|executable| {
        let directory = executable.parent()?;
        let macos = directory.parent()?.join("Resources/cartridges");
        Some(if macos.is_dir() {
            macos
        } else {
            directory.join("cartridges")
        })
    });
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../cartridges");
    let root = override_path
        .or(packaged)
        .filter(|path| path.is_dir())
        .unwrap_or(development);
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("cart.m01w").is_file())
        .collect();
    paths.sort();
    paths
}

fn paint_sprite_canvas(
    ui: &mut egui::Ui,
    editor: &mut SpriteEditor,
    texture: &mut Option<TextureHandle>,
    painting: &mut bool,
    last_point: &mut Option<(i32, i32)>,
    selection_anchor: &mut Option<(usize, usize)>,
) {
    let width = editor.document.width;
    let height = editor.document.height;
    let pixels = editor.document.composite(editor.frame);
    let previous = if editor.onion && editor.document.frames.len() > 1 {
        Some(editor.document.composite(
            (editor.frame + editor.document.frames.len() - 1) % editor.document.frames.len(),
        ))
    } else {
        None
    };
    let mut display = Vec::with_capacity(width * height * 4);
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        let checker = if (index % width / 8 + index / width / 8) % 2 == 0 {
            45.0
        } else {
            59.0
        };
        let mut background = [checker, checker, checker];
        if let Some(previous) = &previous {
            let before = &previous[index * 4..index * 4 + 4];
            let opacity = f32::from(before[3]) / 255.0 * 0.22;
            for channel in 0..3 {
                background[channel] =
                    background[channel] * (1.0 - opacity) + f32::from(before[channel]) * opacity;
            }
        }
        let alpha = f32::from(pixel[3]) / 255.0;
        for channel in 0..3 {
            display.push(
                (f32::from(pixel[channel]) * alpha + background[channel] * (1.0 - alpha)).round()
                    as u8,
            );
        }
        display.push(255);
    }
    let image = ColorImage::from_rgba_unmultiplied([width, height], &display);
    if let Some(texture) = texture.as_mut() {
        texture.set(image, TextureOptions::NEAREST);
    } else {
        *texture = Some(
            ui.ctx()
                .load_texture("sprite-canvas", image, TextureOptions::NEAREST),
        );
    }
    let available = ui.available_size();
    let (response, painter) = ui.allocate_painter(available, egui::Sense::click_and_drag());
    let response = response.on_hover_cursor(egui::CursorIcon::Crosshair);
    painter.rect_filled(response.rect, 0.0, Color32::from_rgb(17, 20, 27));
    let zoom = editor.zoom.clamp(1.0, 32.0);
    let size = egui::vec2(width as f32 * zoom, height as f32 * zoom);
    let offset = egui::vec2(editor.pan[0], editor.pan[1]);
    let image_rect = egui::Rect::from_center_size(response.rect.center() + offset, size);
    let clipped = painter.with_clip_rect(response.rect);
    if let Some(texture) = texture {
        clipped.image(
            texture.id(),
            image_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    if editor.show_grid && zoom >= 8.0 {
        let stroke = egui::Stroke::new(0.5, Color32::from_black_alpha(45));
        for x in 0..=width {
            let screen_x = image_rect.left() + x as f32 * zoom;
            clipped.line_segment(
                [
                    egui::pos2(screen_x, image_rect.top()),
                    egui::pos2(screen_x, image_rect.bottom()),
                ],
                stroke,
            );
        }
        for y in 0..=height {
            let screen_y = image_rect.top() + y as f32 * zoom;
            clipped.line_segment(
                [
                    egui::pos2(image_rect.left(), screen_y),
                    egui::pos2(image_rect.right(), screen_y),
                ],
                stroke,
            );
        }
    }
    if let Some([x0, y0, x1, y1]) = editor.selection {
        let rect = egui::Rect::from_min_max(
            egui::pos2(
                image_rect.left() + x0 as f32 * zoom,
                image_rect.top() + y0 as f32 * zoom,
            ),
            egui::pos2(
                image_rect.left() + (x1 + 1) as f32 * zoom,
                image_rect.top() + (y1 + 1) as f32 * zoom,
            ),
        );
        clipped.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(2.0, Color32::from_rgb(231, 189, 80)),
            egui::StrokeKind::Inside,
        );
    }
    let pointer = ui.ctx().input(|input| {
        (
            input.pointer.hover_pos(),
            input.pointer.primary_pressed(),
            input.pointer.primary_down(),
            input.pointer.button_pressed(egui::PointerButton::Secondary),
            input.pointer.middle_down(),
            input.pointer.delta(),
            input.smooth_scroll_delta.y,
        )
    });
    let (position, primary_pressed, primary_down, secondary_pressed, middle_down, delta, wheel) =
        pointer;
    if let Some(position) = position {
        if response.rect.contains(position) && wheel.abs() > 0.0 {
            let old = editor.zoom;
            editor.zoom = (editor.zoom * if wheel > 0.0 { 1.25 } else { 0.8 }).clamp(1.0, 32.0);
            let ratio = editor.zoom / old;
            let relative = position - image_rect.center();
            editor.pan[0] -= relative.x * (ratio - 1.0);
            editor.pan[1] -= relative.y * (ratio - 1.0);
        }
        if middle_down && response.rect.contains(position) {
            editor.pan[0] += delta.x;
            editor.pan[1] += delta.y;
        }
        let point = if image_rect.contains(position) && response.rect.contains(position) {
            let x = ((position.x - image_rect.left()) / zoom).floor() as usize;
            let y = ((position.y - image_rect.top()) / zoom).floor() as usize;
            if x < width && y < height {
                Some((x, y))
            } else {
                None
            }
        } else {
            None
        };
        if secondary_pressed {
            if let Some((x, y)) = point {
                editor.pick(x, y);
            }
        }
        if primary_pressed {
            if let Some((x, y)) = point {
                match editor.tool {
                    Tool::Eyedropper => editor.pick(x, y),
                    Tool::Fill => {
                        editor.remember();
                        editor.fill(x, y);
                    }
                    Tool::Select => {
                        *selection_anchor = Some((x, y));
                        editor.selection = Some([x, y, x, y]);
                    }
                    Tool::Pencil | Tool::Eraser => {
                        editor.remember();
                        editor.paint_line((x as i32, y as i32), (x as i32, y as i32));
                        *painting = true;
                        *last_point = Some((x as i32, y as i32));
                    }
                }
            }
        } else if primary_down {
            if let Some((x, y)) = point {
                if let Some((anchor_x, anchor_y)) = *selection_anchor {
                    editor.selection = Some([
                        anchor_x.min(x),
                        anchor_y.min(y),
                        anchor_x.max(x),
                        anchor_y.max(y),
                    ]);
                } else if *painting {
                    let next = (x as i32, y as i32);
                    editor.paint_line(last_point.unwrap_or(next), next);
                    *last_point = Some(next);
                }
            }
        }
    }
    if !primary_down {
        *painting = false;
        *last_point = None;
        *selection_anchor = None;
    }
}

fn indexed_image(pixels: &[u8]) -> Option<image::RgbaImage> {
    if pixels.len() != SCREEN_WIDTH * SCREEN_HEIGHT {
        return None;
    }
    let palette = sprite::palette();
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for index in pixels {
        let [r, g, b] = palette[usize::from(*index)];
        rgba.extend_from_slice(&[r, g, b, 255]);
    }
    image::RgbaImage::from_raw(SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32, rgba)
}

fn save_path(id: &str) -> Result<PathBuf, String> {
    let directories = ProjectDirs::from("", "MOD-01", "Native Studio")
        .ok_or("cannot find application data directory")?;
    save_manager::storage_path(directories.data_dir(), id)
}

fn read_save(id: &str) -> Result<Vec<u8>, String> {
    save_manager::read_image(&save_path(id)?).map_err(|error| format!("save for '{id}': {error}"))
}

fn read_save_schema(id: &str) -> Result<u16, String> {
    save_manager::read_schema(&save_path(id)?)
}

fn write_save_schema(id: &str, schema: u16) -> Result<(), String> {
    save_manager::write_schema(&save_path(id)?, schema)
}

fn clear_save_schema(id: &str) -> Result<(), String> {
    save_manager::clear_schema(&save_path(id)?)
}

fn backup_save(id: &str) -> Result<(), String> {
    let bytes = read_save(id)?;
    let schema = read_save_schema(id)?;
    let saves = save_path(id)?;
    save_manager::backup(&saves, id, schema, &bytes).map(|_| ())
}

fn write_save(id: &str, bytes: &[u8]) -> Result<(), String> {
    save_manager::write_image(&save_path(id)?, bytes)
}

fn main() -> eframe::Result {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|directory| directory.join("game.m01c")))
        .filter(|path| path.is_file());
    let launch = match launch::parse(&args, bundled) {
        Ok(launch) => launch,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let (start, player_mode) = match launch {
        launch::Launch::Verify {
            path,
            frames,
            input,
        } => {
            match player::verify(&path, frames, input.as_deref()) {
                Ok(result) => println!("{result}"),
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        launch::Launch::Help => {
            println!(
                "mod01-studio [PATH | --play PATH | --studio PATH | --verify PATH [--frames N] [--input REPLAY]]"
            );
            return Ok(());
        }
        launch::Launch::Studio(path) => (path, false),
        launch::Launch::Player(path) => (Some(path), true),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MOD-01 / Color Development Unit")
            .with_inner_size(if player_mode {
                [960.0, 650.0]
            } else {
                [1280.0, 800.0]
            })
            .with_min_inner_size(player::minimum_size(player_mode, false)),
        ..Default::default()
    };
    eframe::run_native(
        "MOD-01 / Color Development Unit",
        options,
        Box::new(move |creation| {
            ui_assets::install_font(&creation.egui_ctx);
            ui_theme::install(&creation.egui_ctx);
            let mut app = StudioApp::new(start, &creation.egui_ctx);
            app.player_mode = player_mode;
            if player_mode {
                if let Some(project) = &app.project {
                    creation
                        .egui_ctx
                        .send_viewport_cmd(egui::ViewportCommand::Title(
                            project.manifest.title.clone(),
                        ));
                }
                app.run();
            }
            Ok(Box::new(app))
        }),
    )
}
