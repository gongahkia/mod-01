//! UI-event sampling is independent of VM response latency. A short tap survives until one frame.
use crate::replay;
use eframe::egui::{self, Event, Key};
use serde_json::Value;

const KEYS: [Key; 11] = [
    Key::ArrowUp,
    Key::ArrowDown,
    Key::ArrowLeft,
    Key::ArrowRight,
    Key::Z,
    Key::X,
    Key::A,
    Key::S,
    Key::Q,
    Key::W,
    Key::Enter,
];

#[derive(Default)]
pub struct KeyboardController {
    last_ui_frame: Option<u64>,
    held: u16,
    pending: u16,
    blocked: u16,
}

impl KeyboardController {
    pub fn capture(&mut self, context: &egui::Context, enabled: bool) {
        let frame = context.cumulative_frame_nr();
        let enabled = enabled && !context.text_edit_focused();
        if self.last_ui_frame == Some(frame) {
            if !enabled {
                self.quarantine();
            }
            return;
        }
        self.last_ui_frame = Some(frame);
        context.input(|input| {
            let mask = KEYS.iter().enumerate().fold(0, |bits, (index, key)| {
                bits | (u16::from(input.key_down(*key)) << index)
            });
            let enabled = enabled
                && input.focused
                && !input.modifiers.ctrl
                && !input.modifiers.command
                && !input.modifiers.alt
                && !input
                    .events
                    .iter()
                    .any(|event| matches!(event, Event::WindowFocused(false)));
            if !enabled {
                self.held = mask;
                self.blocked = mask;
                self.pending = 0;
                return;
            }
            for event in &input.events {
                if let Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } = event
                    && let Some(index) = KEYS.iter().position(|binding| binding == key)
                {
                    let bit = 1 << index;
                    if *pressed {
                        if modifiers.ctrl || modifiers.command || modifiers.alt {
                            self.blocked |= bit;
                            self.pending &= !bit;
                        } else if !repeat && self.held & bit == 0 && self.blocked & bit == 0 {
                            self.pending |= bit;
                        }
                        self.held |= bit;
                    } else {
                        self.held &= !bit;
                        self.blocked &= !bit;
                    }
                }
            }
            // Held keys without a fresh event (focus/session changes) cannot synthesize a press.
            self.blocked = (self.blocked | (mask & !self.held)) & mask;
            self.held = mask;
        });
    }

    pub fn apply_to(&self, input: &mut Value) {
        let bits = (self.held & !self.blocked) | self.pending;
        for (index, name) in replay::BUTTONS.iter().take(KEYS.len()).enumerate() {
            if bits & (1 << index) != 0 {
                input["controllers"][0]["buttons"][*name] = true.into();
            }
        }
    }

    pub fn consume(&mut self) {
        self.pending = 0;
    }

    /// Pause/focus/text/session interruptions discard taps and require held keys to be released.
    pub fn quarantine(&mut self) {
        self.pending = 0;
        self.blocked |= self.held;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, pressed: bool, modifiers: egui::Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        }
    }

    fn pass(
        context: &egui::Context,
        keys: &mut KeyboardController,
        events: Vec<Event>,
        enabled: bool,
    ) {
        context
            .run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    keys.capture(ui.ctx(), enabled);
                },
            )
            .drop_without_applying_deltas();
    }

    fn pressed(keys: &KeyboardController, button: &str) -> bool {
        let mut input = replay::empty_input();
        keys.apply_to(&mut input);
        assert_eq!(input["controllers"][0]["buttons"]["menu"], false);
        input["controllers"][0]["buttons"][button] == true
    }

    #[test]
    fn short_taps_survive_delayed_vm_frames_and_are_consumed_once() {
        let context = egui::Context::default();
        let mut keys = KeyboardController::default();
        pass(
            &context,
            &mut keys,
            vec![
                key(Key::Z, true, egui::Modifiers::NONE),
                key(Key::Z, false, egui::Modifiers::NONE),
            ],
            true,
        );
        for _ in 0..4 {
            pass(&context, &mut keys, vec![], true);
        }
        assert!(pressed(&keys, "a"));
        keys.consume();
        assert!(!pressed(&keys, "a"));
        pass(
            &context,
            &mut keys,
            vec![key(Key::ArrowUp, true, egui::Modifiers::NONE)],
            true,
        );
        keys.consume();
        assert!(pressed(&keys, "up"));
        pass(
            &context,
            &mut keys,
            vec![key(Key::ArrowUp, false, egui::Modifiers::NONE)],
            true,
        );
        assert!(!pressed(&keys, "up"));
    }

    #[test]
    fn paused_held_shortcuts_and_focus_loss_require_a_fresh_press_and_escape_is_host_only() {
        let context = egui::Context::default();
        let mut keys = KeyboardController::default();
        pass(
            &context,
            &mut keys,
            vec![key(Key::Z, true, egui::Modifiers::NONE)],
            true,
        );
        keys.quarantine();
        pass(&context, &mut keys, vec![], true);
        assert!(!pressed(&keys, "a"));
        pass(
            &context,
            &mut keys,
            vec![
                key(Key::Z, false, egui::Modifiers::NONE),
                key(Key::Z, true, egui::Modifiers::NONE),
            ],
            true,
        );
        assert!(pressed(&keys, "a"));
        pass(&context, &mut keys, vec![Event::WindowFocused(false)], true);
        assert!(!pressed(&keys, "a"));
        pass(
            &context,
            &mut keys,
            vec![
                Event::WindowFocused(true),
                key(Key::Z, false, egui::Modifiers::NONE),
                key(Key::X, true, egui::Modifiers::CTRL),
                key(Key::X, false, egui::Modifiers::CTRL),
                key(Key::Escape, true, egui::Modifiers::NONE),
            ],
            true,
        );
        assert!(!pressed(&keys, "b"));
        pass(
            &context,
            &mut keys,
            vec![key(Key::S, true, egui::Modifiers::NONE)],
            false,
        );
        pass(&context, &mut keys, vec![], true);
        assert!(!pressed(&keys, "y"));
    }

    #[test]
    fn actual_text_focus_blocks_game_keys_and_resampling_one_ui_pass_cannot_duplicate_a_tap() {
        let context = egui::Context::default();
        let mut keys = KeyboardController::default();
        let mut text = String::new();
        context
            .run_ui(egui::RawInput::default(), |ui| {
                ui.text_edit_singleline(&mut text).request_focus();
            })
            .drop_without_applying_deltas();
        context
            .run_ui(
                egui::RawInput {
                    events: vec![
                        key(Key::Z, true, egui::Modifiers::NONE),
                        key(Key::Z, false, egui::Modifiers::NONE),
                    ],
                    ..Default::default()
                },
                |ui| {
                    ui.text_edit_singleline(&mut text);
                    assert!(ui.ctx().text_edit_focused());
                    keys.capture(ui.ctx(), true);
                },
            )
            .drop_without_applying_deltas();
        assert!(!pressed(&keys, "a"));
        context.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
        context
            .run_ui(
                egui::RawInput {
                    events: vec![
                        key(Key::X, true, egui::Modifiers::NONE),
                        key(Key::X, false, egui::Modifiers::NONE),
                    ],
                    ..Default::default()
                },
                |ui| {
                    keys.capture(ui.ctx(), true);
                    assert!(pressed(&keys, "b"));
                    keys.consume();
                    keys.capture(ui.ctx(), true);
                    assert!(!pressed(&keys, "b"));
                },
            )
            .drop_without_applying_deltas();
    }
}
