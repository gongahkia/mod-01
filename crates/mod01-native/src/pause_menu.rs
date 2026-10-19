//! One host system menu for Studio and packaged games. No cartridge execution in UI callbacks.
use crate::{gamepad_controller::Navigation, ui_theme};
use eframe::egui::{self, Key, RichText};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    slot: u8,
    label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Resume,
    Cartridge(u8),
    Restart,
    Source,
}

#[derive(Default)]
pub struct PauseMenu {
    items: Vec<Item>,
    selected: usize,
    armed: bool,
    #[cfg(test)]
    rows: Vec<egui::Rect>,
}

impl PauseMenu {
    pub fn update_items(&mut self, value: &Value) -> Result<(), String> {
        let mut items: Vec<Item> =
            serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        if items.len() > 5
            || items.iter().any(|item| {
                !(1..=5).contains(&item.slot)
                    || item.label.is_empty()
                    || item.label.len() > 24
                    || !item.label.bytes().all(|byte| (32..=126).contains(&byte))
            })
        {
            return Err("INVALID RUNTIME MENU ITEMS".to_owned());
        }
        items.sort_by_key(|item| item.slot);
        if items.windows(2).any(|pair| pair[0].slot == pair[1].slot) {
            return Err("DUPLICATE RUNTIME MENU SLOT".to_owned());
        }
        let selected = self.entries().get(self.selected).map(|(action, _)| *action);
        self.items = items;
        self.selected = self
            .entries()
            .iter()
            .position(|(action, _)| Some(*action) == selected)
            .unwrap_or(0);
        Ok(())
    }

    pub fn has_action(&self, slot: u8) -> bool {
        self.items.iter().any(|item| item.slot == slot)
    }
    pub fn reset_selection(&mut self) {
        self.selected = 0;
        self.armed = false;
    }

    fn entries(&self) -> Vec<(Action, String)> {
        let mut entries = vec![(Action::Resume, "RESUME".to_owned())];
        entries.extend(
            self.items
                .iter()
                .map(|item| (Action::Cartridge(item.slot), item.label.clone())),
        );
        entries.push((Action::Restart, "RESTART CARTRIDGE".to_owned()));
        entries.push((Action::Source, "INSPECT SOURCE".to_owned()));
        entries
    }

    pub fn show(
        &mut self,
        context: &egui::Context,
        title: &str,
        actions_enabled: bool,
        reason: &str,
        mut navigation: Navigation,
    ) -> Option<Action> {
        context.input_mut(|input| {
            navigation.up |= input.consume_key(egui::Modifiers::NONE, Key::ArrowUp);
            navigation.down |= input.consume_key(egui::Modifiers::NONE, Key::ArrowDown);
            navigation.confirm |= input.consume_key(egui::Modifiers::NONE, Key::Enter)
                | input.consume_key(egui::Modifiers::NONE, Key::Z);
            navigation.cancel |= input.consume_key(egui::Modifiers::NONE, Key::X);
        });
        let just_opened = !self.armed;
        self.armed = true;
        if just_opened {
            navigation = Navigation::default();
        }
        let entries = self.entries();
        let enabled = |action| actions_enabled || !matches!(action, Action::Cartridge(_));
        self.selected = self.selected.min(entries.len() - 1);
        if navigation.up != navigation.down {
            for _ in 0..entries.len() {
                self.selected = if navigation.down {
                    (self.selected + 1) % entries.len()
                } else {
                    (self.selected + entries.len() - 1) % entries.len()
                };
                if enabled(entries[self.selected].0) {
                    break;
                }
            }
        }
        let mut action = if navigation.cancel {
            Some(Action::Resume)
        } else if navigation.confirm && enabled(entries[self.selected].0) {
            Some(entries[self.selected].0)
        } else {
            None
        };
        #[cfg(test)]
        self.rows.clear();
        let response = egui::Modal::new(egui::Id::unique("mod01-system-menu"))
            .frame(ui_theme::screen_frame())
            .show(context, |ui| {
                ui.set_width(360.0);
                ui_theme::legend(ui, "MOD-01 / SYSTEM MENU");
                ui.heading(RichText::new("PAUSED").color(ui_theme::CYAN));
                ui.add(egui::Label::new(title).truncate())
                    .on_hover_text(title);
                ui.separator();
                for (index, (item_action, label)) in entries.iter().enumerate() {
                    let response = ui.add_enabled(
                        enabled(*item_action),
                        egui::Button::new(label)
                            .selected(self.selected == index)
                            .min_size(egui::vec2(360.0, 32.0)),
                    );
                    #[cfg(test)]
                    self.rows.push(response.rect);
                    if response.hovered()
                        && ui.input(|input| input.pointer.delta() != egui::Vec2::ZERO)
                    {
                        self.selected = index;
                    }
                    if response.clicked() && !just_opened {
                        action = Some(*item_action);
                    }
                    if !enabled(*item_action) {
                        response.on_disabled_hover_text(reason);
                    }
                }
                if !actions_enabled && !self.items.is_empty() {
                    ui.label(RichText::new(reason).small().color(ui_theme::AMBER));
                }
                ui.separator();
                ui.label("UP/DOWN / Z OR ENTER / X OR ESC BACK");
                ui.label("CONTROLLER: D-PAD / A / B / SELECT MENU");
            });
        if response.should_close() && !just_opened {
            action = Some(Action::Resume);
        }
        action
    }
}

impl crate::StudioApp {
    pub(super) fn handle_system_menu_toggle(&mut self, context: &egui::Context) {
        if self.show_library && !self.player_mode {
            self.pause_navigation = Navigation::default();
            return;
        }
        let navigation = self.gamepad_controller.take_navigation();
        self.pause_navigation = if self.player_paused {
            navigation
        } else {
            Navigation::default()
        };
        let escape = self.running
            && (self.player_mode || self.view == crate::View::Player)
            && (self.player_paused || !context.text_edit_focused())
            && context.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape));
        if self.running && (escape || navigation.menu) {
            self.set_player_paused(!self.player_paused);
            self.pause_navigation = Navigation::default();
        }
    }

    fn menu_action_readiness(&self) -> (bool, &'static str) {
        if !self.loaded || self.pending {
            (false, "WAITING FOR RUNTIME BOUNDARY")
        } else if self.replay_playback.is_some() {
            (false, "CARTRIDGE ACTIONS ARE READ-ONLY DURING REPLAY")
        } else if self.debugging && !self.debug_frame_boundary {
            (
                false,
                "FINISH THE CURRENT DEBUG FRAME BEFORE SELECTING AN ACTION",
            )
        } else {
            (true, "")
        }
    }

    pub(super) fn system_menu_action(&mut self, action: Action) {
        match action {
            Action::Resume => self.set_player_paused(false),
            Action::Restart => self.run_mode(self.debugging),
            Action::Source => {
                self.stop();
                self.player_mode = false;
                self.view = crate::View::Code;
                self.status = "SOURCE INSPECTION / STOPPED".to_owned();
            }
            Action::Cartridge(slot) => {
                if !self.player_paused
                    || !self.menu_action_readiness().0
                    || !self.pause_menu.has_action(slot)
                {
                    return;
                }
                self.pending_menu_action = Some(slot);
                self.set_player_paused(false);
                if self.debugging && !self.debug_running {
                    self.status = "MENU ACTION QUEUED / STEP OR CONTINUE".to_owned();
                }
                self.last_tick = std::time::Instant::now()
                    .checked_sub(std::time::Duration::from_millis(17))
                    .unwrap_or_else(std::time::Instant::now);
            }
        }
    }

    pub(super) fn system_menu_ui(&mut self, context: &egui::Context) {
        if !self.running || !self.player_paused {
            return;
        }
        let (enabled, reason) = self.menu_action_readiness();
        let title = self.runtime_title();
        if let Some(action) = self.pause_menu.show(
            context,
            &title,
            enabled,
            reason,
            std::mem::take(&mut self.pause_navigation),
        ) {
            self.system_menu_action(action);
        }
    }

    pub(super) fn advance_runtime(&mut self, context: &egui::Context) {
        self.capture_keyboard(context);
        if self.player_paused {
            return;
        }
        if self.debugging && self.debug_running {
            self.step(context);
        } else {
            self.send_frame(context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn malformed_items_cannot_replace_valid_items_or_change_selected_action_identity() {
        let mut menu = PauseMenu::default();
        menu.update_items(&json!([{"slot":5,"label":"FIFTH"},{"slot":1,"label":"FIRST"}]))
            .unwrap();
        menu.selected = 2;
        menu.update_items(&json!([{"slot":5,"label":"RENAMED"}]))
            .unwrap();
        assert_eq!(menu.entries()[menu.selected].0, Action::Cartridge(5));
        let previous = menu.entries();
        for invalid in [
            json!([{"slot":0,"label":"BAD"}]),
            json!([{"slot":1,"label":"BAD\n"}]),
            json!([{"slot":1,"label":""}]),
            json!([{"slot":1,"label":"é"}]),
            json!([{"slot":1,"label":"X"},{"slot":1,"label":"Y"}]),
            json!([{"slot":1,"label":"X","extra":true}]),
        ] {
            assert!(menu.update_items(&invalid).is_err());
            assert_eq!(menu.entries(), previous);
        }
        menu.update_items(&json!([])).unwrap();
        assert_eq!(menu.entries()[menu.selected].0, Action::Resume);
    }

    #[test]
    fn actual_modal_fits_eight_items_and_routes_keyboard_controller_and_pointer_selection() {
        let context = egui::Context::default();
        crate::ui_assets::install_font(&context);
        ui_theme::install(&context);
        let mut menu = PauseMenu::default();
        menu.update_items(&json!(
            (1..=5)
                .map(|slot| json!({"slot":slot,"label":"TWENTY FOUR CHARACTERS!!"}))
                .collect::<Vec<_>>()
        ))
        .unwrap();
        let mut pass = |events, enabled, navigation| {
            let mut action = None;
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 600.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        action = menu.show(
                            ui.ctx(),
                            "MENU ACTIONS DEMO",
                            enabled,
                            "FINISH CURRENT FRAME",
                            navigation,
                        );
                    },
                )
                .drop_without_applying_deltas();
            action
        };
        assert_eq!(
            pass(
                vec![],
                true,
                Navigation {
                    confirm: true,
                    ..Default::default()
                }
            ),
            None,
            "the opening gesture cannot also activate an item"
        );
        pass(vec![], true, Navigation::default());
        assert_eq!(
            pass(
                vec![],
                true,
                Navigation {
                    down: true,
                    confirm: true,
                    ..Default::default()
                }
            ),
            Some(Action::Cartridge(1))
        );
        assert_eq!(
            pass(
                vec![],
                false,
                Navigation {
                    down: true,
                    confirm: true,
                    ..Default::default()
                }
            ),
            Some(Action::Restart)
        );
        let event = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert_eq!(
            pass(
                vec![
                    event(Key::ArrowDown, true),
                    event(Key::ArrowDown, false),
                    event(Key::Enter, true),
                    event(Key::Enter, false)
                ],
                true,
                Navigation::default()
            ),
            Some(Action::Source)
        );
        assert_eq!(
            pass(
                vec![event(Key::X, true), event(Key::X, false)],
                true,
                Navigation::default()
            ),
            Some(Action::Resume)
        );
        assert_eq!(menu.rows.len(), 8);
        assert!(
            menu.rows.iter().all(|rect| rect.top() >= 0.0
                && rect.bottom() <= 600.0
                && rect.left() >= 0.0
                && rect.right() <= 900.0),
            "{:?}",
            menu.rows
        );
        let position = menu.rows[2].center();
        let mut action = None;
        for pressed in [true, false] {
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 600.0),
                        )),
                        events: vec![
                            egui::Event::PointerMoved(position),
                            egui::Event::PointerButton {
                                pos: position,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        ..Default::default()
                    },
                    |ui| {
                        action = menu.show(
                            ui.ctx(),
                            "MENU ACTIONS DEMO",
                            true,
                            "",
                            Navigation::default(),
                        );
                    },
                )
                .drop_without_applying_deltas();
        }
        assert_eq!(action, Some(Action::Cartridge(2)));
    }
}
