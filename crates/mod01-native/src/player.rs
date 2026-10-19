//! Standalone game surface; execution/input/audio/save devices are the Studio devices.
use crate::{
    SCREEN_HEIGHT, SCREEN_WIDTH, StudioApp, View, native_host_path, project::NativeProject, replay,
    runtime::RuntimeBridge, ui_theme,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use eframe::egui::{self, Key};
use serde_json::{Value, json};
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

impl StudioApp {
    pub fn player_ui(&mut self, root: &mut egui::Ui) {
        let context = root.ctx().clone();
        if context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.set_player_paused(!self.player_paused);
        }
        egui::Panel::top("game-controls")
            .frame(ui_theme::case_frame())
            .show(root, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading(
                        self.project
                            .as_ref()
                            .map_or("MOD-01", |project| &project.manifest.title),
                    );
                    if ui
                        .button(if self.player_paused {
                            "RESUME"
                        } else {
                            "PAUSE"
                        })
                        .clicked()
                    {
                        self.set_player_paused(!self.player_paused);
                    }
                    if ui.button("RESTART").clicked() {
                        self.player_paused = false;
                        self.run();
                    }
                    if ui.button("SOURCE").clicked() {
                        self.stop();
                        self.player_mode = false;
                        self.view = View::Code;
                    }
                    let mut touch = self.touch_controller.enabled();
                    if ui.checkbox(&mut touch, "TOUCH").changed() {
                        self.touch_controller.set_enabled(touch);
                    }
                });
            });
        egui::Panel::bottom("game-status")
            .frame(ui_theme::case_frame())
            .show(root, |ui| {
                ui.label(if self.player_paused {
                    "PAUSED / ESC TO RESUME"
                } else {
                    &self.status
                });
                ui.label("ARROWS / Z X A S / Q W / ENTER / ESC MENU");
                self.touch_controller.show(
                    ui,
                    self.running
                        && self.loaded
                        && !self.player_paused
                        && !self.debugging
                        && self.replay_playback.is_none(),
                );
            });
        if self.touch_controller.take_menu_request() {
            self.set_player_paused(!self.player_paused);
        }
        egui::CentralPanel::default()
            .frame(ui_theme::screen_frame())
            .show(root, |ui| {
                if let Some(texture) = &self.texture {
                    let available = ui.available_size();
                    let scale = (available.x / SCREEN_WIDTH as f32)
                        .min(available.y / SCREEN_HEIGHT as f32)
                        .floor()
                        .max(1.0);
                    let size =
                        egui::vec2(SCREEN_WIDTH as f32 * scale, SCREEN_HEIGHT as f32 * scale);
                    let image_rect = egui::Rect::from_center_size(ui.max_rect().center(), size);
                    let response = ui.put(image_rect, egui::Image::new((texture.id(), size)));
                    self.screen_rect = Some(response.rect);
                } else {
                    ui.label(&self.status);
                }
            });
        self.capture_keyboard(&context);
        self.send_frame(&context);
        self.cursors.paint(&context);
        context.request_repaint_after(Duration::from_millis(4));
    }
}

/// Exercises the exact native child-process bridge without a display, audio device or save writes.
pub fn verify(path: &Path, frames: u32) -> Result<Value, String> {
    let project = NativeProject::open(path)?;
    let bridge = RuntimeBridge::new(native_host_path());
    bridge.send(project.load_command(false, &[])?)?;
    let receive = || -> Result<Value, String> {
        let deadline = Instant::now() + Duration::from_secs(7);
        loop {
            if let Some(value) = bridge.poll() {
                if value["type"] == "error" {
                    return Err(value["message"].to_string());
                }
                return Ok(value);
            }
            if Instant::now() >= deadline {
                return Err("native verification timed out".to_owned());
            }
            thread::sleep(Duration::from_millis(1));
        }
    };
    if receive()?["type"] != "loaded" {
        return Err("native load failed".to_owned());
    }
    let mut peak = 0;
    let mut fingerprint = 0_u64;
    for frame in 0..frames {
        bridge.send(json!({"type":"frame","input":replay::empty_input()}))?;
        let value = receive()?;
        if value["type"] != "frame" || value["frame"] != frame {
            return Err("native frame sequence differs".to_owned());
        }
        let pixels = STANDARD
            .decode(value["pixels"].as_str().ok_or("missing pixels")?)
            .map_err(|error| error.to_string())?;
        if pixels.len() != SCREEN_WIDTH * SCREEN_HEIGHT {
            return Err("invalid native frame size".to_owned());
        }
        for channel in ["left", "right"] {
            if STANDARD
                .decode(value[channel].as_str().ok_or("missing PCM")?)
                .map_err(|error| error.to_string())?
                .len()
                != 800 * 4
            {
                return Err("invalid native PCM frame size".to_owned());
            }
        }
        fingerprint = pixels.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
        });
        peak = peak.max(value["workUnits"].as_u64().unwrap_or(0));
    }
    bridge.send(json!({"type":"stop"}))?;
    if receive()?["type"] != "stopped" {
        return Err("native stop failed".to_owned());
    }
    Ok(
        json!({"cartridge":project.manifest.id,"frames":frames,"workPeak":peak,"framebufferFnv64":format!("{fingerprint:016x}")}),
    )
}
