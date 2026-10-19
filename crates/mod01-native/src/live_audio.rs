//! Clocked, isolated native preview of the production synth/tracker, never a WAV simulation.
use crate::{audio::AudioOutput, native_host_path, project::NativeProject, runtime::RuntimeBridge};
use eframe::egui;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const PERIOD: Duration = Duration::from_nanos(16_666_667);
const QUEUED_CONTROLS: usize = 16;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Idle,
    Loading,
    Ready,
    Control,
    Frame,
    Closing,
    Stopping,
}

struct Session {
    assets: Vec<Value>,
    command: Value,
}

pub struct AudioPreview {
    runtime: RuntimeBridge,
    phase: Phase,
    next_session: Option<Session>,
    controls: VecDeque<Value>,
    assets: Vec<Value>,
    ticking: bool,
    next_tick: Instant,
    clear_audio: bool,
    pub packet: Value,
    pub status: String,
}

impl AudioPreview {
    pub fn new() -> Self {
        Self {
            runtime: RuntimeBridge::new(native_host_path()),
            phase: Phase::Idle,
            next_session: None,
            controls: VecDeque::new(),
            assets: Vec::new(),
            ticking: false,
            next_tick: Instant::now(),
            clear_audio: false,
            packet: Value::Null,
            status: "LIVE SYNTH / STOPPED".to_owned(),
        }
    }

    pub fn active(&self) -> bool {
        self.phase != Phase::Idle
    }

    pub fn play(&mut self, assets: Vec<Value>, music: &str, order: usize, row: usize) {
        self.start(Session {assets, command: json!({"type":"audio-preview-play", "music":music, "orderIndex":order, "row":row})});
    }

    pub fn audition(&mut self, assets: Vec<Value>, sound: &str, note: f64, volume: f64) {
        let command =
            json!({"type":"audio-preview-audition", "sound":sound, "note":note, "volume":volume});
        if matches!(self.phase, Phase::Ready | Phase::Control | Phase::Frame)
            && sound_bank(&self.assets) == sound_bank(&assets)
        {
            if self.controls.len() < QUEUED_CONTROLS {
                self.controls.push_back(command);
                self.ticking = true;
            } else {
                self.status = "AUDITION QUEUE FULL / TRY AGAIN".to_owned();
            }
        } else {
            self.start(Session { assets, command });
        }
    }

    fn start(&mut self, session: Session) {
        self.next_session = Some(session);
        self.controls.clear();
        self.clear_audio = true;
        if self.phase == Phase::Idle {
            self.load_next();
        } else {
            self.close(Phase::Stopping);
        }
    }

    fn load_next(&mut self) {
        let Some(session) = self.next_session.take() else {
            return;
        };
        self.assets = session.assets;
        self.controls.push_back(session.command);
        self.packet = Value::Null;
        self.ticking = false;
        self.next_tick = Instant::now();
        self.status = "LOADING ISOLATED PRODUCTION SYNTH".to_owned();
        if self.send(json!({"type":"audio-preview-load", "assets":self.assets})) {
            self.phase = Phase::Loading;
        }
    }

    pub fn stop(&mut self) {
        self.next_session = None;
        self.controls.clear();
        if matches!(self.phase, Phase::Stopping | Phase::Closing) {
            return;
        }
        self.clear_audio = true;
        self.ticking = false;
        if self.phase != Phase::Idle {
            self.close(Phase::Stopping);
        } else {
            self.status = "LIVE SYNTH / STOPPED".to_owned();
        }
    }

    fn close(&mut self, phase: Phase) {
        if self.phase == Phase::Stopping || self.phase == Phase::Closing {
            return;
        }
        if self.send(json!({"type":"stop"})) {
            self.phase = phase;
        }
    }

    fn send(&mut self, command: Value) -> bool {
        match self.runtime.send(command) {
            Ok(()) => true,
            Err(error) => {
                self.failed(error);
                false
            }
        }
    }

    fn failed(&mut self, error: String) {
        let _ = self.runtime.send(json!({"type":"stop"}));
        self.phase = Phase::Idle;
        self.controls.clear();
        self.next_session = None;
        self.assets.clear();
        self.ticking = false;
        self.clear_audio = true;
        self.status = format!("AUDIO PREVIEW ERROR: {error}");
    }

    pub fn poll(&mut self, context: &egui::Context, audio: Option<&AudioOutput>) {
        self.poll_at(context, audio, Instant::now());
    }

    fn poll_at(&mut self, context: &egui::Context, audio: Option<&AudioOutput>, now: Instant) {
        self.flush_audio_clear(audio);
        while let Some(event) = self.runtime.poll() {
            match event["type"].as_str() {
                Some("audio-preview-loaded") if self.phase == Phase::Loading => {
                    self.phase = Phase::Ready
                }
                Some("audio-preview-playing" | "audio-preview-auditioned")
                    if self.phase == Phase::Control =>
                {
                    self.phase = Phase::Ready;
                    self.ticking = true;
                }
                Some("audio-preview-frame") if self.phase == Phase::Frame => {
                    if let (Some(audio), Some(left), Some(right)) =
                        (audio, event["left"].as_str(), event["right"].as_str())
                    {
                        if let Err(error) = audio.push(left, right) {
                            self.failed(error);
                            continue;
                        }
                    }
                    self.ticking = !event["nextPosition"].is_null()
                        || event["activeVoices"].as_u64().unwrap_or(0) > 0;
                    self.packet = event;
                    self.phase = Phase::Ready;
                    self.status = if audio.is_some() {
                        "LIVE SYNTH / PLAYING"
                    } else {
                        "LIVE SYNTH / SILENT PREVIEW (NO OUTPUT DEVICE)"
                    }
                    .to_owned();
                }
                Some("stopped") if matches!(self.phase, Phase::Stopping | Phase::Closing) => {
                    self.phase = Phase::Idle;
                    self.assets.clear();
                    self.status = "LIVE SYNTH / COMPLETE OR STOPPED".to_owned();
                    self.load_next();
                }
                Some("error") if self.phase == Phase::Stopping => {}
                Some("error") => self.failed(
                    event["message"]
                        .as_str()
                        .unwrap_or("native audio preview failed")
                        .to_owned(),
                ),
                _ => {}
            }
        }
        if self.phase == Phase::Ready {
            if let Some(command) = self.controls.pop_front() {
                if self.send(command) {
                    self.phase = Phase::Control;
                }
            } else if self.ticking && now >= self.next_tick {
                if self.send(json!({"type":"audio-preview-frame"})) {
                    self.phase = Phase::Frame;
                    self.next_tick = now + PERIOD;
                }
            } else if !self.ticking {
                self.close(Phase::Closing);
            }
        }
        self.flush_audio_clear(audio);
        if self.active() {
            context.request_repaint_after(Duration::from_millis(4));
        }
    }

    fn flush_audio_clear(&mut self, audio: Option<&AudioOutput>) {
        if self.clear_audio {
            if let Some(audio) = audio {
                audio.clear();
            }
            self.clear_audio = false;
        }
    }

    pub fn telemetry_ui(&self, ui: &mut egui::Ui) {
        ui.label(&self.status);
        if self.packet.is_null() {
            return;
        }
        let position = &self.packet["position"];
        ui.label(format!(
            "SYNTH FRAME {} / ORDER {} / ROW {} / TICK {}",
            self.packet["frame"], position["orderIndex"], position["row"], position["frameInRow"]
        ));
        ui.horizontal_wrapped(|ui| {
            for slot in 0..8 {
                let voice = &self.packet["voices"][slot];
                ui.colored_label(
                    if voice["active"] == true {
                        crate::ui_theme::CYAN
                    } else {
                        crate::ui_theme::MUTED
                    },
                    format!(
                        "V{} {}",
                        slot + 1,
                        if voice["active"] == true {
                            voice["note"].to_string()
                        } else {
                            "--".to_owned()
                        }
                    ),
                );
            }
        });
        ui.horizontal(|ui| {
            for (label, key) in [("L", "peakLeft"), ("R", "peakRight")] {
                ui.label(label);
                ui.add(
                    egui::ProgressBar::new(self.packet[key].as_f64().unwrap_or(0.0) as f32)
                        .desired_width(100.0),
                );
            }
        });
    }
}

fn sound_bank(assets: &[Value]) -> Vec<&Value> {
    assets
        .iter()
        .filter(|asset| asset["kind"] == "sound")
        .collect()
}

/// Build a read-only preview bank, including the currently unsaved asset buffer.
pub fn project_bank(
    project: &NativeProject,
    selected_name: &str,
    selected: &Value,
    pattern_only: Option<&str>,
) -> Result<Vec<Value>, String> {
    let declaration = project
        .manifest
        .assets
        .get(selected_name)
        .ok_or("no audio asset is selected")?;
    let expected = match declaration.kind {
        modl_core::AssetKind::Music => "music",
        modl_core::AssetKind::Sound => "sound",
        _ => return Err("select a sound or music asset".to_owned()),
    };
    if selected["kind"].as_str() != Some(expected) {
        return Err(format!("selected asset must be a {expected} object"));
    }
    let mut bank = Vec::new();
    for (name, asset) in &project.manifest.assets {
        if asset.kind != modl_core::AssetKind::Sound {
            continue;
        }
        let mut data: Value = if name == selected_name {
            selected.clone()
        } else {
            serde_json::from_slice(
                project
                    .files
                    .get(&asset.path)
                    .ok_or_else(|| format!("missing sound file {}", asset.path))?,
            )
            .map_err(|error| format!("{}: {error}", asset.path))?
        };
        data.as_object_mut()
            .ok_or_else(|| format!("{} must contain a sound object", asset.path))?
            .insert("name".to_owned(), json!(name));
        bank.push(data);
    }
    if expected == "music" {
        let mut music = selected.clone();
        music["name"] = json!(selected_name);
        if let Some(pattern) = pattern_only {
            if !music["patterns"][pattern]["rows"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty())
            {
                return Err("select an existing non-empty pattern".to_owned());
            }
            music["order"] = json!([pattern]);
            music["loop"] = json!(false);
        }
        bank.push(music);
    } else if pattern_only.is_some() {
        return Err("a sound has no tracker patterns".to_owned());
    }
    Ok(bank)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn assets() -> Vec<Value> {
        vec![
            json!({"kind":"sound","name":"tone","waveform":"triangle", "note":60,
        "durationFrames":4,"volume":0.5,"pan":0,
        "envelope":{"attackFrames":0,"decayFrames":0,"sustainLevel":1,"releaseFrames":1},
        "pitch":{"slideSemitonesPerFrame":0,"vibratoDepthSemitones":0,"vibratoPeriodFrames":0}}),
        ]
    }

    #[test]
    fn preview_bank_copies_unsaved_music_and_keeps_authored_assets_and_project_dirty_state_unchanged()
     {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/api-tour/cart.m01w");
        let project = NativeProject::open(&path).unwrap();
        let before = (
            project.files.clone(),
            project.manifest_source.clone(),
            project.dirty,
        );
        let mut selected: Value =
            serde_json::from_slice(&project.files["assets/loop.m01t"]).unwrap();
        selected["framesPerRow"] = json!(240);
        let bank = project_bank(&project, "loop", &selected, Some("A")).unwrap();
        let song = bank.iter().find(|asset| asset["name"] == "loop").unwrap();
        assert_eq!(song["framesPerRow"], 240);
        assert_eq!(song["order"], json!(["A"]));
        assert_eq!(song["loop"], false);
        assert_eq!(
            (project.files, project.manifest_source, project.dirty),
            before
        );
        assert_eq!(selected["loop"], true);
    }

    fn wait(
        preview: &mut AudioPreview,
        context: &egui::Context,
        now: Instant,
        done: impl Fn(&AudioPreview) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(7);
        while !done(preview) {
            preview.poll_at(context, None, now);
            assert!(Instant::now() < deadline, "{}", preview.status);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn repaint_frequency_does_not_advance_the_synth_clock_and_stop_discards_pending_audio() {
        let context = egui::Context::default();
        let mut preview = AudioPreview::new();
        let now = Instant::now() + Duration::from_secs(1);
        preview.audition(assets(), "tone", 64.0, 0.5);
        wait(&mut preview, &context, now, |preview| {
            preview.packet["frame"] == 0
        });
        for _ in 0..20 {
            preview.poll_at(&context, None, now);
        }
        assert_eq!(preview.packet["frame"], 0);
        wait(&mut preview, &context, now + PERIOD, |preview| {
            preview.packet["frame"] == 1
        });
        preview.stop();
        wait(&mut preview, &context, now + PERIOD, |preview| {
            !preview.active()
        });
        preview.audition(assets(), "tone", 67.0, 0.25);
        wait(&mut preview, &context, now + PERIOD * 2, |preview| {
            preview.packet["frame"] == 0
        });
        assert_eq!(preview.packet["voices"][0]["note"], 67);
        preview.stop();
        wait(&mut preview, &context, now + PERIOD * 2, |preview| {
            !preview.active()
        });
    }

    #[test]
    fn replacing_a_loading_session_cannot_play_its_stale_audition() {
        let context = egui::Context::default();
        let mut preview = AudioPreview::new();
        let now = Instant::now() + Duration::from_secs(1);
        preview.audition(assets(), "tone", 60.0, 1.0);
        preview.audition(assets(), "tone", 72.0, 1.0);
        wait(&mut preview, &context, now, |preview| {
            preview.packet["frame"] == 0
        });
        assert_eq!(preview.packet["voices"][0]["note"], 72);
        preview.stop();
        wait(&mut preview, &context, now, |preview| !preview.active());
    }
}
