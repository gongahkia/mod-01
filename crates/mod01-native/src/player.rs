//! Standalone game surface; execution/input/audio/save devices are the Studio devices.
use crate::{
    SCREEN_HEIGHT, SCREEN_WIDTH, StudioApp, native_host_path, project::NativeProject, replay,
    runtime::RuntimeBridge, ui_theme,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use eframe::egui;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    thread,
    time::{Duration, Instant},
};

impl StudioApp {
    pub fn player_ui(&mut self, root: &mut egui::Ui) {
        let context = root.ctx().clone();
        egui::Panel::top("game-controls")
            .frame(ui_theme::case_frame())
            .show(root, |ui| {
                let narrow = ui.available_width() < 640.0;
                if narrow {
                    let title = self.runtime_title();
                    ui.add(egui::Label::new(egui::RichText::new(&title).heading()).truncate())
                        .on_hover_text(title);
                }
                ui.horizontal_wrapped(|ui| {
                    if !narrow {
                        ui.heading(self.runtime_title());
                    }
                    if ui
                        .add_enabled(
                            self.running,
                            egui::Button::new(if self.player_paused {
                                "RESUME"
                            } else {
                                "PAUSE"
                            }),
                        )
                        .clicked()
                    {
                        self.set_player_paused(!self.player_paused);
                    }
                    if ui.button("RESTART").clicked() {
                        self.system_menu_action(crate::pause_menu::Action::Restart);
                    }
                    if ui.button("SOURCE").clicked() {
                        self.system_menu_action(crate::pause_menu::Action::Source);
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
                let status = if self.player_paused {
                    "PAUSED / ESC TO RESUME"
                } else {
                    &self.status
                };
                let audio = if self.audio.is_some() {
                    "AUD"
                } else {
                    "SILENT"
                };
                ui.add(egui::Label::new(format!("{audio} / {status}")).truncate())
                    .on_hover_text(format!("{status}\n{}", self.audio_message()));
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
        if self.touch_controller.take_menu_request() && self.running {
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
        self.system_menu_ui(&context);
        self.advance_runtime(&context);
        self.cursors.paint(&context);
        context.request_repaint_after(Duration::from_millis(4));
    }
}

/// Exercises the exact native child-process bridge without a display, audio device or save writes.
#[allow(clippy::too_many_lines)] // one ordered load/frame/stop verification workflow
pub fn verify(path: &Path, frames: u32) -> Result<Value, String> {
    let project = NativeProject::open(path)?;
    let game = project.game()?;
    let bridge = RuntimeBridge::new(native_host_path());
    bridge.send(NativeProject::game_command(&game, false, &BTreeMap::new())?)?;
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
    let loaded = receive()?;
    if loaded["type"] != "loaded" || loaded["identity"] != game.identity() {
        return Err("native load failed".to_owned());
    }
    let mut peak = 0;
    let mut pixels_hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut audio_hash = pixels_hash;
    let mut saves_hash = pixels_hash;
    let mut timeline_hash = pixels_hash;
    let mut member_frames: BTreeMap<String, u32> = BTreeMap::new();
    let mut save_commit_frames: BTreeMap<String, u32> = BTreeMap::new();
    let mut audio_peak = 0.0_f32;
    for frame in 0..frames {
        bridge.send(json!({"type":"frame","input":replay::empty_input()}))?;
        let value = receive()?;
        if value["type"] != "frame" || value["frame"] != frame {
            return Err("native frame sequence differs".to_owned());
        }
        let alias = value["cartridgeAlias"]
            .as_str()
            .ok_or("missing frame producer")?;
        let member = game
            .catalog()
            .cartridges()
            .get(alias)
            .ok_or("unknown frame producer")?;
        if value["cartridgeId"] != member.manifest.id {
            return Err("native frame producer identity differs".to_owned());
        }
        *member_frames.entry(alias.to_owned()).or_default() += 1;
        fingerprint(
            &mut timeline_hash,
            &serde_json::to_vec(&json!({
                "alias":alias, "id":member.manifest.id, "local":value["cartridgeFrame"],
                "active":value["active"], "work":value["workUnits"]
            }))
            .map_err(|error| error.to_string())?,
        );
        let pixels = STANDARD
            .decode(value["pixels"].as_str().ok_or("missing pixels")?)
            .map_err(|error| error.to_string())?;
        if pixels.len() != SCREEN_WIDTH * SCREEN_HEIGHT {
            return Err("invalid native frame size".to_owned());
        }
        fingerprint(&mut pixels_hash, &pixels);
        for channel in ["left", "right"] {
            let pcm = STANDARD
                .decode(value[channel].as_str().ok_or("missing PCM")?)
                .map_err(|error| error.to_string())?;
            if pcm.len() != 800 * 4 {
                return Err("invalid native PCM frame size".to_owned());
            }
            for bytes in pcm.chunks_exact(4) {
                let sample = f32::from_le_bytes(bytes.try_into().expect("complete sample"));
                if !sample.is_finite() {
                    return Err("non-finite native PCM sample".to_owned());
                }
                audio_peak = audio_peak.max(sample.abs());
            }
            fingerprint(&mut audio_hash, &pcm);
        }
        if let Some(commit) = value["saveCommit"].as_str() {
            let save = STANDARD.decode(commit).map_err(|error| error.to_string())?;
            if save.len() != 8192 {
                return Err("invalid native save image size".to_owned());
            }
            *save_commit_frames.entry(alias.to_owned()).or_default() += 1;
            fingerprint(&mut saves_hash, &frame.to_le_bytes());
            fingerprint(&mut saves_hash, alias.as_bytes());
            fingerprint(&mut saves_hash, member.manifest.id.as_bytes());
            fingerprint(&mut saves_hash, &save);
        }
        peak = peak.max(value["workUnits"].as_u64().unwrap_or(0));
    }
    bridge.send(json!({"type":"stop"}))?;
    if receive()?["type"] != "stopped" {
        return Err("native stop failed".to_owned());
    }
    Ok(
        json!({"cartridge":game.entry().manifest.id,"gameIdentity":game.identity(),"frames":frames,
            "memberFrames":member_frames,"workPeak":peak,"framebufferFnv64":format!("{pixels_hash:016x}"),
            "audioFnv64":format!("{audio_hash:016x}"),"audioPeak":audio_peak,
            "saveCommitsFnv64":format!("{saves_hash:016x}"),"saveCommitFrames":save_commit_frames,
            "timelineFnv64":format!("{timeline_hash:016x}")}),
    )
}

fn fingerprint(hash: &mut u64, bytes: &[u8]) {
    *hash = bytes.iter().fold(*hash, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_status_and_long_titles_leave_the_entire_framebuffer_visible_at_minimum_player_size() {
        for size in [egui::vec2(480.0, 320.0), egui::vec2(960.0, 650.0)] {
            let context = egui::Context::default();
            crate::ui_assets::install_font(&context);
            ui_theme::install(&context);
            let mut app = StudioApp::new_with_devices(None, &context, false);
            let mut project = NativeProject::from_source(
                include_str!("../../../templates/blank/cart.m01w").to_owned(),
                None,
            )
            .unwrap();
            project.manifest.title = "LONG CARTRIDGE TITLE ".repeat(3);
            app.project = Some(project);
            app.status = "Runtime status with an intentionally long member identifier and output-device detail".to_owned();
            app.texture = Some(context.load_texture(
                "minimum-player",
                egui::ColorImage::filled([240, 144], egui::Color32::WHITE),
                egui::TextureOptions::NEAREST,
            ));
            let mut last = None;
            for _ in 0..6 {
                let output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| app.player_ui(ui),
                );
                last = Some(output.shapes.clone());
                output.drop_without_applying_deltas();
            }
            let screen = app.screen_rect.unwrap();
            assert!(screen.width() >= 240.0 && screen.height() >= 144.0);
            let mut controls_bottom = 0.0_f32;
            let mut status_top = size.y;
            for shape in last.unwrap() {
                if let egui::Shape::Text(text) = shape.shape {
                    let label = text.galley.job.text.as_str();
                    if ["PAUSE", "RESTART", "SOURCE", "TOUCH"].contains(&label) {
                        controls_bottom = controls_bottom.max(text.pos.y + text.galley.size().y);
                    }
                    if label.starts_with("SILENT /") {
                        status_top = status_top.min(text.pos.y);
                    }
                }
            }
            assert!(controls_bottom > 0.0 && status_top < size.y);
            assert!(
                screen.top() >= controls_bottom,
                "{size:?}: screen covers controls"
            );
            assert!(
                screen.bottom() <= status_top,
                "{size:?}: screen is clipped by status"
            );
        }
    }
}
