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

mod audio;
mod project;
mod runtime;
mod sprite;

use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use audio::AudioOutput;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use directories::ProjectDirs;
use eframe::egui::{self, Color32, ColorImage, Key, RichText, TextureHandle, TextureOptions};
use gilrs::{Axis, Button, Gilrs};
use project::NativeProject;
use runtime::RuntimeBridge;
use serde_json::{Value, json};
use sprite::{DEFAULT_AXIS, SpriteDocument, SpriteEditor, Tool};

const SCREEN_WIDTH: usize = 240;
const SCREEN_HEIGHT: usize = 144;
const PALETTE: [[u8; 3]; 32] = [
    [23, 20, 31],
    [41, 37, 50],
    [64, 57, 70],
    [93, 80, 84],
    [128, 106, 99],
    [170, 139, 116],
    [213, 185, 146],
    [244, 229, 189],
    [91, 41, 56],
    [139, 60, 71],
    [191, 85, 88],
    [237, 123, 105],
    [90, 57, 40],
    [137, 87, 46],
    [193, 132, 54],
    [231, 189, 80],
    [38, 60, 50],
    [52, 95, 70],
    [75, 139, 88],
    [127, 189, 104],
    [32, 59, 71],
    [46, 101, 113],
    [67, 150, 154],
    [117, 203, 192],
    [36, 52, 81],
    [52, 88, 129],
    [75, 125, 179],
    [115, 169, 209],
    [62, 49, 84],
    [101, 71, 119],
    [147, 99, 151],
    [195, 138, 174],
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Code,
    Sprites,
    Assets,
    Player,
    Manifest,
}

struct StudioApp {
    project: Option<NativeProject>,
    manifest_text: String,
    view: View,
    runtime: RuntimeBridge,
    audio: Option<AudioOutput>,
    gamepads: Option<Gilrs>,
    texture: Option<TextureHandle>,
    sprite: Option<SpriteEditor>,
    sprite_texture: Option<TextureHandle>,
    sprite_name: String,
    asset_name: String,
    asset_path: String,
    asset_text: String,
    asset_dirty: bool,
    color_hex: String,
    sprite_painting: bool,
    last_sprite_point: Option<(i32, i32)>,
    selection_anchor: Option<(usize, usize)>,
    running: bool,
    debugging: bool,
    snapshot: Option<String>,
    debug_event: String,
    loaded: bool,
    pending: bool,
    frame: u32,
    work: u32,
    last_tick: Instant,
    screen_rect: Option<egui::Rect>,
    status: String,
    new_path: String,
    new_id: String,
    show_new: bool,
}

impl StudioApp {
    fn new(start: Option<PathBuf>) -> Self {
        let host_path = native_host_path();
        let mut app = Self {
            project: None,
            manifest_text: String::new(),
            view: View::Code,
            runtime: RuntimeBridge::new(host_path),
            audio: AudioOutput::new().ok(),
            gamepads: Gilrs::new().ok(),
            texture: None,
            sprite: None,
            sprite_texture: None,
            sprite_name: "hero".to_owned(),
            asset_name: String::new(),
            asset_path: String::new(),
            asset_text: String::new(),
            asset_dirty: false,
            color_hex: "ED7B69FF".to_owned(),
            sprite_painting: false,
            last_sprite_point: None,
            selection_anchor: None,
            running: false,
            debugging: false,
            snapshot: None,
            debug_event: String::new(),
            loaded: false,
            pending: false,
            frame: 0,
            work: 0,
            last_tick: Instant::now(),
            screen_rect: None,
            status: "MOD-01 NATIVE STUDIO / READY".to_owned(),
            new_path: String::new(),
            new_id: "new-cart".to_owned(),
            show_new: false,
        };
        if let Some(path) = start {
            app.open(path);
        }
        app
    }

    fn open(&mut self, path: PathBuf) {
        match NativeProject::open(&path) {
            Ok(project) => {
                self.stop();
                self.manifest_text = project.manifest_source.clone();
                self.status = format!("OPENED {}", project.manifest.title);
                self.project = Some(project);
                self.sprite = None;
                self.asset_name.clear();
                self.asset_dirty = false;
                self.view = View::Code;
            }
            Err(error) => self.status = error,
        }
    }

    fn open_sample(&mut self, path: PathBuf, play: bool) {
        match NativeProject::open(&path) {
            Ok(mut project) => {
                project.root = None;
                self.stop();
                self.manifest_text = project.manifest_source.clone();
                self.status = format!("OPENED SAMPLE {}", project.manifest.title);
                self.project = Some(project);
                self.sprite = None;
                self.asset_name.clear();
                self.asset_dirty = false;
                self.view = View::Code;
                if play {
                    self.run();
                }
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
                self.sprite = None;
                self.asset_name.clear();
                self.asset_dirty = false;
                self.show_new = false;
                self.view = View::Code;
            }
            Err(error) => self.status = error,
        }
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
        let Some(project) = self.project.as_mut() else {
            return;
        };
        match modl_core::parse_project_manifest(&requested_manifest) {
            Ok(manifest) => {
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
            .is_some_and(|project| project.root.is_none())
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
        let previous_root = project.root.clone();
        project.root = Some(path);
        match project.save() {
            Ok(()) => {
                project.dirty = false;
                self.status = "PROJECT SAVED".to_owned();
            }
            Err(error) => {
                project.root = previous_root;
                self.status = error;
            }
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
        match project.pack() {
            Ok(bytes) => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(format!("{}.m01c", project.manifest.id))
                    .add_filter("MOD-01 cartridge", &["m01c"])
                    .save_file()
                {
                    self.status = match fs::write(path, &bytes) {
                        Ok(()) => format!("PACKED {} BYTES", bytes.len()),
                        Err(error) => error.to_string(),
                    };
                }
            }
            Err(error) => self.status = error,
        }
    }

    fn export_player(&mut self, zip: bool) {
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
        let output = if zip {
            modl_core::export_itch_zip(&project.manifest_source, &project.files)
        } else {
            modl_core::export_standalone_html(&project.manifest_source, &project.files)
                .map(String::into_bytes)
        };
        match output {
            Ok(bytes) => {
                let extension = if zip { "zip" } else { "html" };
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(format!("{}.{}", project.manifest.id, extension))
                    .save_file()
                {
                    self.status = match fs::write(path, &bytes) {
                        Ok(()) => format!("EXPORTED {} BYTES", bytes.len()),
                        Err(error) => error.to_string(),
                    };
                }
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn run(&mut self) {
        self.run_mode(false);
    }

    fn run_mode(&mut self, debug: bool) {
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
        let (Ok(rom), Ok(javascript)) = (project.pack(), project.javascript(debug)) else {
            self.status = project.check().unwrap_or_else(|error| error);
            return;
        };
        let files: serde_json::Map<String, Value> = project
            .manifest
            .assets
            .values()
            .filter_map(|asset| {
                project
                    .files
                    .get(&asset.path)
                    .map(|bytes| (asset.path.clone(), Value::String(STANDARD.encode(bytes))))
            })
            .chain(project.manifest.display.iter().filter_map(|path| {
                project
                    .files
                    .get(path)
                    .map(|bytes| (path.clone(), Value::String(STANDARD.encode(bytes))))
            }))
            .collect();
        let save = read_save(&project.manifest.id).unwrap_or_default();
        let command = json!({
            "type":"load", "javascript":javascript,
            "manifest": {
                "id": project.manifest.id,
                "hardwareRevision": project.manifest.hardware_revision,
                "updateRate": project.manifest.update_rate,
                "display": project.manifest.display,
                "assets": project.manifest.assets,
            },
            "files": files,
            "rom": STANDARD.encode(rom),
            "save": STANDARD.encode(save),
            "debug": debug,
        });
        if let Some(audio) = &self.audio {
            audio.clear();
        }
        self.texture = None;
        self.screen_rect = None;
        match self.runtime.send(command) {
            Ok(()) => {
                self.running = true;
                self.debugging = debug;
                self.snapshot = None;
                self.debug_event.clear();
                self.loaded = false;
                self.pending = true;
                self.frame = 0;
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
        self.running = false;
        self.loaded = false;
        self.pending = false;
        if let Some(audio) = &self.audio {
            audio.clear();
        }
        let _ = self.runtime.send(json!({"type":"stop"}));
    }

    fn process_events(&mut self, context: &egui::Context) {
        while let Some(event) = self.runtime.poll() {
            self.pending = false;
            match event["type"].as_str() {
                Some("loaded") => {
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
                        self.status = "DEBUG BOOT COMPLETE".to_owned();
                    } else if event["event"].is_object() {
                        self.debug_event = serde_json::to_string_pretty(&event["event"])
                            .unwrap_or_else(|_| "debug event".to_owned());
                        self.status = "PAUSED AT SOURCE STEP".to_owned();
                    }
                }
                Some("snapshot") => {
                    self.snapshot = event["snapshot"].as_str().map(str::to_owned);
                    self.status = "DEBUG SNAPSHOT CAPTURED".to_owned();
                }
                Some("restored") => {
                    self.status = "DEBUG SNAPSHOT RESTORED".to_owned();
                    if let Some(audio) = &self.audio {
                        audio.clear();
                    }
                }
                Some("stopped") => {}
                Some("error") => {
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
    }

    fn accept_frame(&mut self, event: &Value, context: &egui::Context) {
        self.frame = event["frame"].as_u64().unwrap_or(0) as u32;
        self.work = event["workUnits"].as_u64().unwrap_or(0) as u32;
        if let Some(encoded) = event["pixels"].as_str() {
            if let Ok(pixels) = STANDARD.decode(encoded) {
                if pixels.len() == SCREEN_WIDTH * SCREEN_HEIGHT {
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
        if let (Some(audio), Some(left), Some(right)) =
            (&self.audio, event["left"].as_str(), event["right"].as_str())
        {
            if let Err(error) = audio.push(left, right) {
                self.status = format!("AUDIO: {error}");
            }
        }
        if let (Some(project), Some(encoded)) =
            (self.project.as_ref(), event["saveCommit"].as_str())
        {
            if let Ok(bytes) = STANDARD.decode(encoded) {
                if let Err(error) = write_save(&project.manifest.id, &bytes) {
                    self.status = error;
                }
            }
        }
    }

    fn send_frame(&mut self, context: &egui::Context) {
        if !self.running
            || !self.loaded
            || self.pending
            || self.debugging
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
        if self.running && self.loaded && !self.pending && self.debugging {
            self.send_input(context, "step");
        }
    }

    fn send_input(&mut self, context: &egui::Context, kind: &str) {
        let buttons = context.input(|input| {
            let mut buttons = serde_json::Map::new();
            for name in [
                "up", "down", "left", "right", "a", "b", "x", "y", "l", "r", "start", "menu",
            ] {
                let key = match name {
                    "up" => Key::ArrowUp,
                    "down" => Key::ArrowDown,
                    "left" => Key::ArrowLeft,
                    "right" => Key::ArrowRight,
                    "a" => Key::Z,
                    "b" => Key::X,
                    "x" => Key::A,
                    "y" => Key::S,
                    "l" => Key::Q,
                    "r" => Key::W,
                    "start" => Key::Enter,
                    _ => Key::Escape,
                };
                buttons.insert(name.to_owned(), Value::Bool(input.key_down(key)));
            }
            buttons
        });
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
        let empty = || {
            let mut values = serde_json::Map::new();
            for key in buttons.keys() {
                values.insert(key.clone(), Value::Bool(false));
            }
            json!({"buttons":values})
        };
        let mut controllers = vec![json!({"buttons":buttons}), empty(), empty(), empty()];
        if let Some(gamepads) = self.gamepads.as_mut() {
            while gamepads.next_event().is_some() {}
            for (port, (_, gamepad)) in gamepads
                .gamepads()
                .filter(|(_, gamepad)| gamepad.is_connected())
                .take(4)
                .enumerate()
            {
                let states = [
                    (
                        "up",
                        gamepad.is_pressed(Button::DPadUp) || gamepad.value(Axis::LeftStickY) > 0.5,
                    ),
                    (
                        "down",
                        gamepad.is_pressed(Button::DPadDown)
                            || gamepad.value(Axis::LeftStickY) < -0.5,
                    ),
                    (
                        "left",
                        gamepad.is_pressed(Button::DPadLeft)
                            || gamepad.value(Axis::LeftStickX) < -0.5,
                    ),
                    (
                        "right",
                        gamepad.is_pressed(Button::DPadRight)
                            || gamepad.value(Axis::LeftStickX) > 0.5,
                    ),
                    ("a", gamepad.is_pressed(Button::South)),
                    ("b", gamepad.is_pressed(Button::East)),
                    ("x", gamepad.is_pressed(Button::West)),
                    ("y", gamepad.is_pressed(Button::North)),
                    ("l", gamepad.is_pressed(Button::LeftTrigger)),
                    ("r", gamepad.is_pressed(Button::RightTrigger)),
                    ("start", gamepad.is_pressed(Button::Start)),
                    ("menu", gamepad.is_pressed(Button::Select)),
                ];
                for (name, pressed) in states {
                    let before = controllers[port]["buttons"][name]
                        .as_bool()
                        .unwrap_or(false);
                    controllers[port]["buttons"][name] = Value::Bool(before || pressed);
                }
            }
        }
        let input = json!({"controllers":controllers,"pointer":pointer});
        if let Err(error) = self.runtime.send(json!({"type":kind,"input":input})) {
            self.status = error;
            self.running = false;
        } else {
            self.pending = true;
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("M01")
                    .strong()
                    .color(Color32::from_rgb(231, 189, 80)),
            );
            ui.separator();
            if ui.button("OPEN PROJECT").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.open(path);
                }
            }
            if ui.button("OPEN CART").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("MOD-01 cartridge", &["m01c"])
                    .pick_file()
                {
                    self.open(path);
                }
            }
            if ui.button("NEW").clicked() {
                self.show_new = true;
            }
            ui.add_enabled_ui(self.project.is_some(), |ui| {
                if ui.button("SAVE").clicked() {
                    self.save();
                }
                if ui.button("SAVE AS").clicked() {
                    self.save_as();
                }
                if ui.button("CHECK").clicked() {
                    self.check();
                }
                if ui.button("PACK").clicked() {
                    self.pack();
                }
                ui.menu_button("EXPORT", |ui| {
                    if ui.button("STANDALONE HTML").clicked() {
                        self.export_player(false);
                    }
                    if ui.button("ITCH.IO ZIP").clicked() {
                        self.export_player(true);
                    }
                });
                if ui.button("RUN").clicked() {
                    self.run();
                }
                if ui.button("DEBUG").clicked() {
                    self.run_mode(true);
                }
            });
            if ui
                .add_enabled(self.running, egui::Button::new("STOP"))
                .clicked()
            {
                self.stop();
            }
            if self.debugging && self.running {
                if ui
                    .add_enabled(self.loaded && !self.pending, egui::Button::new("STEP"))
                    .clicked()
                {
                    self.step(ui.ctx());
                }
                if ui
                    .add_enabled(self.loaded && !self.pending, egui::Button::new("SNAPSHOT"))
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
        if !self.asset_dirty {
            return Ok(());
        }
        let _: Value = serde_json::from_str(&self.asset_text)
            .map_err(|error| format!("{}: {error}", self.asset_path))?;
        let project = self.project.as_mut().ok_or("no project is open")?;
        project
            .files
            .insert(self.asset_path.clone(), self.asset_text.as_bytes().to_vec());
        project.dirty = true;
        self.asset_dirty = false;
        Ok(())
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
        match String::from_utf8(bytes.clone()) {
            Ok(source) => {
                self.sprite = None;
                self.asset_name = name.to_owned();
                self.asset_path = asset.path.clone();
                self.asset_text = source;
                self.asset_dirty = false;
                self.view = View::Assets;
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn load_sprite(&mut self, name: &str) {
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
        let sidecar = format!("editor/sprites/{name}.m01paint");
        let palette = sprite::palette();
        let editor = if let Some(source) = project.files.get(&sidecar) {
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
                self.sprite = Some(editor);
                self.sprite_texture = None;
                if let Err(error) = self.save_sprite() {
                    self.status = error;
                } else {
                    self.status = format!("CREATED {name} / 64×64");
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
        let frames = editor.document.indexed_frames(&sprite::palette());
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
            if ui.button("CREATE 64×64").clicked() {
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
            ui.label("Select an existing sprite or create a new 64×64 sprite.");
            return;
        };
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(272.0);
                ui.label(
                    RichText::new(format!(
                        "{}  •  FRAME {}/{}",
                        editor.name.to_uppercase(),
                        editor.frame + 1,
                        editor.document.frames.len()
                    ))
                    .strong(),
                );
                ui.horizontal(|ui| {
                    if ui.button("◀").clicked() {
                        editor.frame = (editor.frame + editor.document.frames.len() - 1)
                            % editor.document.frames.len();
                        editor.layer = editor
                            .layer
                            .min(editor.document.frames[editor.frame].layers.len() - 1);
                    }
                    if ui.button("▶").clicked() {
                        editor.frame = (editor.frame + 1) % editor.document.frames.len();
                        editor.layer = editor
                            .layer
                            .min(editor.document.frames[editor.frame].layers.len() - 1);
                    }
                    if ui.button("+ FRAME").clicked() {
                        editor.add_frame();
                    }
                    if ui.button("− FRAME").clicked() {
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
                    ui.label("×");
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
                            match fs::read(path)
                                .map_err(|error| error.to_string())
                                .and_then(|bytes| editor.import_png(&bytes))
                            {
                                Ok(()) => {
                                    self.status = "PNG IMPORTED INTO CURRENT LAYER".to_owned()
                                }
                                Err(error) => self.status = error,
                            }
                        }
                    }
                    if ui.button("PNG OUT").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name(format!("{}.png", editor.name))
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
                    if ui.button("−").clicked() {
                        editor.delete_layer();
                    }
                    if ui.button("↑").clicked() {
                        editor.move_layer(1);
                    }
                    if ui.button("↓").clicked() {
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
                            .selectable_label(editor.layer == index, format!("{}", index + 1))
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
                        egui::Grid::new("sprite-palette")
                            .spacing([3.0, 3.0])
                            .show(ui, |ui| {
                                for (index, rgb) in palette.iter().enumerate() {
                                    let fill = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                                    if ui
                                        .add_sized([36.0, 30.0], egui::Button::new("").fill(fill))
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
                            });
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
        self.process_events(&context);
        context.set_visuals(egui::Visuals::dark());
        egui::Panel::top("toolbar").show(root, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(root, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if let Some(project) = &self.project {
                    ui.separator();
                    ui.label(format!(
                        "{}{}",
                        project.manifest.id,
                        if project.dirty { " *" } else { "" }
                    ));
                }
                if self.running {
                    ui.separator();
                    ui.label(format!("FRAME {}  WORK {}", self.frame, self.work));
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
        if self.project.is_none() {
            egui::CentralPanel::default().show(root, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(48.0);
                    ui.heading("MOD-01 COLOR DEVELOPMENT UNIT");
                    ui.label("Native Studio • Open a project or choose a cartridge below");
                    ui.add_space(24.0);
                    ui.label(RichText::new("BUNDLED CARTRIDGES").strong());
                    ui.add_space(8.0);
                    egui::Grid::new("sample-cartridges")
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            for path in bundled_samples() {
                                let title = path
                                    .file_name()
                                    .and_then(|value| value.to_str())
                                    .unwrap_or("CARTRIDGE")
                                    .replace('-', " ")
                                    .to_uppercase();
                                ui.label(title);
                                if ui.button("OPEN").clicked() {
                                    self.open_sample(path.clone(), false);
                                }
                                if ui.button("PLAY").clicked() {
                                    self.open_sample(path, true);
                                }
                                ui.end_row();
                            }
                        });
                });
            });
            return;
        }
        egui::Panel::left("project")
            .default_size(205.0)
            .show(root, |ui| {
                ui.heading("PROJECT");
                ui.separator();
                for (view, title) in [
                    (View::Code, "CODE"),
                    (View::Sprites, "SPRITES"),
                    (View::Assets, "ASSETS"),
                    (View::Player, "PLAYER"),
                    (View::Manifest, "MANIFEST"),
                ] {
                    if ui.selectable_label(self.view == view, title).clicked() {
                        self.view = view;
                    }
                }
                ui.separator();
                if let Some(project) = self.project.as_mut() {
                    ui.label("SOURCE FILES");
                    for path in project.source_paths() {
                        if ui
                            .selectable_label(project.selected_file == path, &path)
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
                ui.label("ASSETS");
                let names: Vec<String> = self.project.as_ref().map_or_else(Vec::new, |project| {
                    project.manifest.assets.keys().cloned().collect()
                });
                for name in names {
                    let selected = self.asset_name == name
                        || self
                            .sprite
                            .as_ref()
                            .is_some_and(|sprite| sprite.name == name);
                    if ui.selectable_label(selected, &name).clicked() {
                        self.open_asset(&name);
                    }
                }
            });
        egui::CentralPanel::default().show(root, |ui| match self.view {
            View::Code => {
                if let Some(project) = self.project.as_mut() {
                    ui.heading(&project.selected_file);
                    let response = ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(&mut project.editor_text)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_rows(30),
                    );
                    if response.changed() {
                        project.dirty = true;
                    }
                }
            }
            View::Sprites => self.sprite_ui(ui),
            View::Assets => {
                if self.asset_name.is_empty() {
                    ui.heading("ASSETS");
                    ui.label("Choose a map, font, sound or music asset from the project list.");
                } else {
                    ui.heading(&self.asset_name);
                    ui.label(&self.asset_path);
                    if ui.button("APPLY JSON").clicked() {
                        self.status = match self.sync_asset() {
                            Ok(()) => "ASSET APPLIED".to_owned(),
                            Err(error) => error,
                        };
                    }
                    if ui
                        .add_sized(
                            ui.available_size(),
                            egui::TextEdit::multiline(&mut self.asset_text)
                                .font(egui::TextStyle::Monospace)
                                .code_editor(),
                        )
                        .changed()
                    {
                        self.asset_dirty = true;
                    }
                }
            }
            View::Player => {
                ui.heading("PLAYER");
                ui.label("ARROWS + Z X A S • Q W • ENTER");
                ui.add_space(12.0);
                if let Some(texture) = &self.texture {
                    let mut available = ui.available_size();
                    if self.debugging {
                        available.y = (available.y - 210.0).max(144.0);
                    }
                    let scale = (available.x / SCREEN_WIDTH as f32)
                        .min(available.y / SCREEN_HEIGHT as f32)
                        .floor()
                        .max(1.0);
                    let size =
                        egui::vec2(SCREEN_WIDTH as f32 * scale, SCREEN_HEIGHT as f32 * scale);
                    let response = ui.add(egui::Image::new((texture.id(), size)));
                    self.screen_rect = Some(response.rect);
                } else {
                    ui.label(if self.running {
                        "BOOTING CARTRIDGE..."
                    } else {
                        "PRESS RUN TO START"
                    });
                }
                if self.debugging {
                    ui.separator();
                    ui.label("SOURCE STEP");
                    egui::ScrollArea::vertical()
                        .max_height(190.0)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(if self.debug_event.is_empty() {
                                    "Press STEP to advance the debug cartridge."
                                } else {
                                    &self.debug_event
                                })
                                .monospace(),
                            );
                        });
                }
            }
            View::Manifest => {
                ui.heading("cart.toml");
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
        self.send_frame(&context);
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
        .filter(|path| path.join("cart.toml").is_file())
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

fn save_path(id: &str) -> Result<PathBuf, String> {
    let directories = ProjectDirs::from("", "MOD-01", "Native Studio")
        .ok_or("cannot find application data directory")?;
    Ok(directories
        .data_dir()
        .join("saves")
        .join(format!("{id}.bin")))
}

fn read_save(id: &str) -> Result<Vec<u8>, String> {
    let path = save_path(id)?;
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.to_string()),
    }
}

fn write_save(id: &str, bytes: &[u8]) -> Result<(), String> {
    let path = save_path(id)?;
    let parent = path.parent().ok_or("invalid save path")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temporary, &path).map_err(|error| error.to_string())
}

fn main() -> eframe::Result {
    let start = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MOD-01 Native Studio")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "MOD-01 Native Studio",
        options,
        Box::new(move |_context| Ok(Box::new(StudioApp::new(start)))),
    )
}
