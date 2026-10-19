//! Event-latched four-port input with stable slots and host-only menu/navigation edges.
use crate::replay;
use eframe::egui;
use gilrs::{Axis, Button, EventType, Gamepad, Gilrs};
use serde_json::Value;

#[derive(Clone, Copy, Default)]
pub struct Navigation {
    pub up: bool,
    pub down: bool,
    pub confirm: bool,
    pub cancel: bool,
    pub menu: bool,
    pub inspect: bool,
    pub favourite: bool,
}

#[derive(Clone, Copy)]
struct Pad {
    id: usize,
    held: u16,
    pending: u16,
    blocked: u16,
}

#[derive(Default)]
pub struct GamepadController {
    slots: [Option<Pad>; 4],
    navigation: Navigation,
    last_ui_frame: Option<u64>,
}

impl GamepadController {
    pub fn capture(&mut self, context: &egui::Context, devices: &mut Option<Gilrs>, enabled: bool) {
        let focused = context.input(|input| {
            input.focused
                && !input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::WindowFocused(false)))
        });
        let enabled = enabled && focused;
        if self.last_ui_frame == Some(context.cumulative_frame_nr()) {
            if !enabled {
                self.quarantine();
            }
            return;
        }
        self.last_ui_frame = Some(context.cumulative_frame_nr());
        self.navigation = Navigation::default();
        let Some(devices) = devices else {
            return;
        };
        while let Some(event) = devices.next_event() {
            let id = usize::from(event.id);
            if matches!(event.event, EventType::Disconnected) {
                self.disconnect(id);
            } else if !matches!(event.event, EventType::Dropped) {
                let gamepad = devices.gamepad(event.id);
                if gamepad.is_connected() {
                    self.observe(id, mask(&gamepad), enabled, focused);
                }
            }
        }
        let connected = devices
            .gamepads()
            .filter(|(_, pad)| pad.is_connected())
            .map(|(id, pad)| {
                let id = usize::from(id);
                self.observe(id, mask(&pad), enabled, focused);
                id
            })
            .collect::<Vec<_>>();
        for slot in &mut self.slots {
            if slot.is_some_and(|pad| !connected.contains(&pad.id)) {
                *slot = None;
            }
        }
    }

    fn observe(&mut self, id: usize, held: u16, enabled: bool, navigation: bool) {
        let index = self
            .slots
            .iter()
            .position(|pad| pad.is_some_and(|pad| pad.id == id))
            .or_else(|| self.slots.iter().position(Option::is_none));
        let Some(index) = index else {
            return;
        };
        let Some(pad) = &mut self.slots[index] else {
            self.slots[index] = Some(Pad {
                id,
                held,
                pending: 0,
                blocked: held,
            });
            return;
        };
        let rising = held & !pad.held;
        if navigation {
            self.navigation.up |= rising & 1 != 0;
            self.navigation.down |= rising & 2 != 0;
            self.navigation.confirm |= rising & ((1 << 4) | (1 << 10)) != 0;
            self.navigation.cancel |= rising & (1 << 5) != 0;
            self.navigation.menu |= rising & (1 << 11) != 0;
            self.navigation.inspect |= rising & (1 << 6) != 0;
            self.navigation.favourite |= rising & (1 << 7) != 0;
        }
        pad.blocked &= held;
        if enabled {
            pad.pending |= rising & !pad.blocked;
        } else {
            pad.pending = 0;
            pad.blocked = held;
        }
        pad.held = held;
    }

    fn disconnect(&mut self, id: usize) {
        for slot in &mut self.slots {
            if slot.is_some_and(|pad| pad.id == id) {
                *slot = None;
            }
        }
    }

    pub fn apply_to(&self, input: &mut Value) {
        for (port, pad) in self.slots.iter().enumerate() {
            if let Some(pad) = pad {
                let bits = (pad.held & !pad.blocked) | pad.pending;
                for (index, name) in replay::BUTTONS.iter().take(11).enumerate() {
                    if bits & (1 << index) != 0 {
                        input["controllers"][port]["buttons"][*name] = true.into();
                    }
                }
            }
        }
    }

    pub fn consume(&mut self) {
        for pad in self.slots.iter_mut().flatten() {
            pad.pending = 0;
        }
    }
    pub fn quarantine(&mut self) {
        for pad in self.slots.iter_mut().flatten() {
            pad.pending = 0;
            pad.blocked |= pad.held;
        }
    }
    pub fn take_navigation(&mut self) -> Navigation {
        std::mem::take(&mut self.navigation)
    }
}

fn mask(pad: &Gamepad<'_>) -> u16 {
    [
        pad.is_pressed(Button::DPadUp) || pad.value(Axis::LeftStickY) > 0.5,
        pad.is_pressed(Button::DPadDown) || pad.value(Axis::LeftStickY) < -0.5,
        pad.is_pressed(Button::DPadLeft) || pad.value(Axis::LeftStickX) < -0.5,
        pad.is_pressed(Button::DPadRight) || pad.value(Axis::LeftStickX) > 0.5,
        pad.is_pressed(Button::South),
        pad.is_pressed(Button::East),
        pad.is_pressed(Button::West),
        pad.is_pressed(Button::North),
        pad.is_pressed(Button::LeftTrigger),
        pad.is_pressed(Button::RightTrigger),
        pad.is_pressed(Button::Start),
        pad.is_pressed(Button::Select),
    ]
    .iter()
    .enumerate()
    .fold(0, |bits, (index, pressed)| {
        bits | (u16::from(*pressed) << index)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_slots_disconnect_and_quarantine_preserve_other_ports_without_menu_leaks() {
        let mut pads = GamepadController::default();
        pads.observe(40, 0, true, true);
        pads.observe(80, 0, true, true);
        pads.observe(40, 1 << 4, true, true);
        pads.observe(40, 0, true, true);
        pads.observe(80, (1 << 1) | (1 << 11), true, true);
        let mut input = replay::empty_input();
        pads.apply_to(&mut input);
        assert_eq!(input["controllers"][0]["buttons"]["a"], true);
        assert_eq!(input["controllers"][1]["buttons"]["down"], true);
        assert_eq!(input["controllers"][1]["buttons"]["menu"], false);
        let navigation = pads.take_navigation();
        assert!(navigation.menu && navigation.confirm && navigation.down);
        assert!(!pads.take_navigation().menu);
        pads.consume();
        pads.disconnect(40);
        pads.observe(90, 0, true, true);
        pads.observe(90, 1 << 6, true, true);
        pads.quarantine();
        pads.observe(80, 1 << 1, true, true);
        let mut input = replay::empty_input();
        pads.apply_to(&mut input);
        assert_eq!(input, replay::empty_input());
        pads.observe(80, 0, true, true);
        pads.observe(80, 1 << 1, true, true);
        pads.apply_to(&mut input);
        assert_eq!(input["controllers"][1]["buttons"]["down"], true);
        assert_eq!(input["controllers"][0]["buttons"]["down"], false);
    }
}
