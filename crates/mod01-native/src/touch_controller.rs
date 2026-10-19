//! Native multi-contact pad feeding the ordinary recorded controller frame, never a second engine.
use crate::{replay, ui_theme};
use eframe::egui::{self, Pos2, Rect, TouchPhase};
use serde_json::Value;
use std::collections::BTreeMap;

const MAX_CONTACTS: usize = 32;
const MENU: u16 = 1 << 11;
const GAME_BUTTONS: u16 = MENU - 1;

#[derive(Clone, Copy)]
enum Zone {
    Dpad,
    Actions,
    Auxiliary(usize),
}

#[derive(Clone, Copy)]
struct Contact {
    zone: Zone,
    position: Pos2,
    pending: u16,
}

#[derive(Clone, Copy, PartialEq)]
struct Layout {
    dpad: Rect,
    actions: [(u16, Rect); 4],
    auxiliary: [(u16, Rect); 4],
}

impl Layout {
    fn new(area: Rect) -> Self {
        let dpad = Rect::from_min_size(area.min + egui::vec2(8.0, 8.0), egui::vec2(144.0, 144.0));
        let actions = Rect::from_min_size(
            egui::pos2(area.right() - 152.0, area.top() + 8.0),
            egui::vec2(144.0, 144.0),
        );
        let square = |center| Rect::from_center_size(center, egui::vec2(44.0, 44.0));
        let cx = area.center().x;
        let cy = area.top();
        Self {
            dpad,
            actions: [
                (1 << 4, square(actions.min + egui::vec2(72.0, 120.0))),
                (1 << 5, square(actions.min + egui::vec2(120.0, 72.0))),
                (1 << 6, square(actions.min + egui::vec2(24.0, 72.0))),
                (1 << 7, square(actions.min + egui::vec2(72.0, 24.0))),
            ],
            auxiliary: [
                (
                    1 << 8,
                    Rect::from_center_size(
                        egui::pos2(cx - 52.0, cy + 42.0),
                        egui::vec2(76.0, 42.0),
                    ),
                ),
                (
                    1 << 9,
                    Rect::from_center_size(
                        egui::pos2(cx + 52.0, cy + 42.0),
                        egui::vec2(76.0, 42.0),
                    ),
                ),
                (
                    1 << 10,
                    Rect::from_center_size(
                        egui::pos2(cx - 52.0, cy + 112.0),
                        egui::vec2(96.0, 44.0),
                    ),
                ),
                (
                    MENU,
                    Rect::from_center_size(
                        egui::pos2(cx + 52.0, cy + 112.0),
                        egui::vec2(96.0, 44.0),
                    ),
                ),
            ],
        }
    }

    fn owner(&self, position: Pos2) -> Option<Zone> {
        if self.dpad.contains(position) {
            Some(Zone::Dpad)
        } else if self.actions.iter().any(|(_, rect)| rect.contains(position)) {
            Some(Zone::Actions)
        } else {
            self.auxiliary
                .iter()
                .position(|(_, rect)| rect.contains(position))
                .map(Zone::Auxiliary)
        }
    }

    fn bits(&self, contact: &Contact) -> u16 {
        let position = contact.position;
        match contact.zone {
            Zone::Dpad if self.dpad.contains(position) => {
                let x = (position.x - self.dpad.left()) / 48.0;
                let y = (position.y - self.dpad.top()) / 48.0;
                u16::from(y < 1.0)
                    | (u16::from(y >= 2.0) << 1)
                    | (u16::from(x < 1.0) << 2)
                    | (u16::from(x >= 2.0) << 3)
            }
            Zone::Actions => self
                .actions
                .iter()
                .filter(|(_, rect)| rect.contains(position))
                .fold(0, |bits, (mask, _)| bits | mask),
            Zone::Auxiliary(index) if self.auxiliary[index].1.contains(position) => {
                self.auxiliary[index].0
            }
            _ => 0,
        }
    }
}

#[derive(Default)]
pub struct TouchController {
    enabled: bool,
    contacts: BTreeMap<(u64, u64), Contact>,
    mouse: Option<Contact>,
    completed_presses: u16,
    menu_requested: bool,
    layout: Option<Layout>,
    sampled_frame: Option<u64>,
}

impl TouchController {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.clear();
            self.layout = None;
        }
        self.enabled = enabled;
    }

    pub fn clear(&mut self) {
        self.contacts.clear();
        self.mouse = None;
        self.completed_presses = 0;
        self.menu_requested = false;
    }

    fn held(&self) -> u16 {
        let Some(layout) = self.layout else { return 0 };
        self.contacts
            .values()
            .chain(self.mouse.iter())
            .fold(0, |bits, contact| bits | layout.bits(contact))
    }

    fn mask(&self) -> u16 {
        (self.held()
            | self.completed_presses
            | self
                .contacts
                .values()
                .chain(self.mouse.iter())
                .fold(0, |bits, contact| bits | contact.pending))
            & GAME_BUTTONS
    }

    pub fn owns_pointer(&self) -> bool {
        !self.contacts.is_empty() || self.mouse.is_some()
    }

    pub fn take_menu_request(&mut self) -> bool {
        std::mem::take(&mut self.menu_requested)
    }

    /// Called only after an input frame is submitted; held contacts remain held.
    pub fn consume(&mut self) {
        self.completed_presses = 0;
        for contact in self.contacts.values_mut().chain(self.mouse.iter_mut()) {
            contact.pending = 0;
        }
    }

    pub fn apply_to(&self, input: &mut Value) {
        let mask = self.mask();
        for (index, name) in replay::BUTTONS.iter().enumerate() {
            if mask & (1 << index) != 0 {
                input["controllers"][0]["buttons"][*name] = true.into();
            }
        }
        if self.owns_pointer() {
            input["pointer"] = replay::empty_input()["pointer"].clone();
        }
    }

    fn contact(layout: Layout, position: Pos2, gameplay: bool) -> Option<Contact> {
        if !position.x.is_finite() || !position.y.is_finite() {
            return None;
        }
        let zone = layout.owner(position)?;
        let mut contact = Contact {
            zone,
            position,
            pending: 0,
        };
        let bits = layout.bits(&contact);
        if !gameplay && bits != MENU {
            return None;
        }
        contact.pending = bits;
        Some(contact)
    }

    fn capture(&mut self, events: &[egui::Event], focused: bool, gameplay: bool) {
        if !self.enabled || !focused {
            self.clear();
            return;
        }
        let Some(layout) = self.layout else { return };
        if !gameplay {
            self.contacts
                .retain(|_, contact| matches!(contact.zone, Zone::Auxiliary(3)));
            self.mouse = self
                .mouse
                .filter(|contact| matches!(contact.zone, Zone::Auxiliary(3)));
            self.completed_presses = 0;
        }
        let has_touch = events
            .iter()
            .any(|event| matches!(event, egui::Event::Touch { .. }));
        if has_touch || !self.contacts.is_empty() {
            self.mouse = None;
        }
        for event in events {
            let before = self.held();
            match *event {
                egui::Event::Touch {
                    device_id,
                    id,
                    phase,
                    pos,
                    ..
                } => {
                    let key = (device_id.0, id.0);
                    match phase {
                        TouchPhase::Start => {
                            self.contacts.remove(&key);
                            if self.contacts.len() < MAX_CONTACTS
                                && let Some(contact) = Self::contact(layout, pos, gameplay)
                            {
                                self.contacts.insert(key, contact);
                            }
                        }
                        TouchPhase::Move => {
                            if !pos.x.is_finite() || !pos.y.is_finite() {
                                self.contacts.remove(&key);
                            } else if let Some(contact) = self.contacts.get_mut(&key) {
                                let previous = layout.bits(contact);
                                contact.position = pos;
                                contact.pending |= layout.bits(contact) & !previous;
                            }
                        }
                        TouchPhase::End => {
                            if let Some(contact) = self.contacts.remove(&key) {
                                self.completed_presses |= contact.pending & GAME_BUTTONS;
                            }
                        }
                        TouchPhase::Cancel => {
                            self.contacts.remove(&key);
                        }
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    ..
                } if !has_touch && self.contacts.is_empty() => {
                    if pressed {
                        self.mouse = Self::contact(layout, pos, gameplay);
                    } else if let Some(contact) = self.mouse.take() {
                        self.completed_presses |= contact.pending & GAME_BUTTONS;
                    }
                }
                egui::Event::PointerMoved(position) if !has_touch && self.contacts.is_empty() => {
                    if let Some(contact) = self.mouse.as_mut() {
                        let previous = layout.bits(contact);
                        contact.position = position;
                        contact.pending |= layout.bits(contact) & !previous;
                    }
                }
                egui::Event::PointerGone => self.mouse = None,
                egui::Event::WindowFocused(false) => self.clear(),
                _ => {}
            }
            self.menu_requested |= self.held() & !before & MENU != 0;
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, gameplay: bool) {
        if !self.enabled {
            self.clear();
            return;
        }
        let (area, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().clamp(360.0, 16384.0), 152.0),
            egui::Sense::hover(),
        );
        let layout = Layout::new(area);
        if self.layout != Some(layout) {
            self.clear();
            self.layout = Some(layout);
        }
        let frame = ui.ctx().cumulative_frame_nr();
        if self.sampled_frame != Some(frame) {
            let (events, focused) = ui.input(|input| (input.events.clone(), input.focused));
            self.capture(&events, focused, gameplay);
            self.sampled_frame = Some(frame);
        }
        let held = self.held();
        let button = |ui: &mut egui::Ui, rect: Rect, mask: u16, label: &str| {
            let active = held & mask != 0;
            let fill = if active {
                ui_theme::CYAN
            } else {
                ui_theme::SCREEN_RAISED
            };
            ui.painter().rect_filled(rect, 1.0, fill);
            ui.painter().rect_stroke(
                rect,
                1.0,
                egui::Stroke::new(1.0, ui_theme::EDGE),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::monospace(13.0),
                if active {
                    ui_theme::SCREEN
                } else {
                    ui_theme::CREAM
                },
            );
            ui.interact(rect, ui.scope_id().with(label), egui::Sense::hover())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
        };
        ui.painter().rect_filled(layout.dpad, 1.0, ui_theme::CASE);
        for (x, y, mask, label) in [
            (1, 0, 1, "UP"),
            (1, 2, 2, "DOWN"),
            (0, 1, 4, "LEFT"),
            (2, 1, 8, "RIGHT"),
        ] {
            let rect = Rect::from_min_size(
                layout.dpad.min + egui::vec2(x as f32 * 48.0 + 2.0, y as f32 * 48.0 + 2.0),
                egui::vec2(44.0, 44.0),
            );
            button(ui, rect, mask, label);
        }
        for ((mask, rect), label) in layout
            .actions
            .iter()
            .zip(["A", "B", "X", "Y"])
            .chain(layout.auxiliary.iter().zip(["L", "R", "START", "MENU"]))
        {
            button(ui, *rect, *mask, label);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(device: u64, id: u64, phase: TouchPhase, pos: Pos2) -> egui::Event {
        egui::Event::Touch {
            device_id: egui::TouchDeviceId(device),
            id: egui::TouchId(id),
            phase,
            pos,
            force: None,
        }
    }

    fn pad() -> (TouchController, Layout) {
        let layout = Layout::new(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 160.0)));
        let mut pad = TouchController::default();
        pad.set_enabled(true);
        pad.layout = Some(layout);
        (pad, layout)
    }

    #[test]
    fn independent_fingers_devices_diagonals_cancel_and_release_do_not_stick() {
        let (mut pad, layout) = pad();
        let left = layout.dpad.min + egui::vec2(24.0, 72.0);
        let action = layout.actions[0].1.center();
        pad.capture(
            &[
                touch(1, 1, TouchPhase::Start, left),
                touch(2, 1, TouchPhase::Start, action),
            ],
            true,
            true,
        );
        assert_eq!(pad.mask(), (1 << 2) | (1 << 4));
        pad.consume();
        pad.capture(
            &[touch(
                1,
                1,
                TouchPhase::Move,
                layout.dpad.min + egui::vec2(24.0, 24.0),
            )],
            true,
            true,
        );
        assert_eq!(pad.mask(), 1 | (1 << 2) | (1 << 4));
        pad.consume();
        pad.capture(&[touch(1, 1, TouchPhase::End, left)], true, true);
        assert_eq!(pad.mask(), 1 << 4);
        pad.capture(&[touch(2, 1, TouchPhase::Cancel, action)], true, true);
        assert_eq!(pad.mask(), 0);
        assert!(!pad.owns_pointer());
        pad.capture(
            &[
                touch(1, 2, TouchPhase::Start, action),
                touch(1, 3, TouchPhase::Start, action),
            ],
            true,
            true,
        );
        pad.consume();
        pad.capture(&[touch(1, 2, TouchPhase::Cancel, action)], true, true);
        assert_eq!(pad.mask(), 1 << 4);
        pad.capture(&[touch(1, 3, TouchPhase::End, action)], true, true);
        assert_eq!(pad.mask(), 0);
    }

    #[test]
    fn short_taps_survive_frame_latency_but_cancelled_taps_do_not() {
        let (mut pad, layout) = pad();
        let action = layout.actions[0].1.center();
        pad.capture(
            &[
                touch(1, 1, TouchPhase::Start, action),
                touch(1, 1, TouchPhase::End, action),
            ],
            true,
            true,
        );
        assert_eq!(pad.mask(), 1 << 4);
        pad.capture(&[], true, true);
        assert_eq!(pad.mask(), 1 << 4);
        pad.consume();
        assert_eq!(pad.mask(), 0);
        pad.capture(
            &[
                touch(1, 2, TouchPhase::Start, action),
                touch(1, 2, TouchPhase::Cancel, action),
            ],
            true,
            true,
        );
        assert_eq!(pad.mask(), 0);
    }

    #[test]
    fn outside_starts_focus_loss_disabled_controls_and_mouse_aliases_cannot_create_ghost_presses() {
        let (mut pad, layout) = pad();
        let action = layout.actions[0].1.center();
        pad.capture(
            &[
                touch(1, 1, TouchPhase::Start, egui::pos2(400.0, 300.0)),
                touch(1, 1, TouchPhase::Move, action),
            ],
            true,
            true,
        );
        assert_eq!(pad.mask(), 0);
        let mouse = egui::Event::PointerButton {
            pos: action,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        };
        pad.capture(&[touch(1, 2, TouchPhase::Start, action), mouse], true, true);
        assert!(pad.mouse.is_none());
        pad.consume();
        pad.capture(&[touch(1, 2, TouchPhase::End, action)], true, true);
        assert_eq!(pad.mask(), 0);
        pad.capture(&[touch(1, 3, TouchPhase::Start, action)], true, true);
        pad.capture(&[], false, true);
        assert_eq!(pad.mask(), 0);
        pad.set_enabled(false);
        pad.capture(&[touch(1, 4, TouchPhase::Start, action)], true, true);
        assert_eq!(pad.mask(), 0);
        pad.set_enabled(true);
        pad.capture(&[touch(1, 4, TouchPhase::Move, action)], true, true);
        assert_eq!(pad.mask(), 0);
    }

    #[test]
    fn menu_is_host_only_and_paused_controls_cannot_queue_gameplay_or_pointer_clicks() {
        let (mut pad, layout) = pad();
        let action = layout.actions[0].1.center();
        let menu = layout.auxiliary[3].1.center();
        pad.capture(
            &[
                touch(1, 1, TouchPhase::Start, action),
                touch(1, 2, TouchPhase::Start, menu),
            ],
            true,
            false,
        );
        assert_eq!(pad.mask(), 0);
        assert!(pad.take_menu_request());
        assert!(!pad.take_menu_request());
        let mut frame = replay::empty_input();
        frame["pointer"]["inside"] = true.into();
        frame["pointer"]["primary"] = true.into();
        pad.apply_to(&mut frame);
        assert_eq!(frame, replay::empty_input());
    }

    fn render(
        pad: &mut TouchController,
        context: &egui::Context,
        events: Vec<egui::Event>,
        width: f32,
    ) {
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 600.0))),
                    events,
                    ..Default::default()
                },
                |ui| pad.show(ui, true),
            )
            .drop_without_applying_deltas();
    }

    #[test]
    fn actual_egui_touch_events_drive_simultaneous_buttons_and_resize_releases_ownership() {
        let context = egui::Context::default();
        crate::ui_assets::install_font(&context);
        crate::ui_theme::install(&context);
        let mut pad = TouchController::default();
        pad.set_enabled(true);
        render(&mut pad, &context, Vec::new(), 900.0);
        let layout = pad.layout.unwrap();
        let left = layout.dpad.min + egui::vec2(24.0, 72.0);
        let action = layout.actions[0].1.center();
        render(
            &mut pad,
            &context,
            vec![
                touch(1, 1, TouchPhase::Start, left),
                touch(1, 2, TouchPhase::Start, action),
            ],
            900.0,
        );
        assert_eq!(pad.mask(), (1 << 2) | (1 << 4));
        assert!(pad.owns_pointer());
        pad.consume();
        render(
            &mut pad,
            &context,
            vec![touch(1, 1, TouchPhase::End, left)],
            900.0,
        );
        assert_eq!(pad.mask(), 1 << 4);
        render(&mut pad, &context, Vec::new(), 1280.0);
        assert_eq!(pad.mask(), 0);
        assert!(!pad.owns_pointer());
        render(
            &mut pad,
            &context,
            vec![touch(1, 2, TouchPhase::Move, action)],
            1280.0,
        );
        assert_eq!(pad.mask(), 0);
    }

    fn receive(bridge: &crate::runtime::RuntimeBridge) -> Value {
        use std::{
            thread,
            time::{Duration, Instant},
        };
        let deadline = Instant::now() + Duration::from_secs(7);
        loop {
            if let Some(event) = bridge.poll() {
                assert_ne!(event["type"], "error", "{event}");
                return event;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn native_touch_chords_and_short_taps_replay_identical_real_modl_pixels_work_and_pcm() {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let mut project =
            modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
        project.manifest.id = "touch-conformance".to_owned();
        project.files.insert("src/main.modl".to_owned(),b"state hits: Int = 0\n\non update:\n  if btnp(pad1, left) and btn(pad1, a):\n    hits += 1\n\non draw:\n  clear(0)\n  if hits == 1:\n    pixel(0, 0, 7)\n  if hits == 2:\n    pixel(0, 0, 9)\n".to_vec());
        let source = modl_core::encode_workcart(
            &project.manifest,
            &project.files,
            &project.recipes,
            &project.tests,
        )
        .unwrap();
        let project = crate::project::NativeProject::from_source(source, None).unwrap();
        let bridge = crate::runtime::RuntimeBridge::new(crate::native_host_path());
        bridge
            .send(project.load_command(false, &[]).unwrap())
            .unwrap();
        assert_eq!(receive(&bridge)["type"], "loaded");
        let context = egui::Context::default();
        crate::ui_assets::install_font(&context);
        crate::ui_theme::install(&context);
        let mut pad = TouchController::default();
        pad.set_enabled(true);
        render(&mut pad, &context, Vec::new(), 900.0);
        let layout = pad.layout.unwrap();
        let left = layout.dpad.min + egui::vec2(24.0, 72.0);
        let action = layout.actions[0].1.center();
        let events = vec![
            vec![
                touch(1, 1, TouchPhase::Start, left),
                touch(1, 2, TouchPhase::Start, action),
            ],
            vec![touch(1, 1, TouchPhase::End, left)],
            vec![touch(1, 2, TouchPhase::Cancel, action)],
            vec![
                touch(1, 3, TouchPhase::Start, left),
                touch(1, 4, TouchPhase::Start, action),
                touch(1, 3, TouchPhase::End, left),
                touch(1, 4, TouchPhase::End, action),
            ],
            Vec::new(),
        ];
        let mut recorded = Vec::new();
        let mut expected = Vec::new();
        for (frame, events) in events.into_iter().enumerate() {
            render(&mut pad, &context, events, 900.0);
            let mut input = replay::empty_input();
            pad.apply_to(&mut input);
            recorded.push((frame as u32, input.clone()));
            bridge
                .send(serde_json::json!({"type":"frame","input":input}))
                .unwrap();
            pad.consume();
            let output = receive(&bridge);
            assert_eq!(
                STANDARD.decode(output["pixels"].as_str().unwrap()).unwrap()[0],
                if frame < 3 { 7 } else { 9 }
            );
            expected.push(output);
        }
        assert_eq!(pad.mask(), 0);
        let playback = replay::decode(&replay::encode(&recorded).unwrap()).unwrap();
        bridge
            .send(project.load_command(false, &[]).unwrap())
            .unwrap();
        assert_eq!(receive(&bridge)["type"], "loaded");
        for (frame, input) in recorded {
            assert_eq!(playback[&frame], input);
            bridge
                .send(serde_json::json!({"type":"frame","input":playback[&frame]}))
                .unwrap();
            let actual = receive(&bridge);
            for field in [
                "frame",
                "pixels",
                "workUnits",
                "left",
                "right",
                "saveCommit",
            ] {
                assert_eq!(
                    actual[field], expected[frame as usize][field],
                    "{field} at {frame}"
                );
            }
        }
        bridge.send(serde_json::json!({"type":"stop"})).unwrap();
        assert_eq!(receive(&bridge)["type"], "stopped");
    }
}
