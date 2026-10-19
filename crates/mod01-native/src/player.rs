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

pub(super) fn minimum_size(player: bool, touch: bool) -> egui::Vec2 {
    if player {
        egui::vec2(480.0, if touch { 500.0 } else { 320.0 })
    } else {
        egui::vec2(900.0, 600.0)
    }
}

impl StudioApp {
    pub(super) fn sync_viewport_mode(&mut self, context: &egui::Context) {
        let mode = (
            self.player_mode,
            self.player_mode && self.touch_controller.enabled(),
        );
        if self.viewport_mode == Some(mode) {
            return;
        }
        self.viewport_mode = Some(mode);
        let minimum = minimum_size(mode.0, mode.1);
        context.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(minimum));
        let current = context.content_rect().size();
        if current.x < minimum.x || current.y < minimum.y {
            context.send_viewport_cmd(egui::ViewportCommand::InnerSize(current.max(minimum)));
        }
    }

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
pub fn verify(path: &Path, frames: u32, input: Option<&Path>) -> Result<Value, String> {
    if !(1..=36_000).contains(&frames) {
        return Err("verification requires 1-36000 frames".to_owned());
    }
    let inputs = input.map(replay::read).transpose()?.unwrap_or_default();
    if inputs
        .last_key_value()
        .is_some_and(|(frame, _)| *frame >= frames)
    {
        return Err("replay contains input outside the requested verification frames".to_owned());
    }
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
    let mut input_hash = pixels_hash;
    let mut member_frames: BTreeMap<String, u32> = BTreeMap::new();
    let mut save_commit_frames: BTreeMap<String, u32> = BTreeMap::new();
    let mut audio_peak = 0.0_f32;
    let mut frame_checks = Vec::with_capacity(frames as usize);
    for frame in 0..frames {
        let input = inputs
            .get(&frame)
            .cloned()
            .unwrap_or_else(replay::empty_input);
        fingerprint(&mut input_hash, &frame.to_le_bytes());
        fingerprint(
            &mut input_hash,
            &serde_json::to_vec(&input).map_err(|error| error.to_string())?,
        );
        bridge.send(json!({"type":"frame","input":input}))?;
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
        let mut check = checked_hashes(
            &value["checks"],
            &["framebufferSha256", "pcmSha256", "audioCommandSha256"],
        )?;
        for key in [
            "frame",
            "cartridgeFrame",
            "cartridgeAlias",
            "cartridgeId",
            "workUnits",
        ] {
            check[key] = value[key].clone();
        }
        frame_checks.push(check);
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
    bridge.send(json!({"type":"snapshot"}))?;
    let snapshot = receive()?;
    if snapshot["type"] != "snapshot" || snapshot["frame"] != frames {
        return Err("native final checkpoint cursor differs".to_owned());
    }
    let final_checks = checked_hashes(
        &snapshot["checks"],
        &["stateSha256", "snapshotSha256", "saveSha256"],
    )?;
    let save_ids = game
        .catalog()
        .cartridges()
        .values()
        .map(|member| member.manifest.id.as_str())
        .collect::<Vec<_>>();
    let final_saves = checked_hashes(&snapshot["checks"]["saves"], &save_ids)?;
    let snapshot = STANDARD
        .decode(
            snapshot["snapshot"]
                .as_str()
                .ok_or("missing final checkpoint")?,
        )
        .map_err(|error| error.to_string())?;
    let mut snapshot_hash = 0xcbf2_9ce4_8422_2325_u64;
    fingerprint(&mut snapshot_hash, &snapshot);
    bridge.send(json!({"type":"stop"}))?;
    if receive()?["type"] != "stopped" {
        return Err("native stop failed".to_owned());
    }
    Ok(
        json!({"cartridge":game.entry().manifest.id,"gameIdentity":game.identity(),"frames":frames,
            "memberFrames":member_frames,"workPeak":peak,"framebufferFnv64":format!("{pixels_hash:016x}"),
            "audioFnv64":format!("{audio_hash:016x}"),"audioPeak":audio_peak,
            "saveCommitsFnv64":format!("{saves_hash:016x}"),"saveCommitFrames":save_commit_frames,
            "timelineFnv64":format!("{timeline_hash:016x}"),
            "inputFnv64":format!("{input_hash:016x}"), "snapshotFnv64":format!("{snapshot_hash:016x}"),
            "frameChecks":frame_checks, "finalStateSha256":final_checks["stateSha256"],
            "finalSnapshotSha256":final_checks["snapshotSha256"],
            "finalSaveSha256":final_checks["saveSha256"], "finalSaves":final_saves}),
    )
}

fn checked_hashes(value: &Value, fields: &[&str]) -> Result<Value, String> {
    let mut output = json!({});
    for field in fields {
        let hash = value[field]
            .as_str()
            .filter(|hash| {
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
            .ok_or_else(|| format!("missing or invalid native {field} fingerprint"))?;
        output[field] = json!(hash);
    }
    Ok(output)
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
    fn verification_requires_complete_canonical_sha256_evidence() {
        let good = "01ab".repeat(16);
        assert_eq!(
            checked_hashes(&json!({"pixels":good}), &["pixels"]).unwrap(),
            json!({"pixels":good})
        );
        for bad in [
            Value::Null,
            json!(0),
            json!("a".repeat(63)),
            json!("A".repeat(64)),
            json!("g".repeat(64)),
        ] {
            assert!(checked_hashes(&json!({"pixels":bad}), &["pixels"]).is_err());
        }
        assert!(checked_hashes(&json!({}), &["pixels"]).is_err());
    }

    #[test]
    fn native_verification_replays_real_member_transitions_and_rejects_out_of_range_input_before_load()
     {
        let directory = std::env::temp_dir().join(format!(
            "mod01-replay-verify-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("game.m01w");
        let trace = directory.join("run.m01rec");
        let source = crate::studio_game_tests::game_source();
        std::fs::write(&path, &source).unwrap();
        let mut transfer = replay::empty_input();
        transfer["menuAction"] = json!(1);
        let bytes = replay::encode(&[(1, transfer)]).unwrap();
        std::fs::write(&trace, &bytes).unwrap();
        let plain = verify(&path, 4, None).unwrap();
        assert_eq!(plain["memberFrames"], json!({"main":4}));
        let actual = verify(&path, 4, Some(&trace)).unwrap();
        assert_eq!(actual["memberFrames"], json!({"main":2,"arena":2}));
        assert_eq!(actual["saveCommitFrames"], json!({"main":1,"arena":1}));
        for key in [
            "inputFnv64",
            "timelineFnv64",
            "framebufferFnv64",
            "snapshotFnv64",
        ] {
            assert_ne!(actual[key], plain[key]);
        }
        assert_eq!(actual, verify(&path, 4, Some(&trace)).unwrap());
        assert_eq!(std::fs::read(&trace).unwrap(), bytes);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
        assert!(
            verify(&directory.join("not-a-game"), 1, Some(&trace))
                .unwrap_err()
                .contains("outside")
        );
        assert!(verify(&path, 0, None).unwrap_err().contains("1-36000"));
        assert!(verify(&path, 36_001, None).unwrap_err().contains("1-36000"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn audio_status_and_long_titles_leave_the_entire_framebuffer_visible_at_minimum_player_size() {
        for (size, touch) in [
            (egui::vec2(480.0, 320.0), false),
            (egui::vec2(960.0, 650.0), false),
            (egui::vec2(480.0, 500.0), true),
        ] {
            let context = egui::Context::default();
            crate::ui_assets::install_font(&context);
            ui_theme::install(&context);
            let mut app = StudioApp::new_with_devices(None, &context, false);
            app.touch_controller.set_enabled(touch);
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

    #[test]
    fn switching_from_minimum_player_to_source_or_touch_updates_real_viewport_commands_once() {
        let context = egui::Context::default();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../templates/blank/cart.m01w");
        let mut app = StudioApp::new_with_devices(Some(path), &context, false);
        let sync = |app: &mut StudioApp, size| {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| app.sync_viewport_mode(ui.ctx()),
            );
            let commands = output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .clone();
            output.drop_without_applying_deltas();
            commands
        };
        app.player_mode = true;
        let commands = sync(&mut app, egui::vec2(480.0, 320.0));
        assert!(
            commands.contains(&egui::ViewportCommand::MinInnerSize(egui::vec2(
                480.0, 320.0
            )))
        );
        app.active_alias = "main".to_owned();
        app.system_menu_action(crate::pause_menu::Action::Source);
        assert!(!app.player_mode);
        let commands = sync(&mut app, egui::vec2(480.0, 320.0));
        assert!(
            commands.contains(&egui::ViewportCommand::MinInnerSize(egui::vec2(
                900.0, 600.0
            )))
        );
        assert!(commands.contains(&egui::ViewportCommand::InnerSize(egui::vec2(900.0, 600.0))));
        assert!(sync(&mut app, egui::vec2(900.0, 600.0)).is_empty());
        app.player_mode = true;
        app.touch_controller.set_enabled(true);
        let commands = sync(&mut app, egui::vec2(480.0, 320.0));
        assert!(
            commands.contains(&egui::ViewportCommand::MinInnerSize(egui::vec2(
                480.0, 500.0
            )))
        );
        assert!(commands.contains(&egui::ViewportCommand::InnerSize(egui::vec2(480.0, 500.0))));
        assert!(sync(&mut app, egui::vec2(480.0, 500.0)).is_empty());
        app.touch_controller.set_enabled(false);
        let commands = sync(&mut app, egui::vec2(480.0, 500.0));
        assert_eq!(
            commands,
            vec![egui::ViewportCommand::MinInnerSize(egui::vec2(
                480.0, 320.0
            ))]
        );
    }
}
