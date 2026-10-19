//! Source-preserving eight-channel tracker. This edits assets, never synthesizer state.
use crate::ui_theme;
use eframe::egui::{self, Key};
use serde_json::{Value, json};

const CHANNELS: usize = 8;
const MAX_ROWS: usize = 256;
const MAX_PATTERNS: usize = 256;
const MAX_ORDERS: usize = 4096;
const CELL_WIDTH: f32 = 108.0;
const ROW_HEIGHT: f32 = 38.0;
const GUTTER: f32 = 42.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Audition {
        sound: String,
        note: u8,
        volume: f64,
    },
    Pattern,
    Song(usize),
    Stop,
    Undo,
    Redo,
}

#[derive(Default)]
pub struct TrackerEditor {
    pattern: String,
    row: usize,
    channel: usize,
    order: usize,
    anchor: Option<(usize, usize)>,
    clipboard: Option<Vec<Vec<Value>>>,
    sound: String,
    octave: u8,
    step: usize,
    volume: f64,
    audition_only: bool,
    rename: String,
    status: String,
    actions: Vec<Action>,
    scroll_cursor: bool,
}

impl TrackerEditor {
    pub fn new() -> Self {
        Self {
            octave: 4,
            step: 1,
            volume: 1.0,
            ..Self::default()
        }
    }

    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.pattern.capacity()
            + self.sound.capacity()
            + self.rename.capacity()
            + self.status.capacity()
            + self.clipboard.as_ref().map_or(0, |rows| {
                rows.iter()
                    .flatten()
                    .map(crate::asset_sessions::json_bytes)
                    .sum::<usize>()
            })
            + self.actions.capacity() * std::mem::size_of::<Action>()
    }

    pub fn cursor(&self, document: &Value) -> Option<(String, usize, usize)> {
        let patterns = document["patterns"].as_object()?;
        let name = if patterns.contains_key(&self.pattern) {
            &self.pattern
        } else {
            patterns.keys().next()?
        };
        let rows = patterns[name]["rows"].as_array()?;
        (!rows.is_empty()).then(|| {
            (
                name.clone(),
                self.row.min(rows.len() - 1),
                self.channel.min(7),
            )
        })
    }

    fn normalize(&mut self, document: &Value) -> bool {
        if !valid_shape(document) {
            return false;
        }
        let Some((pattern, row, channel)) = self.cursor(document) else {
            return false;
        };
        let Some(rows) = document["patterns"][&pattern]["rows"].as_array() else {
            return false;
        };
        if rows.len() > MAX_ROWS
            || rows
                .iter()
                .any(|row| row.as_array().is_none_or(|cells| cells.len() != CHANNELS))
        {
            return false;
        }
        let Some(order) = document["order"]
            .as_array()
            .filter(|order| !order.is_empty())
        else {
            return false;
        };
        self.pattern = pattern;
        self.row = row;
        self.channel = channel;
        self.order = self.order.min(order.len() - 1);
        self.anchor = self
            .anchor
            .map(|(row, channel)| (row.min(rows.len() - 1), channel.min(7)));
        true
    }

    fn selection(&self) -> (usize, usize, usize, usize) {
        let (row, channel) = self.anchor.unwrap_or((self.row, self.channel));
        (
            row.min(self.row),
            channel.min(self.channel),
            row.max(self.row),
            channel.max(self.channel),
        )
    }

    fn move_cursor(&mut self, row: usize, channel: usize, extend: bool, rows: usize) {
        if extend {
            self.anchor.get_or_insert((self.row, self.channel));
        } else {
            self.anchor = None;
        }
        self.row = row.min(rows - 1);
        self.channel = channel.min(CHANNELS - 1);
        self.scroll_cursor = true;
    }

    fn copy(&mut self, document: &Value) {
        let (r0, c0, r1, c1) = self.selection();
        let rows = &document["patterns"][&self.pattern]["rows"];
        self.clipboard = Some(
            (r0..=r1)
                .map(|row| {
                    (c0..=c1)
                        .map(|channel| rows[row][channel].clone())
                        .collect()
                })
                .collect(),
        );
        self.status = format!("COPIED {} ROWS × {} CHANNELS", r1 - r0 + 1, c1 - c0 + 1);
    }

    fn clear(&self, document: &mut Value) {
        let (r0, c0, r1, c1) = self.selection();
        let rows = &mut document["patterns"][&self.pattern]["rows"];
        for row in r0..=r1 {
            for channel in c0..=c1 {
                rows[row][channel] = Value::Null;
            }
        }
    }

    fn paste(&mut self, document: &mut Value) {
        let Some(block) = &self.clipboard else { return };
        let rows = document["patterns"][&self.pattern]["rows"]
            .as_array_mut()
            .unwrap();
        for (dy, source) in block.iter().enumerate() {
            let Some(destination) = rows.get_mut(self.row + dy).and_then(Value::as_array_mut)
            else {
                break;
            };
            for (dx, cell) in source.iter().enumerate() {
                let Some(destination) = destination.get_mut(self.channel + dx) else {
                    break;
                };
                *destination = cell.clone();
            }
        }
        self.status = "PASTED / CLIPPED TO PATTERN BOUNDS".to_owned();
    }

    fn transpose(&mut self, document: &mut Value, delta: i16) {
        let (r0, c0, r1, c1) = self.selection();
        let rows = &mut document["patterns"][&self.pattern]["rows"];
        // Preflight the entire block; never partially transpose at MIDI boundaries.
        if (r0..=r1).any(|row| {
            (c0..=c1).any(|channel| {
                rows[row][channel]["note"]
                    .as_i64()
                    .is_some_and(|note| !(0..=127).contains(&(note + i64::from(delta))))
            })
        }) {
            self.status = "TRANSPOSE REFUSED / MIDI RANGE 0–127".to_owned();
            return;
        }
        for row in r0..=r1 {
            for channel in c0..=c1 {
                if let Some(note) = rows[row][channel]["note"].as_i64() {
                    rows[row][channel]["note"] = json!(note + i64::from(delta));
                }
            }
        }
    }

    fn enter_note(&mut self, document: &mut Value, note: u8) {
        if self.sound.is_empty() {
            self.status = "DECLARE AND SELECT A SOUND PATCH FIRST".to_owned();
            return;
        }
        if !self.audition_only {
            let cell = &mut document["patterns"][&self.pattern]["rows"][self.row][self.channel];
            // Preserve optional/future authored metadata on an existing cell.
            if !cell.is_object() {
                *cell = json!({});
            }
            cell["note"] = json!(note);
            cell["sound"] = json!(self.sound);
            cell["volume"] = json!(self.volume);
            let rows = document["patterns"][&self.pattern]["rows"]
                .as_array()
                .unwrap()
                .len();
            self.move_cursor(
                (self.row + self.step).min(rows - 1),
                self.channel,
                false,
                rows,
            );
        }
        self.actions.push(Action::Audition {
            sound: self.sound.clone(),
            note,
            volume: self.volume,
        });
    }

    fn create_pattern(&mut self, document: &mut Value, duplicate: bool) {
        let patterns = document["patterns"].as_object_mut().unwrap();
        if patterns.len() >= MAX_PATTERNS {
            self.status = "PATTERN LIMIT: 256".to_owned();
            return;
        }
        let name = (0..MAX_PATTERNS)
            .map(|index| format!("{index:02X}"))
            .find(|name| !patterns.contains_key(name))
            .unwrap();
        let pattern = if duplicate {
            patterns[&self.pattern].clone()
        } else {
            json!({"rows":vec![vec![Value::Null; CHANNELS];16]})
        };
        patterns.insert(name.clone(), pattern);
        self.pattern = name;
        self.row = 0;
        self.anchor = None;
    }

    fn rename_pattern(&mut self, document: &mut Value) {
        let name = self.rename.trim();
        let patterns = document["patterns"].as_object_mut().unwrap();
        if name.is_empty() || name.len() > 64 || patterns.contains_key(name) {
            self.status = "NAME MUST BE UNIQUE AND 1–64 UTF-8 BYTES".to_owned();
            return;
        }
        let pattern = patterns.remove(&self.pattern).unwrap();
        patterns.insert(name.to_owned(), pattern);
        for entry in document["order"].as_array_mut().unwrap() {
            if entry.as_str() == Some(&self.pattern) {
                *entry = json!(name);
            }
        }
        self.pattern = name.to_owned();
        self.rename.clear();
        self.status = "RENAMED PATTERN AND EVERY ORDER REFERENCE".to_owned();
    }

    fn delete_pattern(&mut self, document: &mut Value) {
        if document["order"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name.as_str() == Some(&self.pattern))
        {
            self.status = "REMOVE THIS PATTERN FROM THE ORDER LIST BEFORE DELETING".to_owned();
            return;
        }
        let patterns = document["patterns"].as_object_mut().unwrap();
        if patterns.len() <= 1 {
            return;
        }
        patterns.remove(&self.pattern);
        self.pattern = patterns.keys().next().unwrap().clone();
        self.row = 0;
        self.anchor = None;
    }

    fn row_edit(&mut self, document: &mut Value, insert: bool) {
        let rows = document["patterns"][&self.pattern]["rows"]
            .as_array_mut()
            .unwrap();
        if insert && rows.len() < MAX_ROWS {
            rows.insert(self.row, json!(vec![Value::Null; CHANNELS]));
        } else if !insert && rows.len() > 1 {
            rows.remove(self.row);
            self.row = self.row.min(rows.len() - 1);
        }
        self.anchor = None;
    }

    fn order_edit(&mut self, document: &mut Value, operation: &str) {
        let order = document["order"].as_array_mut().unwrap();
        match operation {
            "insert" if order.len() < MAX_ORDERS => {
                self.order += 1;
                order.insert(self.order, json!(self.pattern));
            }
            "replace" => order[self.order] = json!(self.pattern),
            "remove" if order.len() > 1 => {
                order.remove(self.order);
                self.order = self.order.min(order.len() - 1);
            }
            "left" if self.order > 0 => {
                order.swap(self.order, self.order - 1);
                self.order -= 1;
            }
            "right" if self.order + 1 < order.len() => {
                order.swap(self.order, self.order + 1);
                self.order += 1;
            }
            _ => {}
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Value,
        sounds: &[String],
        position: Option<&Value>,
    ) {
        if !self.normalize(document) {
            ui.colored_label(
                ui_theme::CORAL,
                "INVALID TRACKER SHAPE / FIX RAW JSON; NO DATA WAS CHANGED",
            );
            return;
        }
        if !sounds.contains(&self.sound) {
            self.sound = sounds.first().cloned().unwrap_or_default();
        }
        self.transport_controls(ui, document, sounds);
        self.pattern_controls(ui, document);
        ui.collapsing("SEQUENCE / BLOCK TOOLS / KEYBOARD HELP", |ui| {
        self.sequence_controls(ui, document);
        ui.horizontal_wrapped(|ui| {
            for (label, operation) in [
                ("INSERT ROW", "insert"),
                ("DELETE ROW", "delete"),
                ("COPY", "copy"),
                ("CUT", "cut"),
                ("PASTE", "paste"),
                ("CLEAR", "clear"),
                ("−SEMITONE", "down"),
                ("+SEMITONE", "up"),
            ] {
                if ui.button(label).clicked() {
                    self.edit(document, operation);
                }
            }
        });
        ui.label(
            "CLICK GRID: Z S X D C V G B H N J M = NOTES / ARROWS, TAB, HOME/END / SHIFT = BLOCK",
        );
        ui.label("CTRL/CMD C X V Z Y / DEL: CLEAR / INSERT: ROW / ENTER: PATTERN / CTRL+ENTER: SONG / ESC: STOP");
        });
        if !self.status.is_empty() {
            ui.colored_label(ui_theme::AMBER, &self.status);
        }
        self.grid(ui, document, position);
        self.cell_controls(ui, document, sounds);
    }

    fn transport_controls(&mut self, ui: &mut egui::Ui, document: &mut Value, sounds: &[String]) {
        ui.horizontal_wrapped(|ui| {
            let mut speed = document["framesPerRow"].as_u64().unwrap_or(6);
            ui.label("FRAMES/ROW");
            if ui
                .add(egui::DragValue::new(&mut speed).range(1..=240))
                .changed()
            {
                document["framesPerRow"] = json!(speed);
            }
            let mut looping = document["loop"].as_bool().unwrap_or(false);
            if ui.checkbox(&mut looping, "LOOP").changed() {
                document["loop"] = json!(looping);
            }
            ui.checkbox(&mut self.audition_only, "AUDITION ONLY");
            ui.label("OCTAVE");
            ui.add(egui::DragValue::new(&mut self.octave).range(0..=8));
            ui.label("STEP");
            ui.add(egui::DragValue::new(&mut self.step).range(0..=16));
            ui.label("ENTRY VOLUME");
            ui.add(
                egui::DragValue::new(&mut self.volume)
                    .range(0.0..=1.0)
                    .speed(0.01),
            );
            egui::ComboBox::from_id_salt("tracker-entry-sound")
                .selected_text(if self.sound.is_empty() {
                    "NO SOUND"
                } else {
                    &self.sound
                })
                .show_ui(ui, |ui| {
                    for sound in sounds {
                        ui.selectable_value(&mut self.sound, sound.clone(), sound);
                    }
                });
        });
    }

    fn pattern_controls(&mut self, ui: &mut egui::Ui, document: &mut Value) {
        ui.horizontal_wrapped(|ui| {
            ui.label("PATTERN");
            egui::ComboBox::from_id_salt("tracker-pattern")
                .selected_text(&self.pattern)
                .show_ui(ui, |ui| {
                    for name in document["patterns"].as_object().unwrap().keys() {
                        if ui
                            .selectable_value(&mut self.pattern, name.clone(), name)
                            .changed()
                        {
                            self.row = 0;
                            self.anchor = None;
                        }
                    }
                });
            if ui.button("NEW").clicked() {
                self.create_pattern(document, false);
            }
            if ui.button("DUPLICATE").clicked() {
                self.create_pattern(document, true);
            }
            if ui.button("DELETE PATTERN").clicked() {
                self.delete_pattern(document);
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.rename)
                    .desired_width(100.0)
                    .hint_text("new name"),
            );
            if ui.button("RENAME").clicked() {
                self.rename_pattern(document);
            }
        });
    }

    fn sequence_controls(&mut self, ui: &mut egui::Ui, document: &mut Value) {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("ORDER {:03}", self.order));
            for (label, operation) in [
                ("+ SELECTED", "insert"),
                ("REPLACE", "replace"),
                ("REMOVE", "remove"),
                ("←", "left"),
                ("→", "right"),
            ] {
                if ui.button(label).clicked() {
                    self.order_edit(document, operation);
                }
            }
        });
        egui::ScrollArea::horizontal()
            .id_salt("tracker-order-list")
            .max_height(45.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, pattern) in document["order"].as_array().unwrap().iter().enumerate()
                    {
                        let name = pattern.as_str().unwrap_or("?");
                        if ui
                            .selectable_label(index == self.order, format!("{index:03}:{name}"))
                            .clicked()
                        {
                            self.order = index;
                            if document["patterns"].get(name).is_some() {
                                self.pattern = name.to_owned();
                                self.row = 0;
                                self.anchor = None;
                            }
                        }
                    }
                });
            });
    }

    fn edit(&mut self, document: &mut Value, operation: &str) {
        match operation {
            "copy" => self.copy(document),
            "cut" => {
                self.copy(document);
                self.clear(document);
            }
            "paste" => self.paste(document),
            "clear" => self.clear(document),
            "insert" => self.row_edit(document, true),
            "delete" => self.row_edit(document, false),
            "down" => self.transpose(document, -1),
            "up" => self.transpose(document, 1),
            _ => {}
        }
    }

    fn keyboard(&mut self, ui: &mut egui::Ui, document: &mut Value, id: egui::Id) {
        if !ui.memory(|memory| memory.has_focus(id)) {
            return;
        }
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            )
        });
        let events = ui.input(|input| input.events.clone());
        let native_clipboard_event = events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_)
            )
        });
        for event in events {
            match &event {
                egui::Event::Copy => {
                    self.copy(document);
                    continue;
                }
                egui::Event::Cut => {
                    self.edit(document, "cut");
                    continue;
                }
                egui::Event::Paste(_) => {
                    self.paste(document);
                    continue;
                }
                _ => {}
            }
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                repeat,
                ..
            } = event
            else {
                continue;
            };
            let command = modifiers.command || modifiers.ctrl;
            let rows = document["patterns"][&self.pattern]["rows"]
                .as_array()
                .unwrap()
                .len();
            if command {
                if native_clipboard_event && matches!(key, Key::C | Key::X | Key::V) {
                    continue;
                }
                match key {
                    Key::C => self.copy(document),
                    Key::X => self.edit(document, "cut"),
                    Key::V => self.paste(document),
                    Key::A => {
                        self.anchor = Some((0, 0));
                        self.row = rows - 1;
                        self.channel = 7;
                    }
                    Key::Z => self.actions.push(if modifiers.shift {
                        Action::Redo
                    } else {
                        Action::Undo
                    }),
                    Key::Y => self.actions.push(Action::Redo),
                    Key::Enter => self.actions.push(Action::Song(self.order)),
                    _ => continue,
                }
            } else {
                match key {
                    Key::ArrowUp => self.move_cursor(
                        self.row.saturating_sub(1),
                        self.channel,
                        modifiers.shift,
                        rows,
                    ),
                    Key::ArrowDown => {
                        self.move_cursor(self.row + 1, self.channel, modifiers.shift, rows)
                    }
                    Key::ArrowLeft => self.move_cursor(
                        self.row,
                        self.channel.saturating_sub(1),
                        modifiers.shift,
                        rows,
                    ),
                    Key::ArrowRight => {
                        self.move_cursor(self.row, self.channel + 1, modifiers.shift, rows)
                    }
                    Key::Tab => self.move_cursor(
                        self.row,
                        if modifiers.shift {
                            (self.channel + 7) % 8
                        } else {
                            (self.channel + 1) % 8
                        },
                        false,
                        rows,
                    ),
                    Key::PageUp => self.move_cursor(
                        self.row.saturating_sub(16),
                        self.channel,
                        modifiers.shift,
                        rows,
                    ),
                    Key::PageDown => {
                        self.move_cursor(self.row + 16, self.channel, modifiers.shift, rows)
                    }
                    Key::Home => self.move_cursor(0, self.channel, modifiers.shift, rows),
                    Key::End => self.move_cursor(rows - 1, self.channel, modifiers.shift, rows),
                    Key::Delete | Key::Backspace => self.clear(document),
                    Key::Insert => self.row_edit(document, true),
                    Key::Enter => self.actions.push(Action::Pattern),
                    Key::Escape => self.actions.push(Action::Stop),
                    _ if !repeat && !modifiers.alt => {
                        if let Some(semitone) = note_key(key) {
                            let note = u16::from(self.octave + 1) * 12 + u16::from(semitone);
                            if note <= 127 {
                                self.enter_note(document, note as u8);
                            }
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                }
            }
            ui.input_mut(|input| {
                input.consume_key(modifiers, key);
            });
        }
    }

    fn grid(
        &mut self,
        ui: &mut egui::Ui,
        document: &mut Value,
        position: Option<&Value>,
    ) -> egui::Vec2 {
        let row_count = document["patterns"][&self.pattern]["rows"]
            .as_array()
            .unwrap()
            .len();
        let id = ui.make_persistent_id("tracker-grid");
        let playing = position
            .filter(|position| position["pattern"].as_str() == Some(&self.pattern))
            .and_then(|position| position["row"].as_u64())
            .map(|row| row as usize);
        egui::ScrollArea::both()
            .id_salt("tracker-grid-scroll")
            .max_height(390.0)
            .min_scrolled_height(160.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (_, rect) = ui.allocate_space(egui::vec2(
                    GUTTER + CELL_WIDTH * CHANNELS as f32,
                    ROW_HEIGHT * (row_count + 1) as f32,
                ));
                // One stable focus owner, not a separate button/tab stop for every note.
                let response = ui.interact(rect, id, egui::Sense::click_and_drag());
                let header_y = rect.min.y.max(ui.clip_rect().min.y);
                if (response.clicked() || response.dragged())
                    && let Some(pointer) = response.interact_pointer_pos()
                {
                    response.request_focus();
                    let local = pointer - rect.min;
                    if pointer.y >= header_y + ROW_HEIGHT && local.x >= GUTTER {
                        let row = ((local.y / ROW_HEIGHT) as usize)
                            .saturating_sub(1)
                            .min(row_count - 1);
                        let channel = ((local.x - GUTTER) / CELL_WIDTH) as usize;
                        let extend = response.dragged() || ui.input(|input| input.modifiers.shift);
                        self.move_cursor(row, channel, extend, row_count);
                    }
                }
                self.keyboard(ui, document, id);
                if self.scroll_cursor {
                    let cursor = rect.min
                        + egui::vec2(
                            GUTTER + CELL_WIDTH * self.channel as f32,
                            ROW_HEIGHT * (self.row + 1) as f32,
                        );
                    let mut cursor_rect =
                        egui::Rect::from_min_size(cursor, egui::vec2(CELL_WIDTH, ROW_HEIGHT));
                    cursor_rect.min.y -= ROW_HEIGHT;
                    ui.scroll_to_rect_animation(
                        cursor_rect,
                        None,
                        egui::style::ScrollAnimation::none(),
                    );
                    self.scroll_cursor = false;
                }
                let (r0, c0, r1, c1) = self.selection();
                let font = egui::FontId::monospace(14.0);
                for row in 0..row_count {
                    let y = rect.min.y + ROW_HEIGHT * (row + 1) as f32;
                    let row_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.min.x, y),
                        egui::vec2(rect.width(), ROW_HEIGHT),
                    );
                    if !ui.is_rect_visible(row_rect) {
                        continue;
                    }
                    ui.painter().text(
                        egui::pos2(rect.min.x + 3.0, y + 10.0),
                        egui::Align2::LEFT_TOP,
                        format!("{row:03}"),
                        font.clone(),
                        if playing == Some(row) {
                            ui_theme::CYAN
                        } else {
                            ui_theme::MUTED
                        },
                    );
                    for channel in 0..CHANNELS {
                        let cell_rect = egui::Rect::from_min_size(
                            egui::pos2(rect.min.x + GUTTER + CELL_WIDTH * channel as f32, y),
                            egui::vec2(CELL_WIDTH - 2.0, ROW_HEIGHT - 1.0),
                        );
                        let selected = (r0..=r1).contains(&row) && (c0..=c1).contains(&channel);
                        let fill = if selected {
                            ui_theme::SCREEN_RAISED
                        } else if playing == Some(row) {
                            egui::Color32::from_rgb(31, 70, 67)
                        } else if row % 4 == 0 {
                            ui_theme::CASE
                        } else {
                            ui_theme::SCREEN
                        };
                        ui.painter().rect_filled(cell_rect, 0.0, fill);
                        if row == self.row && channel == self.channel {
                            ui.painter().rect_stroke(
                                cell_rect,
                                0.0,
                                egui::Stroke::new(
                                    2.0,
                                    if response.has_focus() {
                                        ui_theme::CYAN
                                    } else {
                                        ui_theme::AMBER
                                    },
                                ),
                                egui::StrokeKind::Inside,
                            );
                        }
                        let cell = &document["patterns"][&self.pattern]["rows"][row][channel];
                        let painter = ui
                            .painter()
                            .with_clip_rect(cell_rect.intersect(ui.clip_rect()));
                        let note = cell["note"]
                            .as_u64()
                            .map_or_else(|| "---".to_owned(), |note| note_name(note as u8));
                        let volume = cell
                            .get("volume")
                            .and_then(Value::as_f64)
                            .map_or_else(String::new, |volume| {
                                format!(" {:02}", (volume * 99.0).round() as u8)
                            });
                        painter.text(
                            cell_rect.min + egui::vec2(5.0, 2.0),
                            egui::Align2::LEFT_TOP,
                            format!("{note}{volume}"),
                            font.clone(),
                            ui_theme::CREAM,
                        );
                        painter.text(
                            cell_rect.min + egui::vec2(5.0, 20.0),
                            egui::Align2::LEFT_TOP,
                            cell["sound"].as_str().unwrap_or("..."),
                            egui::FontId::monospace(12.0),
                            ui_theme::MUTED,
                        );
                    }
                }
                // Keep channel labels visible while the pattern scrolls vertically.
                let header = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x, header_y),
                    egui::vec2(rect.width(), ROW_HEIGHT),
                );
                ui.painter().rect_filled(header, 0.0, ui_theme::SCREEN);
                for channel in 0..CHANNELS {
                    ui.painter().text(
                        egui::pos2(
                            rect.min.x + GUTTER + CELL_WIDTH * channel as f32 + 5.0,
                            header_y + 10.0,
                        ),
                        egui::Align2::LEFT_TOP,
                        format!("CH {}", channel + 1),
                        font.clone(),
                        ui_theme::AMBER,
                    );
                }
            })
            .state
            .clamped_offset()
    }

    fn cell_controls(&mut self, ui: &mut egui::Ui, document: &mut Value, sounds: &[String]) {
        let cell = &mut document["patterns"][&self.pattern]["rows"][self.row][self.channel];
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("CELL {:03}/CH{}", self.row, self.channel + 1));
            if cell.is_null() {
                if ui.add_enabled(!self.sound.is_empty(), egui::Button::new("INSERT NOTE")).clicked() {
                    *cell = json!({"note":(self.octave + 1) * 12,"sound":self.sound,"volume":self.volume});
                }
                return;
            }
            let mut note = cell["note"].as_u64().unwrap_or(60);
            if ui.add(egui::DragValue::new(&mut note).range(0..=127)).changed() { cell["note"] = json!(note); }
            ui.label(note_name(note.min(127) as u8));
            let mut sound = cell["sound"].as_str().unwrap_or("").to_owned();
            egui::ComboBox::from_id_salt("tracker-cell-sound").selected_text(&sound).show_ui(ui, |ui| {
                for name in sounds { if ui.selectable_value(&mut sound, name.clone(), name).changed() { cell["sound"] = json!(sound); } }
            });
            let mut volume = cell["volume"].as_f64().unwrap_or(1.0);
            ui.label("VOL");
            if ui.add(egui::DragValue::new(&mut volume).range(0.0..=1.0).speed(0.01)).changed() { cell["volume"] = json!(volume); }
            if ui.button("DEFAULT VOLUME").clicked() { cell.as_object_mut().unwrap().remove("volume"); }
            if ui.button("AUDITION").clicked() { self.actions.push(Action::Audition { sound, note: note.min(127) as u8, volume }); }
        });
    }
}

fn valid_shape(document: &Value) -> bool {
    let Some(patterns) = document["patterns"]
        .as_object()
        .filter(|patterns| !patterns.is_empty())
    else {
        return false;
    };
    let Some(order) = document["order"]
        .as_array()
        .filter(|order| !order.is_empty())
    else {
        return false;
    };
    document["kind"] == "music"
        && document["framesPerRow"]
            .as_u64()
            .is_some_and(|speed| (1..=240).contains(&speed))
        && document["loop"].is_boolean()
        && order.iter().all(|entry| {
            entry
                .as_str()
                .is_some_and(|name| patterns.contains_key(name))
        })
        && patterns.values().all(|pattern| {
            pattern["rows"].as_array().is_some_and(|rows| {
                (1..=MAX_ROWS).contains(&rows.len())
                    && rows.iter().all(|row| {
                        row.as_array().is_some_and(|cells| {
                            cells.len() == CHANNELS
                                && cells.iter().all(|cell| {
                                    cell.is_null()
                                        || (cell.is_object()
                                            && cell["note"]
                                                .as_u64()
                                                .is_some_and(|note| note <= 127)
                                            && cell["sound"]
                                                .as_str()
                                                .is_some_and(|sound| !sound.is_empty())
                                            && cell.get("volume").is_none_or(|volume| {
                                                volume.as_f64().is_some_and(|volume| {
                                                    (0.0..=1.0).contains(&volume)
                                                })
                                            }))
                                })
                        })
                    })
            })
        })
}

fn note_key(key: Key) -> Option<u8> {
    [
        Key::Z,
        Key::S,
        Key::X,
        Key::D,
        Key::C,
        Key::V,
        Key::G,
        Key::B,
        Key::H,
        Key::N,
        Key::J,
        Key::M,
        Key::Comma,
        Key::L,
        Key::Period,
        Key::Semicolon,
        Key::Slash,
    ]
    .iter()
    .position(|note| *note == key)
    .map(|index| index as u8)
}

fn note_name(note: u8) -> String {
    let names = [
        "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
    ];
    format!(
        "{}{}",
        names[usize::from(note % 12)],
        i16::from(note / 12) - 1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song() -> Value {
        json!({"kind":"music","revision":1,"framesPerRow":240,"loop":true,"order":["A","A"],"patterns":{"A":{"rows":vec![vec![Value::Null;8];16],"annotation":"retain"}},"authored":"retain"})
    }

    #[test]
    fn eight_channel_note_entry_and_clipped_block_paste_preserve_source_metadata() {
        let mut document = song();
        let mut editor = TrackerEditor::new();
        assert!(editor.normalize(&document));
        editor.sound = "beep".to_owned();
        for channel in 0..8 {
            editor.row = 0;
            editor.channel = channel;
            editor.enter_note(&mut document, 60 + channel as u8);
        }
        assert_eq!(
            document["patterns"]["A"]["rows"][0]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        assert_eq!(document["patterns"]["A"]["rows"][0][7]["note"], 67);
        editor.anchor = Some((0, 0));
        editor.row = 1;
        editor.channel = 7;
        document["patterns"]["A"]["rows"][0][0]["custom"] = json!("keep");
        editor.copy(&document);
        editor.anchor = None;
        editor.row = 15;
        editor.channel = 6;
        editor.paste(&mut document);
        assert_eq!(document["patterns"]["A"]["rows"][15][6]["custom"], "keep");
        assert_eq!(document["patterns"]["A"]["rows"][15][7]["note"], 61);
        assert_eq!(document["authored"], "retain");
        assert_eq!(document["patterns"]["A"]["annotation"], "retain");
        assert_eq!(document["framesPerRow"], 240);
    }

    #[test]
    fn patterns_and_orders_are_reference_safe_and_undoable_as_one_document() {
        let mut document = song();
        let mut editor = TrackerEditor::new();
        editor.normalize(&document);
        let original = document.clone();
        editor.delete_pattern(&mut document);
        assert_eq!(document, original);
        editor.rename = "Intro".to_owned();
        editor.rename_pattern(&mut document);
        assert_eq!(document["order"], json!(["Intro", "Intro"]));
        assert_eq!(document["patterns"]["Intro"]["annotation"], "retain");
        let renamed = document.clone();
        editor.rename = "Intro".to_owned();
        editor.rename_pattern(&mut document);
        assert_eq!(document, renamed);
        editor.create_pattern(&mut document, true);
        assert_eq!(editor.pattern, "00");
        assert_eq!(document["patterns"]["00"], document["patterns"]["Intro"]);
        editor.order_edit(&mut document, "insert");
        assert_eq!(document["order"], json!(["Intro", "00", "Intro"]));
        editor.order_edit(&mut document, "left");
        assert_eq!(document["order"], json!(["00", "Intro", "Intro"]));
        editor.order_edit(&mut document, "remove");
        editor.delete_pattern(&mut document);
        assert_eq!(document, renamed);
    }

    #[test]
    fn transpose_preflights_the_whole_block_and_audition_only_never_edits_source() {
        let mut document = song();
        let mut editor = TrackerEditor::new();
        editor.normalize(&document);
        document["patterns"]["A"]["rows"][0][0] = json!({"note":127,"sound":"beep"});
        document["patterns"]["A"]["rows"][0][1] = json!({"note":60,"sound":"beep","volume":0.4});
        editor.anchor = Some((0, 0));
        editor.channel = 1;
        let before = document.clone();
        editor.transpose(&mut document, 1);
        assert_eq!(document, before);
        editor.transpose(&mut document, -12);
        assert_eq!(document["patterns"]["A"]["rows"][0][1]["note"], 48);
        assert_eq!(document["patterns"]["A"]["rows"][0][1]["volume"], 0.4);
        editor.audition_only = true;
        editor.sound = "beep".to_owned();
        let before = document.clone();
        editor.enter_note(&mut document, 64);
        assert_eq!(document, before);
        assert!(matches!(
            editor.take_actions()[0],
            Action::Audition { note: 64, .. }
        ));
        for _ in 0..300 {
            editor.row_edit(&mut document, true);
        }
        assert_eq!(
            document["patterns"]["A"]["rows"].as_array().unwrap().len(),
            256
        );
        for _ in 0..300 {
            editor.row_edit(&mut document, false);
        }
        assert_eq!(
            document["patterns"]["A"]["rows"].as_array().unwrap().len(),
            1
        );
        assert_eq!(note_name(0), "C--1");
        assert_eq!(note_name(60), "C-4");
        assert_eq!(note_name(127), "G-9");
    }

    fn key(key: Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn grid_frame(
        editor: &mut TrackerEditor,
        document: &mut Value,
        context: &egui::Context,
        events: Vec<egui::Event>,
        focused: bool,
    ) -> egui::Vec2 {
        let mut scroll = egui::Vec2::ZERO;
        let events = events
            .into_iter()
            .flat_map(|event| {
                if let egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                } = &event
                {
                    vec![
                        event.clone(),
                        egui::Event::Key {
                            key: *key,
                            physical_key: None,
                            pressed: false,
                            repeat: false,
                            modifiers: *modifiers,
                        },
                    ]
                } else {
                    vec![event]
                }
            })
            .collect();
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
                    assert!(editor.normalize(document));
                    editor.sound = "beep".to_owned();
                    if focused {
                        ui.memory_mut(|memory| {
                            memory.request_focus(ui.make_persistent_id("tracker-grid"))
                        });
                    }
                    scroll = editor.grid(ui, document, None);
                },
            )
            .drop_without_applying_deltas();
        scroll
    }

    #[test]
    fn actual_grid_routes_notes_navigation_native_clipboard_and_transport_only_when_focused() {
        let context = egui::Context::default();
        crate::ui_assets::install_font(&context);
        crate::ui_theme::install(&context);
        let mut editor = TrackerEditor::new();
        let mut document = song();
        grid_frame(
            &mut editor,
            &mut document,
            &context,
            vec![key(Key::Z, egui::Modifiers::NONE)],
            false,
        );
        assert!(document["patterns"]["A"]["rows"][0][0].is_null());
        grid_frame(
            &mut editor,
            &mut document,
            &context,
            vec![key(Key::Z, egui::Modifiers::NONE)],
            true,
        );
        assert_eq!(document["patterns"]["A"]["rows"][0][0]["note"], 60);
        assert_eq!(editor.row, 1);
        // egui installs focus lock filters once the newly focused widget has survived one frame.
        grid_frame(&mut editor, &mut document, &context, Vec::new(), false);
        grid_frame(
            &mut editor,
            &mut document,
            &context,
            vec![
                key(Key::Tab, egui::Modifiers::NONE),
                key(Key::X, egui::Modifiers::NONE),
            ],
            false,
        );
        assert_eq!(document["patterns"]["A"]["rows"][1][1]["note"], 62);
        assert_eq!(editor.channel, 1);
        grid_frame(
            &mut editor,
            &mut document,
            &context,
            vec![
                key(Key::Home, egui::Modifiers::NONE),
                key(Key::ArrowLeft, egui::Modifiers::NONE),
                egui::Event::Copy,
            ],
            false,
        );
        grid_frame(
            &mut editor,
            &mut document,
            &context,
            vec![
                key(Key::End, egui::Modifiers::NONE),
                egui::Event::Paste("ignored external text".to_owned()),
            ],
            false,
        );
        assert_eq!(document["patterns"]["A"]["rows"][15][0]["note"], 60);
        grid_frame(
            &mut editor,
            &mut document,
            &context,
            vec![
                key(Key::Enter, egui::Modifiers::NONE),
                key(Key::Escape, egui::Modifiers::NONE),
            ],
            false,
        );
        let actions = editor.take_actions();
        assert!(actions.contains(&Action::Pattern));
        assert!(actions.contains(&Action::Stop));
        context.memory_mut(|memory| memory.request_focus(egui::Id::unique("another-tool")));
        let before = document.clone();
        grid_frame(
            &mut editor,
            &mut document,
            &context,
            vec![
                key(Key::C, egui::Modifiers::NONE),
                key(Key::Enter, egui::Modifiers::NONE),
            ],
            false,
        );
        assert_eq!(document, before);
        assert!(editor.take_actions().is_empty());
    }

    #[test]
    fn malformed_unselected_patterns_are_not_edited_and_restored_selection_is_clamped() {
        let mut document = song();
        let mut editor = TrackerEditor::new();
        editor.anchor = Some((255, 7));
        editor.row = 255;
        assert!(editor.normalize(&document));
        assert_eq!(editor.selection(), (15, 0, 15, 7));
        for invalid in [
            json!({"rows":[]}),
            json!({"rows":[[42,null,null,null,null,null,null,null]]}),
            json!({"rows":[[null]]}),
        ] {
            document["patterns"]["broken"] = invalid;
            let before = document.clone();
            assert!(!editor.normalize(&document));
            assert_eq!(document, before);
        }
    }

    #[test]
    fn keyboard_navigation_scrolls_to_offscreen_rows_at_minimum_native_size() {
        let context = egui::Context::default();
        crate::ui_assets::install_font(&context);
        crate::ui_theme::install(&context);
        let mut editor = TrackerEditor::new();
        let mut document = song();
        document["patterns"]["A"]["rows"] = json!(vec![vec![Value::Null; 8]; 256]);
        grid_frame(&mut editor, &mut document, &context, Vec::new(), true);
        grid_frame(&mut editor, &mut document, &context, Vec::new(), false);
        grid_frame(
            &mut editor,
            &mut document,
            &context,
            vec![key(Key::End, egui::Modifiers::NONE)],
            false,
        );
        let end = grid_frame(&mut editor, &mut document, &context, Vec::new(), false);
        assert_eq!(editor.row, 255);
        assert!(end.y > 9000.0, "{end:?}");
        grid_frame(
            &mut editor,
            &mut document,
            &context,
            vec![key(Key::Home, egui::Modifiers::NONE)],
            false,
        );
        let home = grid_frame(&mut editor, &mut document, &context, Vec::new(), false);
        assert_eq!(editor.row, 0);
        assert_eq!(home.y, 0.0);
    }

    #[test]
    fn edited_eight_channel_asset_round_trips_into_the_actual_native_synth() {
        use std::{
            thread,
            time::{Duration, Instant},
        };
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/api-tour/cart.m01w");
        let project = crate::project::NativeProject::open(&path).unwrap();
        let mut document = song();
        let mut editor = TrackerEditor::new();
        editor.normalize(&document);
        editor.sound = "beep".to_owned();
        for channel in 0..8 {
            editor.row = 0;
            editor.channel = channel;
            editor.enter_note(&mut document, 60 + channel as u8);
        }
        let text = serde_json::to_string(&document).unwrap();
        let workbench =
            crate::asset_tools::AssetWorkbench::new(modl_core::AssetKind::Music, &text).unwrap();
        let round_trip: Value = serde_json::from_str(&workbench.source().unwrap()).unwrap();
        assert_eq!(document, round_trip);
        let bank = crate::live_audio::project_bank(&project, "loop", &round_trip, None).unwrap();
        let mut preview = crate::live_audio::AudioPreview::new();
        let context = egui::Context::default();
        preview.play(bank, "loop", 1, 0);
        let deadline = Instant::now() + Duration::from_secs(7);
        while preview.packet.is_null() {
            preview.poll(&context, None);
            assert!(Instant::now() < deadline, "{}", preview.status);
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(preview.packet["activeVoices"], 8);
        assert_eq!(preview.packet["position"]["orderIndex"], 1);
        assert_eq!(preview.playhead("loop").unwrap()["pattern"], "A");
        assert!(preview.playhead("another-song").is_none());
        for channel in 0..8 {
            assert_eq!(preview.packet["voices"][channel]["note"], 60 + channel);
        }
        assert!(preview.packet["peakLeft"].as_f64().unwrap() > 0.0);
        preview.stop();
        while preview.active() {
            preview.poll(&context, None);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        assert!(preview.playhead("loop").is_none());
        assert!(!project.dirty);
    }
}
