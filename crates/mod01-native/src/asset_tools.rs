//! Native, source-visible editors for the non-sprite cartridge assets.

use crate::tracker::{Action, TrackerEditor};
use eframe::egui::{self, Color32, Sense};
use modl_core::AssetKind;
use modl_core::assets::integer_value;
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq, Eq)]
enum MapMode {
    Paint,
    Select,
    Stamp,
    Move,
    Fill,
}

type MapClipboard = (usize, usize, Vec<Value>);

pub struct AssetWorkbench {
    pub kind: AssetKind,
    display: bool,
    pub document: Value,
    pub dirty: bool,
    layer: usize,
    tile: usize,
    glyph: usize,
    raster_line: usize,
    raster_default: bool,
    raster_clipboard: Option<Value>,
    logical_color: usize,
    tracker: TrackerEditor,
    tracker_actions: Vec<Action>,
    map_origin: [usize; 2],
    map_mode: MapMode,
    map_anchor: Option<[usize; 2]>,
    map_selection: Option<[usize; 4]>,
    map_clipboard: Option<MapClipboard>,
    font_preview: String,
    history: Vec<Value>,
    future: Vec<Value>,
}

impl AssetWorkbench {
    pub fn new(kind: AssetKind, source: &str) -> Result<Self, String> {
        let document: Value = serde_json::from_str(source).map_err(|error| error.to_string())?;
        Ok(Self {
            kind,
            display: false,
            document,
            dirty: false,
            layer: 0,
            tile: 0,
            glyph: 0,
            raster_line: 0,
            raster_default: false,
            raster_clipboard: None,
            logical_color: 0,
            tracker: TrackerEditor::new(),
            tracker_actions: Vec::new(),
            map_origin: [0, 0],
            map_mode: MapMode::Paint,
            map_anchor: None,
            map_selection: None,
            map_clipboard: None,
            font_preview: "MOD-01 012345".to_owned(),
            history: Vec::new(),
            future: Vec::new(),
        })
    }

    pub fn new_display(source: &str) -> Result<Self, String> {
        let mut editor = Self::new(AssetKind::Map, source)?;
        editor.display = true;
        Ok(editor)
    }

    pub fn source(&self) -> Result<String, String> {
        serde_json::to_string(&self.document)
            .map(|mut source| {
                source.push('\n');
                source
            })
            .map_err(|error| error.to_string())
    }

    pub fn is_display(&self) -> bool {
        self.display
    }

    pub fn retained_bytes(&self) -> usize {
        use crate::asset_sessions::json_bytes;
        std::mem::size_of::<Self>()
            + json_bytes(&self.document)
            + self
                .history
                .iter()
                .chain(&self.future)
                .map(json_bytes)
                .sum::<usize>()
            + self.raster_clipboard.as_ref().map_or(0, json_bytes)
            + self.map_clipboard.as_ref().map_or(0, |(_, _, cells)| {
                cells.iter().map(json_bytes).sum::<usize>()
            })
            + self.font_preview.capacity()
            + self.tracker.retained_bytes()
    }

    pub fn replace_document(&mut self, document: Value) {
        if document != self.document {
            self.history
                .push(std::mem::replace(&mut self.document, document));
            if self.history.len() > 32 {
                self.history.remove(0);
            }
            self.future.clear();
            self.dirty = true;
        }
    }

    pub fn tracker_cursor(&self) -> Option<(String, usize, usize)> {
        (self.kind == AssetKind::Music)
            .then(|| self.tracker.cursor(&self.document))
            .flatten()
    }

    pub fn take_tracker_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.tracker_actions)
    }

    pub fn undo(&mut self) {
        if let Some(previous) = self.history.pop() {
            self.future
                .push(std::mem::replace(&mut self.document, previous));
            self.dirty = true;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.future.pop() {
            self.history
                .push(std::mem::replace(&mut self.document, next));
            self.dirty = true;
        }
    }

    pub fn export_tile_png(&self, palette: &[[u8; 3]; 256]) -> Result<Vec<u8>, String> {
        if self.kind != AssetKind::TileSet || self.display {
            return Err("OPEN A TILE SET FIRST".to_owned());
        }
        let tiles = self.document["tiles"]
            .as_array()
            .ok_or("tile set has no tiles")?;
        if tiles.is_empty() || tiles.len() > 4096 {
            return Err("TILE SET MUST CONTAIN 1-4096 TILES".to_owned());
        }
        let columns = tiles.len().min(16);
        let rows = tiles.len().div_ceil(columns);
        let mut image = image::RgbaImage::new((columns * 8) as u32, (rows * 8) as u32);
        for (index, tile) in tiles.iter().enumerate() {
            let pixels = tile.as_array().ok_or("invalid tile pixels")?;
            if pixels.len() != 64 {
                return Err("tile must have 64 pixels".to_owned());
            }
            for y in 0..8 {
                for x in 0..8 {
                    let color = integer_value(&pixels[y * 8 + x], 0, 255)
                        .ok_or("tile colors must be integers in 0..=255")?
                        as usize;
                    let [r, g, b] = palette[color];
                    image.put_pixel(
                        ((index % columns) * 8 + x) as u32,
                        ((index / columns) * 8 + y) as u32,
                        image::Rgba([r, g, b, if color == 0 { 0 } else { 255 }]),
                    );
                }
            }
        }
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut output, image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        Ok(output.into_inner())
    }

    pub fn import_tile_png(&mut self, png: &[u8]) -> Result<(), String> {
        use modl_core::asset_import::{PngAssetKind, decode_png, indexed_asset};
        if self.kind != AssetKind::TileSet || self.display {
            return Err("OPEN A TILE SET FIRST".to_owned());
        }
        let source = decode_png(png, PngAssetKind::TileSet).map_err(|error| error.to_string())?;
        let document = indexed_asset(&source, PngAssetKind::TileSet, Some(&self.document))
            .map_err(|error| error.to_string())?;
        self.replace_document(document);
        self.tile = 0;
        Ok(())
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &[[u8; 3]; 256],
        tile_sets: &[(String, Value)],
        sounds: &[String],
        tracker_position: Option<&Value>,
    ) -> bool {
        let original = self.document.clone();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.history.is_empty(), egui::Button::new("UNDO"))
                .clicked()
            {
                self.undo();
            }
            if ui
                .add_enabled(!self.future.is_empty(), egui::Button::new("REDO"))
                .clicked()
            {
                self.redo();
            }
            ui.label(if self.display {
                "DISPLAY / SOURCE-VISIBLE".to_owned()
            } else {
                format!("{:?} / SOURCE-VISIBLE", self.kind)
            });
        });
        let before = self.document.clone();
        if self.display {
            self.palette_ui(ui, palette);
        } else {
            match self.kind {
                AssetKind::Map => self.map_ui(ui, palette, tile_sets),
                AssetKind::TileSet => self.tiles_ui(ui, palette),
                AssetKind::Font => self.font_ui(ui),
                AssetKind::Sound => self.sound_ui(ui),
                AssetKind::Music => {
                    self.tracker
                        .show(ui, &mut self.document, sounds, tracker_position)
                }
                AssetKind::Sprite | AssetKind::Animation => {}
            }
        }
        if self.document != before {
            self.history.push(before);
            if self.history.len() > 32 {
                self.history.remove(0);
            }
            self.future.clear();
        }
        for action in self.tracker.take_actions() {
            match action {
                Action::Undo => self.undo(),
                Action::Redo => self.redo(),
                action => self.tracker_actions.push(action),
            }
        }
        let changed = self.document != original;
        self.dirty |= changed;
        changed
    }

    fn map_ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &[[u8; 3]; 256],
        tile_sets: &[(String, Value)],
    ) {
        let Some(layers) = self.document["layers"].as_array_mut() else {
            ui.label("INVALID MAP DOCUMENT");
            return;
        };
        if layers.is_empty() {
            return;
        }
        self.layer = self.layer.min(layers.len() - 1);
        ui.horizontal(|ui| {
            ui.label(format!("LAYER {} / {}", self.layer + 1, layers.len()));
            if ui.button("<").clicked() {
                self.layer = self.layer.saturating_sub(1);
            }
            if ui.button(">").clicked() {
                self.layer = (self.layer + 1).min(layers.len() - 1);
            }
            if ui.button("+ LAYER").clicked() && layers.len() < 8 {
                let template = &layers[self.layer];
                let width = integer(&template["width"]).clamp(1, 256);
                let height = integer(&template["height"]).clamp(1, 256);
                let tile_set = template["tileSet"].as_str().unwrap_or("tiles");
                layers.push(json!({"width":width,"height":height,"cells":vec![0;width*height],"tileSet":tile_set}));
                self.layer = layers.len() - 1;
            }
            if ui.button("- LAYER").clicked() && layers.len() > 1 {
                layers.remove(self.layer);
                self.layer = self.layer.min(layers.len() - 1);
            }
            if ui.button("UP").clicked() && self.layer > 0 {
                layers.swap(self.layer, self.layer - 1);
                self.layer -= 1;
            }
            if ui.button("DOWN").clicked() && self.layer + 1 < layers.len() {
                layers.swap(self.layer, self.layer + 1);
                self.layer += 1;
            }
        });
        let layer = &mut layers[self.layer];
        let mut width = integer(&layer["width"]).clamp(1, 256);
        let mut height = integer(&layer["height"]).clamp(1, 256);
        ui.horizontal(|ui| {
            ui.label("SIZE");
            let mut new_width = width;
            let mut new_height = height;
            ui.add(egui::DragValue::new(&mut new_width).range(1..=256));
            ui.label("x");
            ui.add(egui::DragValue::new(&mut new_height).range(1..=256));
            if new_width != width || new_height != height {
                let old = layer["cells"].as_array().cloned().unwrap_or_default();
                let mut cells = vec![json!(0); new_width * new_height];
                for y in 0..height.min(new_height) {
                    for x in 0..width.min(new_width) {
                        cells[y * new_width + x] =
                            old.get(y * width + x).cloned().unwrap_or(json!(0));
                    }
                }
                layer["width"] = json!(new_width);
                layer["height"] = json!(new_height);
                layer["cells"] = Value::Array(cells);
                width = new_width;
                height = new_height;
            }
        });
        ui.horizontal(|ui| {
            let mut selected = layer["tileSet"].as_str().unwrap_or("").to_owned();
            egui::ComboBox::from_id_salt("map-tileset")
                .selected_text(&selected)
                .show_ui(ui, |ui| {
                    for (name, _) in tile_sets {
                        ui.selectable_value(&mut selected, name.clone(), name);
                    }
                });
            layer["tileSet"] = json!(selected);
            ui.label("TILE");
            ui.add(egui::DragValue::new(&mut self.tile).range(0..=4095));
        });
        ui.horizontal_wrapped(|ui| {
            for (mode, title) in [
                (MapMode::Paint, "PAINT"),
                (MapMode::Select, "SELECT"),
                (MapMode::Stamp, "STAMP"),
                (MapMode::Move, "MOVE"),
                (MapMode::Fill, "FILL"),
            ] {
                if ui.selectable_label(self.map_mode == mode, title).clicked() {
                    if mode == MapMode::Select {
                        self.map_anchor = None;
                    }
                    if matches!(mode, MapMode::Stamp | MapMode::Move)
                        && self.map_clipboard.is_none()
                        && let Some(region) = self.map_selection
                    {
                        self.map_clipboard = copy_map_region(layer, region);
                    }
                    self.map_mode = mode;
                }
            }
            if ui.button("COPY").clicked()
                && let Some(region) = self.map_selection
            {
                self.map_clipboard = copy_map_region(layer, region);
            }
            if ui.button("CLEAR SELECTION").clicked() {
                self.map_selection = None;
                self.map_anchor = None;
                self.map_mode = MapMode::Paint;
            }
        });
        let tile_set_name = layer["tileSet"].as_str().unwrap_or("");
        let tile_colors = tile_sets
            .iter()
            .find(|(name, _)| name == tile_set_name)
            .and_then(|(_, value)| value["tiles"].as_array());
        if let Some(tiles) = tile_colors {
            ui.horizontal_wrapped(|ui| {
                for (index, tile) in tiles.iter().enumerate().take(64) {
                    let color = color_for_tile(tile, palette);
                    let response = ui.add(
                        egui::Button::new(format!("{index:02X}"))
                            .fill(color)
                            .selected(index == self.tile),
                    );
                    if response.clicked() {
                        self.tile = index;
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("VIEW X/Y");
            ui.add(
                egui::DragValue::new(&mut self.map_origin[0]).range(0..=width.saturating_sub(1)),
            );
            ui.add(
                egui::DragValue::new(&mut self.map_origin[1]).range(0..=height.saturating_sub(1)),
            );
        });
        let Some(cells) = layer["cells"].as_array_mut() else {
            return;
        };
        let columns = width.saturating_sub(self.map_origin[0]).min(30);
        let rows = height.saturating_sub(self.map_origin[1]).min(18);
        if columns == 0 || rows == 0 {
            return;
        }
        let side = (ui.available_width() / columns as f32)
            .floor()
            .clamp(8.0, 28.0);
        let (response, painter) = ui.allocate_painter(
            egui::vec2(columns as f32 * side, rows as f32 * side),
            Sense::click_and_drag(),
        );
        let response = response.on_hover_cursor(egui::CursorIcon::Crosshair);
        for y in 0..rows {
            for x in 0..columns {
                let index = (self.map_origin[1] + y) * width + self.map_origin[0] + x;
                let tile_index = cells.get(index).and_then(unsigned).unwrap_or(0) as usize;
                let color = tile_colors
                    .and_then(|tiles| tiles.get(tile_index))
                    .map_or_else(
                        || palette[tile_index % 256],
                        |tile| rgb_for_tile(tile, palette),
                    );
                let rect = egui::Rect::from_min_size(
                    response.rect.min + egui::vec2(x as f32 * side, y as f32 * side),
                    egui::vec2(side - 1.0, side - 1.0),
                );
                painter.rect_filled(rect, 0.0, Color32::from_rgb(color[0], color[1], color[2]));
            }
        }
        if let Some([left, top, right, bottom]) = self.map_selection {
            let origin = response.rect.min;
            let x0 = (left.saturating_sub(self.map_origin[0])) as f32 * side;
            let y0 = (top.saturating_sub(self.map_origin[1])) as f32 * side;
            let x1 = (right + 1).saturating_sub(self.map_origin[0]) as f32 * side;
            let y1 = (bottom + 1).saturating_sub(self.map_origin[1]) as f32 * side;
            let selected =
                egui::Rect::from_min_max(origin + egui::vec2(x0, y0), origin + egui::vec2(x1, y1))
                    .intersect(response.rect);
            if selected.is_positive() {
                let stroke = egui::Stroke::new(2.0, Color32::from_rgb(115, 169, 209));
                painter.rect_stroke(selected, 0.0, stroke, egui::StrokeKind::Inside);
            }
        }
        if (response.dragged() || response.clicked())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let x =
                ((pointer.x - response.rect.left()) / side).floor() as usize + self.map_origin[0];
            let y =
                ((pointer.y - response.rect.top()) / side).floor() as usize + self.map_origin[1];
            if x < width && y < height {
                let index = y * width + x;
                match self.map_mode {
                    MapMode::Paint => {
                        if let Some(cell) = cells.get_mut(index) {
                            *cell = json!(self.tile);
                        }
                    }
                    MapMode::Select if response.clicked() => {
                        if let Some(anchor) = self.map_anchor.take() {
                            self.map_selection = Some([
                                anchor[0].min(x),
                                anchor[1].min(y),
                                anchor[0].max(x),
                                anchor[1].max(y),
                            ]);
                            self.map_mode = MapMode::Paint;
                        } else {
                            self.map_anchor = Some([x, y]);
                        }
                    }
                    MapMode::Fill if response.clicked() => {
                        flood_fill(cells, width, height, index, self.tile as u64);
                        self.map_mode = MapMode::Paint;
                    }
                    MapMode::Stamp | MapMode::Move if response.clicked() => {
                        if let Some((copy_width, copy_height, copied)) = &self.map_clipboard {
                            if self.map_mode == MapMode::Move
                                && let Some(selection) = self.map_selection
                            {
                                clear_map_region(cells, width, height, selection);
                            }
                            stamp_map(
                                cells,
                                width,
                                height,
                                [x, y],
                                (*copy_width, *copy_height, copied),
                            );
                            self.map_selection = Some([
                                x,
                                y,
                                (x + copy_width - 1).min(width - 1),
                                (y + copy_height - 1).min(height - 1),
                            ]);
                            self.map_mode = MapMode::Paint;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn palette_ui(&mut self, ui: &mut egui::Ui, palette: &[[u8; 3]; 256]) {
        ui.horizontal(|ui| {
            ui.label("RASTER LINE");
            ui.add(egui::Slider::new(&mut self.raster_line, 0..=143));
            ui.checkbox(&mut self.raster_default, "DEFAULT REMAP");
        });
        let line = self.raster_line;
        let Some(mut raster) = self.document["raster"].as_array().cloned() else {
            return;
        };
        let existing = raster.iter().position(|row| integer(&row["line"]) == line);
        let mut enabled = existing.is_some();
        ui.horizontal(|ui| {
            if ui.checkbox(&mut enabled, "KEYFRAME").changed() {
                if enabled {
                    let inherited = raster
                        .iter()
                        .filter(|row| integer(&row["line"]) < line)
                        .max_by_key(|row| integer(&row["line"]))
                        .and_then(|row| row["remap"].as_array().cloned())
                        .unwrap_or_else(|| (0..32).map(|n| json!(n)).collect());
                    raster.push(json!({"line":line,"scrollX":0,"scrollY":0,"remap":inherited}));
                    raster.sort_by_key(|row| integer(&row["line"]));
                } else if let Some(index) = existing {
                    raster.remove(index);
                }
            }
            if let Some(index) = existing {
                if ui.button("COPY").clicked() {
                    self.raster_clipboard = Some(raster[index].clone());
                }
                if ui.button("PASTE").clicked()
                    && let Some(row) = &self.raster_clipboard
                {
                    let mut pasted = row.clone();
                    pasted["line"] = json!(line);
                    raster[index] = pasted;
                }
            }
        });
        let current = raster.iter().position(|row| integer(&row["line"]) == line);
        if let Some(index) = current {
            for (label, key) in [("SCROLL X", "scrollX"), ("SCROLL Y", "scrollY")] {
                let mut value = integer_value(&raster[index][key], -32768, 32767).unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui
                        .add(egui::DragValue::new(&mut value).range(-32768..=32767))
                        .changed()
                    {
                        raster[index][key] = json!(value);
                    }
                });
            }
        }
        ui.horizontal(|ui| {
            ui.label("LOGICAL COLOR");
            ui.add(egui::DragValue::new(&mut self.logical_color).range(0..=31));
        });
        let key = if self.raster_default { None } else { current };
        let mut mapped = key
            .and_then(|index| unsigned(&raster[index]["remap"][self.logical_color]))
            .or_else(|| unsigned(&self.document["remap"][self.logical_color]))
            .unwrap_or(self.logical_color as u64)
            .min(255);
        ui.horizontal(|ui| {
            ui.label("PHYSICAL COLOR");
            if ui
                .add(egui::DragValue::new(&mut mapped).range(0..=255))
                .changed()
            {
                if let Some(index) = key {
                    raster[index]["remap"][self.logical_color] = json!(mapped);
                } else {
                    self.document["remap"][self.logical_color] = json!(mapped);
                }
            }
            let [r, g, b] = palette[mapped as usize];
            let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 22.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_rgb(r, g, b));
            ui.painter().rect_stroke(
                rect,
                0.0,
                egui::Stroke::new(1.0, Color32::from_rgb(109, 100, 82)),
                egui::StrokeKind::Inside,
            );
        });
        ui.horizontal_wrapped(|ui| {
            for (physical, &[r, g, b]) in palette.iter().enumerate() {
                let button = egui::Button::new("  ").fill(Color32::from_rgb(r, g, b));
                if ui.add(button).clicked() {
                    if let Some(index) = key {
                        raster[index]["remap"][self.logical_color] = json!(physical);
                    } else {
                        self.document["remap"][self.logical_color] = json!(physical);
                    }
                }
            }
        });
        ui.label(format!("{} KEYFRAMES / 144 SCANLINES", raster.len()));
        self.document["raster"] = Value::Array(raster);
    }

    fn tiles_ui(&mut self, ui: &mut egui::Ui, palette: &[[u8; 3]; 256]) {
        let Some(tiles) = self.document["tiles"].as_array() else {
            return;
        };
        if tiles.is_empty() {
            return;
        }
        self.tile = self.tile.min(tiles.len() - 1);
        ui.horizontal(|ui| {
            ui.label(format!("TILE {} / {}", self.tile, tiles.len()));
            ui.add(egui::DragValue::new(&mut self.tile).range(0..=tiles.len() - 1));
        });
        ui.horizontal(|ui| {
            if ui.button("+ TILE").clicked()
                && self.document["tiles"]
                    .as_array()
                    .is_some_and(|items| items.len() < 4096)
            {
                if let Some(items) = self.document["tiles"].as_array_mut() {
                    items.push(json!(vec![0; 64]));
                    self.tile = items.len() - 1;
                }
                if let Some(flags) = self.document["flags"].as_array_mut() {
                    flags.push(json!(0));
                }
            }
            if ui.button("- TILE").clicked()
                && self.document["tiles"]
                    .as_array()
                    .is_some_and(|items| items.len() > 1)
            {
                let removed = self.tile;
                if let Some(items) = self.document["tiles"].as_array_mut() {
                    items.remove(self.tile);
                    self.tile = self.tile.min(items.len() - 1);
                }
                if let Some(flags) = self.document["flags"].as_array_mut() {
                    if removed < flags.len() {
                        flags.remove(removed);
                    }
                }
            }
        });
        let mut flag = unsigned(&self.document["flags"][self.tile])
            .unwrap_or(0)
            .min(255);
        ui.horizontal(|ui| {
            ui.label("FLAGS");
            if ui
                .add(egui::DragValue::new(&mut flag).range(0..=255))
                .changed()
            {
                self.document["flags"][self.tile] = json!(flag);
            }
        });
        let mut color = self.logical_color;
        ui.horizontal(|ui| {
            ui.label("COLOR");
            ui.add(egui::DragValue::new(&mut color).range(0..=255));
        });
        self.logical_color = color;
        let Some(pixels) = self.document["tiles"][self.tile].as_array_mut() else {
            return;
        };
        pixel_grid(ui, pixels, 8, 8, 28.0, palette, color as u64);
    }

    fn font_ui(&mut self, ui: &mut egui::Ui) {
        let mut width = integer(&self.document["glyphWidth"]).clamp(1, 16);
        let mut height = integer(&self.document["glyphHeight"]).clamp(1, 16);
        ui.horizontal(|ui| {
            ui.label("GLYPH SIZE");
            let mut next_width = width;
            let mut next_height = height;
            ui.add(egui::DragValue::new(&mut next_width).range(1..=16));
            ui.label("x");
            ui.add(egui::DragValue::new(&mut next_height).range(1..=16));
            if next_width != width || next_height != height {
                if let Some(glyphs) = self.document["glyphs"].as_array_mut() {
                    for glyph in glyphs {
                        if let Some(pixels) = glyph["pixels"].as_array_mut() {
                            *pixels = resize_mask(pixels, width, height, next_width, next_height);
                        }
                    }
                }
                self.document["glyphWidth"] = json!(next_width);
                self.document["glyphHeight"] = json!(next_height);
                self.document["baseline"] =
                    json!(integer(&self.document["baseline"]).min(next_height - 1));
                width = next_width;
                height = next_height;
            }
        });
        let fallback = integer(&self.document["missingGlyph"]);
        let Some(glyphs) = self.document["glyphs"].as_array_mut() else {
            return;
        };
        if glyphs.is_empty() {
            return;
        }
        self.glyph = self.glyph.min(glyphs.len() - 1);
        ui.horizontal(|ui| {
            ui.label("GLYPH");
            ui.add(egui::DragValue::new(&mut self.glyph).range(0..=glyphs.len() - 1));
            if ui.button("+ GLYPH").clicked() && glyphs.len() < 256 {
                let code =
                    (0..=255).find(|code| glyphs.iter().all(|g| integer(&g["code"]) != *code));
                if let Some(code) = code {
                    glyphs.push(json!({"code":code,"pixels":vec![0;width*height]}));
                    glyphs.sort_by_key(|g| integer(&g["code"]));
                    self.glyph = glyphs
                        .iter()
                        .position(|g| integer(&g["code"]) == code)
                        .unwrap_or(0);
                }
            }
            if ui.button("- GLYPH").clicked() && glyphs.len() > 1 {
                if integer(&glyphs[self.glyph]["code"]) != fallback {
                    glyphs.remove(self.glyph);
                    self.glyph = self.glyph.min(glyphs.len() - 1);
                }
            }
        });
        ui.label(format!(
            "CODE {} / {}x{}",
            integer(&glyphs[self.glyph]["code"]),
            width,
            height
        ));
        ui.horizontal(|ui| {
            for (title, transform) in [
                ("FLIP H", "flip-h"),
                ("FLIP V", "flip-v"),
                ("ROTATE", "rotate"),
            ] {
                if ui.button(title).clicked()
                    && let Some(pixels) = glyphs[self.glyph]["pixels"].as_array_mut()
                {
                    *pixels = transform_mask(pixels, width, height, transform);
                }
            }
        });
        let Some(pixels) = glyphs[self.glyph]["pixels"].as_array_mut() else {
            return;
        };
        let two_colors = [[23, 20, 31], [244, 229, 189]];
        pixel_grid(ui, pixels, width, height, 24.0, &two_colors, 1);
        ui.label("LEFT DRAW / RIGHT ERASE");
        for (label, key, max) in [
            ("BASELINE", "baseline", height.saturating_sub(1)),
            ("ADVANCE X", "advanceX", 32),
            ("ADVANCE Y", "advanceY", 32),
            ("FALLBACK", "missingGlyph", 255),
        ] {
            let mut value = integer(&self.document[key]);
            ui.horizontal(|ui| {
                ui.label(label);
                if ui
                    .add(egui::DragValue::new(&mut value).range(0..=max))
                    .changed()
                {
                    self.document[key] = json!(value);
                }
            });
        }
        ui.horizontal(|ui| {
            ui.label("PREVIEW TEXT");
            ui.text_edit_singleline(&mut self.font_preview);
        });
        draw_font_preview(ui, &self.document, &self.font_preview);
    }

    fn sound_ui(&mut self, ui: &mut egui::Ui) {
        if !self.document.is_object() {
            ui.label("INVALID SOUND OBJECT / EDIT SOURCE");
            return;
        }
        let mut wave = self.document["waveform"]
            .as_str()
            .unwrap_or("INVALID / SELECT WAVEFORM")
            .to_owned();
        let previous_wave = wave.clone();
        egui::ComboBox::from_id_salt("waveform")
            .height(5.0 * (ui.spacing().interact_size.y + ui.spacing().item_spacing.y) + 12.0)
            .selected_text(&wave)
            .show_ui(ui, |ui| {
                for choice in ["pulse", "triangle", "saw", "noise", "wavetable"] {
                    ui.selectable_value(&mut wave, choice.to_owned(), choice);
                }
            });
        if wave != previous_wave {
            self.document["waveform"] = json!(wave);
            if wave == "pulse"
                && !self.document["duty"]
                    .as_f64()
                    .is_some_and(|duty| duty > 0.0 && duty < 1.0)
            {
                self.document["duty"] = json!(0.5);
            }
            if wave == "wavetable" && !valid_wavetable(&self.document["wavetable"]) {
                self.document["wavetable"] = json!([0.0, 1.0, 0.0, -1.0]);
            }
        }
        for (label, path, low, high) in [
            ("NOTE", "note", 0_u32, 127_u32),
            ("FRAMES", "durationFrames", 1, 3600),
            ("ATTACK", "envelope.attackFrames", 0, 3600),
            ("DECAY", "envelope.decayFrames", 0, 3600),
            ("RELEASE", "envelope.releaseFrames", 0, 3600),
            ("VIBRATO PERIOD", "pitch.vibratoPeriodFrames", 0, 3600),
        ] {
            let Some(number) = number_path(&self.document, path).filter(|number| {
                number.fract() == 0.0 && *number >= 0.0 && *number <= f64::from(u32::MAX)
            }) else {
                ui.label(format!("{label} INVALID / EDIT SOURCE"));
                continue;
            };
            // Integer backing values cannot write fractional MIDI/frame counts.
            // Valid integral JSON floats remain untouched until explicitly edited.
            let mut value = number as u32;
            ui.push_id(path, |ui| {
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui
                        .add(
                            egui::DragValue::new(&mut value)
                                .speed(1)
                                .custom_parser(|text| text.parse::<u32>().ok().map(f64::from))
                                .range(low..=high)
                                .clamp_existing_to_range(false),
                        )
                        .changed()
                    {
                        set_value_path(&mut self.document, path, json!(value));
                    }
                });
            });
        }
        for (label, path, low, high) in [
            ("VOLUME", "volume", 0.0, 1.0),
            ("PAN", "pan", -1.0, 1.0),
            ("DUTY", "duty", f64::from_bits(1), 1.0_f64.next_down()),
            ("SUSTAIN", "envelope.sustainLevel", 0.0, 1.0),
            ("SLIDE", "pitch.slideSemitonesPerFrame", -f64::MAX, f64::MAX),
            (
                "VIBRATO DEPTH",
                "pitch.vibratoDepthSemitones",
                -f64::MAX,
                f64::MAX,
            ),
        ] {
            if path == "duty" && wave != "pulse" {
                continue;
            }
            let Some(mut value) = number_path(&self.document, path) else {
                ui.label(format!("{label} INVALID / EDIT SOURCE"));
                continue;
            };
            ui.push_id(path, |ui| {
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui
                        .add(
                            egui::DragValue::new(&mut value)
                                .speed(0.01)
                                .custom_formatter(|value, _| value.to_string())
                                .range(low..=high)
                                .clamp_existing_to_range(false),
                        )
                        .changed()
                        && value.is_finite()
                    {
                        set_value_path(&mut self.document, path, json!(value));
                    }
                });
            });
        }
        if wave == "wavetable" {
            self.wavetable_ui(ui);
        }
        ui.label("CLICK VALUE TO TYPE / DRAG TO ADJUST / PITCH VALIDATED BY CORE");
    }

    fn wavetable_ui(&mut self, ui: &mut egui::Ui) {
        if !valid_wavetable(&self.document["wavetable"]) {
            ui.label("INVALID WAVETABLE / EDIT SOURCE (4–32 SAMPLES IN -1..1)");
            return;
        }
        let table = self.document["wavetable"].as_array_mut().unwrap();
        ui.horizontal(|ui| {
            ui.label(format!("WAVETABLE / {} SAMPLES", table.len()));
            if ui
                .add_enabled(table.len() < 32, egui::Button::new("ADD SAMPLE"))
                .clicked()
            {
                table.push(json!(0.0));
            }
            if ui
                .add_enabled(table.len() > 4, egui::Button::new("REMOVE SAMPLE"))
                .clicked()
            {
                table.pop();
            }
        });
        for (index, sample) in table.iter_mut().enumerate() {
            ui.push_id(("wavetable", index), |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("SAMPLE {index:02}"));
                    let mut value = sample.as_f64().unwrap();
                    if ui
                        .add(
                            egui::DragValue::new(&mut value)
                                .speed(0.01)
                                .custom_formatter(|value, _| value.to_string())
                                .range(-1.0..=1.0),
                        )
                        .changed()
                    {
                        *sample = json!(value);
                    }
                });
            });
        }
    }
}

fn integer(value: &Value) -> usize {
    unsigned(value).unwrap_or(0) as usize
}

fn unsigned(value: &Value) -> Option<u64> {
    integer_value(value, 0, 65535).map(|number| number as u64)
}

fn number_path(value: &Value, path: &str) -> Option<f64> {
    path.split('.')
        .fold(value, |current, key| &current[key])
        .as_f64()
        .filter(|number| number.is_finite())
}

fn set_value_path(value: &mut Value, path: &str, number: Value) {
    let mut parts = path.split('.');
    let first = parts.next().unwrap_or("");
    if let Some(second) = parts.next() {
        value[first][second] = number;
    } else {
        value[first] = number;
    }
}

fn valid_wavetable(value: &Value) -> bool {
    value.as_array().is_some_and(|table| {
        (4..=32).contains(&table.len())
            && table.iter().all(|sample| {
                sample
                    .as_f64()
                    .is_some_and(|number| (-1.0..=1.0).contains(&number))
            })
    })
}

fn rgb_for_tile(tile: &Value, palette: &[[u8; 3]; 256]) -> [u8; 3] {
    let index = tile
        .as_array()
        .and_then(|pixels| pixels.get(27))
        .and_then(unsigned)
        .unwrap_or(0) as usize;
    palette[index.min(255)]
}

fn color_for_tile(tile: &Value, palette: &[[u8; 3]; 256]) -> Color32 {
    let [r, g, b] = rgb_for_tile(tile, palette);
    Color32::from_rgb(r, g, b)
}

fn flood_fill(cells: &mut [Value], width: usize, height: usize, start: usize, tile: u64) {
    let Some(target) = cells.get(start).and_then(unsigned) else {
        return;
    };
    if target == tile {
        return;
    }
    let mut stack = vec![start];
    while let Some(index) = stack.pop() {
        if unsigned(&cells[index]) != Some(target) {
            continue;
        }
        cells[index] = json!(tile);
        let x = index % width;
        let y = index / width;
        if x > 0 {
            stack.push(index - 1);
        }
        if x + 1 < width {
            stack.push(index + 1);
        }
        if y > 0 {
            stack.push(index - width);
        }
        if y + 1 < height {
            stack.push(index + width);
        }
    }
}

fn copy_map_region(layer: &Value, region: [usize; 4]) -> Option<MapClipboard> {
    let width = integer(&layer["width"]);
    let height = integer(&layer["height"]);
    let cells = layer["cells"].as_array()?;
    let [left, top, right, bottom] = region;
    if width == 0 || height == 0 || left > right || top > bottom || left >= width || top >= height {
        return None;
    }
    let copy_width = right.min(width - 1) - left + 1;
    let copy_height = bottom.min(height - 1) - top + 1;
    let copied = (0..copy_height)
        .flat_map(|y| {
            (0..copy_width).map(move |x| {
                cells
                    .get((top + y) * width + left + x)
                    .cloned()
                    .unwrap_or(json!(0))
            })
        })
        .collect();
    Some((copy_width, copy_height, copied))
}

fn clear_map_region(cells: &mut [Value], width: usize, height: usize, region: [usize; 4]) {
    let [left, top, right, bottom] = region;
    for y in top..=bottom.min(height.saturating_sub(1)) {
        for x in left..=right.min(width.saturating_sub(1)) {
            if let Some(cell) = cells.get_mut(y * width + x) {
                *cell = json!(0);
            }
        }
    }
}

fn stamp_map(
    cells: &mut [Value],
    width: usize,
    height: usize,
    destination: [usize; 2],
    clipboard: (usize, usize, &[Value]),
) {
    let [x, y] = destination;
    let (copy_width, copy_height, copied) = clipboard;
    for offset_y in 0..copy_height.min(height.saturating_sub(y)) {
        for offset_x in 0..copy_width.min(width.saturating_sub(x)) {
            if let (Some(target), Some(source)) = (
                cells.get_mut((y + offset_y) * width + x + offset_x),
                copied.get(offset_y * copy_width + offset_x),
            ) {
                *target = source.clone();
            }
        }
    }
}

fn resize_mask(
    pixels: &[Value],
    width: usize,
    height: usize,
    next_width: usize,
    next_height: usize,
) -> Vec<Value> {
    let mut result = vec![json!(0); next_width * next_height];
    for y in 0..height.min(next_height) {
        for x in 0..width.min(next_width) {
            result[y * next_width + x] = pixels.get(y * width + x).cloned().unwrap_or(json!(0));
        }
    }
    result
}

fn transform_mask(pixels: &[Value], width: usize, height: usize, transform: &str) -> Vec<Value> {
    let mut result = vec![json!(0); width * height];
    for y in 0..height {
        for x in 0..width {
            let (source_x, source_y) = match transform {
                "flip-h" => (width - 1 - x, y),
                "flip-v" => (x, height - 1 - y),
                "rotate" => (
                    ((y as f32 + 0.5) * width as f32 / height as f32).floor() as usize,
                    (((width - 1 - x) as f32 + 0.5) * height as f32 / width as f32).floor()
                        as usize,
                ),
                _ => (x, y),
            };
            result[y * width + x] = pixels
                .get(source_y * width + source_x)
                .cloned()
                .unwrap_or(json!(0));
        }
    }
    result
}

fn draw_font_preview(ui: &mut egui::Ui, document: &Value, sample: &str) {
    let width = integer(&document["glyphWidth"]).clamp(1, 16);
    let height = integer(&document["glyphHeight"]).clamp(1, 16);
    let advance_x = integer(&document["advanceX"]).clamp(1, 32);
    let advance_y = integer(&document["advanceY"]).clamp(1, 32);
    let fallback = integer(&document["missingGlyph"]);
    let Some(glyphs) = document["glyphs"].as_array() else {
        return;
    };
    let scale = 3.0;
    let (response, painter) = ui.allocate_painter(
        egui::vec2(
            ui.available_width().max(120.0),
            (advance_y.max(height) * 3) as f32 * scale,
        ),
        Sense::hover(),
    );
    painter.rect_filled(response.rect, 0.0, Color32::from_rgb(23, 20, 31));
    let mut pen_x = 4.0;
    let mut pen_y = 4.0;
    for character in sample.chars().take(120) {
        if character == '\n' {
            pen_x = 4.0;
            pen_y += advance_y as f32 * scale;
            continue;
        }
        if pen_x + width as f32 * scale > response.rect.width() {
            pen_x = 4.0;
            pen_y += advance_y as f32 * scale;
        }
        if pen_y + height as f32 * scale > response.rect.height() {
            break;
        }
        let code = character as usize;
        let glyph = glyphs
            .iter()
            .find(|glyph| integer(&glyph["code"]) == code)
            .or_else(|| {
                glyphs
                    .iter()
                    .find(|glyph| integer(&glyph["code"]) == fallback)
            });
        if let Some(glyph) = glyph {
            for y in 0..height {
                for x in 0..width {
                    if unsigned(&glyph["pixels"][y * width + x]).unwrap_or(0) != 0 {
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                response.rect.min
                                    + egui::vec2(
                                        pen_x + x as f32 * scale,
                                        pen_y + y as f32 * scale,
                                    ),
                                egui::vec2(scale, scale),
                            ),
                            0.0,
                            Color32::from_rgb(244, 229, 189),
                        );
                    }
                }
            }
        }
        pen_x += advance_x as f32 * scale;
    }
}

fn pixel_grid<const N: usize>(
    ui: &mut egui::Ui,
    pixels: &mut [Value],
    width: usize,
    height: usize,
    side: f32,
    palette: &[[u8; 3]; N],
    color: u64,
) {
    let (response, painter) = ui.allocate_painter(
        egui::vec2(width as f32 * side, height as f32 * side),
        Sense::click_and_drag(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::Crosshair);
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let selected = pixels.get(index).and_then(unsigned).unwrap_or(0) as usize;
            let [r, g, b] = palette[selected.min(N - 1)];
            let rect = egui::Rect::from_min_size(
                response.rect.min + egui::vec2(x as f32 * side, y as f32 * side),
                egui::vec2(side - 1.0, side - 1.0),
            );
            painter.rect_filled(rect, 0.0, Color32::from_rgb(r, g, b));
        }
    }
    let erase = response.clicked_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Secondary);
    if (response.clicked() || response.dragged() || erase)
        && let Some(pointer) = response.interact_pointer_pos()
    {
        let x = ((pointer.x - response.rect.left()) / side).floor() as usize;
        let y = ((pointer.y - response.rect.top()) / side).floor() as usize;
        if x < width
            && y < height
            && let Some(pixel) = pixels.get_mut(y * width + x)
        {
            *pixel = json!(if erase { 0 } else { color });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AssetWorkbench, clear_map_region, copy_map_region, flood_fill, resize_mask, stamp_map,
        transform_mask,
    };
    use modl_core::AssetKind;
    use serde_json::json;

    #[test]
    fn map_fill_stays_inside_its_region() {
        let mut cells = vec![json!(0), json!(0), json!(2), json!(0)];
        flood_fill(&mut cells, 2, 2, 0, 3);
        assert_eq!(cells, vec![json!(3), json!(3), json!(2), json!(3)]);
    }

    #[test]
    fn map_selection_can_copy_move_and_clip_at_edges() {
        let layer = json!({"width":3,"height":2,"cells":[1,2,3,4,5,6]});
        let (copy_width, copy_height, copied) = copy_map_region(&layer, [1, 0, 2, 1]).unwrap();
        assert_eq!(copied, vec![json!(2), json!(3), json!(5), json!(6)]);
        let mut cells = layer["cells"].as_array().unwrap().clone();
        clear_map_region(&mut cells, 3, 2, [1, 0, 2, 1]);
        stamp_map(&mut cells, 3, 2, [0, 0], (copy_width, copy_height, &copied));
        assert_eq!(
            cells,
            vec![json!(2), json!(3), json!(0), json!(5), json!(6), json!(0)]
        );
    }

    #[test]
    fn font_mask_resize_and_flip_preserve_pixels() {
        let pixels = vec![json!(1), json!(0), json!(0), json!(1)];
        assert_eq!(
            resize_mask(&pixels, 2, 2, 3, 2),
            vec![json!(1), json!(0), json!(0), json!(0), json!(1), json!(0)]
        );
        assert_eq!(
            transform_mask(&pixels, 2, 2, "flip-h"),
            vec![json!(0), json!(1), json!(1), json!(0)]
        );
    }

    #[test]
    fn tile_atlas_png_round_trips_palette_indices() {
        let palette = crate::sprite::palette();
        let first = (0..64)
            .map(|index| if index == 0 { 0 } else { 7 })
            .collect::<Vec<_>>();
        let second = vec![10; 64];
        let source = json!({"revision":2,"kind":"tile_set","tiles":[first,second],"flags":[0,1]})
            .to_string();
        let editor = AssetWorkbench::new(AssetKind::TileSet, &source).unwrap();
        let png = editor.export_tile_png(&palette).unwrap();
        let mut imported = AssetWorkbench::new(AssetKind::TileSet, &source).unwrap();
        imported.import_tile_png(&png).unwrap();
        assert_eq!(imported.document["tiles"], editor.document["tiles"]);
        assert_eq!(imported.document["flags"], editor.document["flags"]);
    }

    #[test]
    fn integer_asset_spellings_export_real_tile_colors_and_fill_equivalent_cells() {
        let colors = crate::sprite::palette();
        let document =
            json!({"revision":2.0,"kind":"tile_set","tiles":[vec![255.0;64]],"flags":[7.0]});
        let source = document.to_string();
        let mut editor = AssetWorkbench::new(AssetKind::TileSet, &source).unwrap();
        let bytes = editor.export_tile_png(&colors).unwrap();
        let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(
            image.get_pixel(0, 0).0,
            [colors[255][0], colors[255][1], colors[255][2], 255]
        );
        assert_eq!(editor.document, document);
        assert!(!editor.dirty && editor.history.is_empty());
        for bad in [
            json!(1.5),
            json!(-1.0),
            json!(256.0),
            json!(false),
            json!("7"),
        ] {
            editor.document["tiles"][0][0] = bad;
            assert!(
                editor.export_tile_png(&colors).is_err(),
                "cannot silently export invalid colors"
            );
        }
        let mut cells = vec![json!(1.0), json!(1), json!(2.0), json!(1.0)];
        flood_fill(&mut cells, 2, 2, 0, 3);
        assert_eq!(cells, vec![json!(3), json!(3), json!(2.0), json!(3)]);
    }

    #[test]
    fn integer_asset_spellings_render_valid_banks_identically_without_source_edits() {
        use eframe::egui;
        use serde_json::Value;
        fn floats(value: &Value) -> Value {
            match value {
                Value::Number(number) => json!(number.as_f64().unwrap()),
                Value::Array(values) => Value::Array(values.iter().map(floats).collect()),
                Value::Object(values) => Value::Object(
                    values
                        .iter()
                        .map(|(key, value)| (key.clone(), floats(value)))
                        .collect(),
                ),
                _ => value.clone(),
            }
        }
        let tiles =
            json!({"revision":2,"kind":"tile_set","tiles":[vec![23;64],vec![11;64]],"flags":[3,0]});
        let map = json!({"revision":1,"kind":"map","layers":[{"width":2,"height":2,"cells":[0,1,1,0],"tileSet":"tiles"}]});
        let font = json!({"revision":1,"kind":"font","glyphWidth":2,"glyphHeight":2,"baseline":1,"advanceX":3,"advanceY":3,"missingGlyph":63,"glyphs":[{"code":63,"pixels":[1,0,0,1]},{"code":65,"pixels":[0,1,1,0]}]});
        let display = json!({"revision":2,"kind":"display","remap":(0..32).collect::<Vec<_>>(),"raster":[{"line":0,"scrollX":-3,"scrollY":8,"remap":(0..32).map(|number|255-number).collect::<Vec<_>>()}]});
        let assets = [
            ("tiles", AssetKind::TileSet, tiles.clone()),
            ("map", AssetKind::Map, map),
            ("font", AssetKind::Font, font),
        ];
        let mut project =
            modl_core::parse_workcart(include_str!("../../../templates/blank/cart.m01w")).unwrap();
        for (name, kind, value) in &assets {
            let path = format!("assets/{name}.json");
            project.manifest.assets.insert(
                (*name).to_owned(),
                modl_core::ProjectAsset {
                    kind: *kind,
                    path: path.clone(),
                },
            );
            project
                .files
                .insert(path, serde_json::to_vec(&floats(value)).unwrap());
        }
        project.manifest.display = Some("assets/display.json".to_owned());
        project.files.insert(
            "assets/display.json".to_owned(),
            serde_json::to_vec(&floats(&display)).unwrap(),
        );
        let report = modl_core::assets::analyze_project_assets(&project.manifest, &project.files);
        assert!(report.is_valid(), "{:?}", report.diagnostics);
        let workcart =
            modl_core::encode_workcart(&project.manifest, &project.files, &[], &[]).unwrap();
        modl_core::game::pack_game_workcart(&workcart).unwrap();
        let render = |document: &Value, kind, display: bool, tile_bank: &Value| {
            let context = egui::Context::default();
            crate::ui_assets::install_font(&context);
            crate::ui_theme::install(&context);
            let mut editor = if display {
                AssetWorkbench::new_display(&document.to_string())
            } else {
                AssetWorkbench::new(kind, &document.to_string())
            }
            .unwrap();
            let mut evidence = (Vec::new(), Vec::new());
            for _ in 0..3 {
                let output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 1200.0),
                        )),
                        time: Some(0.0),
                        ..Default::default()
                    },
                    |ui| {
                        assert!(!editor.ui(
                            ui,
                            &crate::sprite::palette(),
                            &[("tiles".to_owned(), tile_bank.clone())],
                            &[],
                            None
                        ));
                    },
                );
                evidence.0.clear();
                evidence.1.clear();
                for shape in &output.shapes {
                    match &shape.shape {
                        egui::Shape::Rect(rect) => evidence.0.push((rect.rect, rect.fill)),
                        egui::Shape::Text(text) => evidence.1.push(text.galley.job.text.clone()),
                        _ => {}
                    }
                }
                output.drop_without_applying_deltas();
            }
            assert_eq!(editor.document, *document);
            assert!(!editor.dirty && editor.history.is_empty() && editor.future.is_empty());
            evidence
        };
        for (name, kind, value) in assets
            .into_iter()
            .chain([("display", AssetKind::Map, display)])
        {
            assert!(
                render(&value, kind, name == "display", &tiles)
                    == render(&floats(&value), kind, name == "display", &floats(&tiles)),
                "{name} must display the same valid numbers/colors"
            );
        }
    }

    #[test]
    fn structured_editor_keeps_source_visible() {
        let workbench =
            AssetWorkbench::new(AssetKind::Sound, r#"{"kind":"sound","note":60}"#).unwrap();
        assert!(workbench.source().unwrap().contains("\"note\":60"));
    }

    fn sound() -> serde_json::Value {
        json!({"revision":1,"kind":"sound","waveform":"pulse","note":60.0,
            "durationFrames":3600.0,"volume":0.6,"pan":0,"duty":0.000_001,
            "envelope":{"attackFrames":3600.0,"decayFrames":240.0,"sustainLevel":0.65,"releaseFrames":180.0,"memo":"keep envelope"},
            "pitch":{"slideSemitonesPerFrame":0.0,"vibratoDepthSemitones":24.0,"vibratoPeriodFrames":3600.0},
            "wavetable":[0.0,0.5,0.0,-0.5],"memo":"keep patch"})
    }

    #[test]
    fn opening_sound_controls_never_clamps_valid_values_or_repairs_invalid_source() {
        use eframe::egui;
        let mut documents = Vec::new();
        for waveform in ["pulse", "triangle", "saw", "noise", "wavetable"] {
            let mut document = sound();
            document["waveform"] = json!(waveform);
            documents.push(document);
        }
        let mut missing = sound();
        missing.as_object_mut().unwrap().remove("waveform");
        documents.push(missing);
        let mut malformed = sound();
        malformed["envelope"] = json!("invalid but recoverable");
        malformed["note"] = json!(60.5);
        malformed["pan"] = json!(2.0);
        malformed["pitch"] = json!(false);
        documents.extend([malformed, json!([]), json!(null)]);
        for original in documents {
            let context = egui::Context::default();
            crate::ui_assets::install_font(&context);
            crate::ui_theme::install(&context);
            let mut editor = AssetWorkbench::new(AssetKind::Sound, &original.to_string()).unwrap();
            for _ in 0..4 {
                context
                    .run_ui(egui::RawInput::default(), |ui| {
                        assert!(!editor.ui(ui, &crate::sprite::palette(), &[], &[], None));
                    })
                    .drop_without_applying_deltas();
            }
            assert_eq!(editor.document, original);
            assert!(!editor.dirty);
            assert!(editor.history.is_empty() && editor.future.is_empty());
        }
    }

    fn sound_frame(
        editor: &mut AssetWorkbench,
        context: &eframe::egui::Context,
        events: Vec<eframe::egui::Event>,
    ) -> Vec<(String, eframe::egui::Rect)> {
        use eframe::egui;
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 1600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                editor.ui(ui, &crate::sprite::palette(), &[], &[], None);
            },
        );
        let labels = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape
                    && shape
                        .clip_rect
                        .contains(text.pos + text.galley.size() * 0.5)
                {
                    Some((
                        text.galley.job.text.clone(),
                        egui::Rect::from_min_size(text.pos, text.galley.size()),
                    ))
                } else {
                    None
                }
            })
            .collect();
        output.drop_without_applying_deltas();
        labels
    }

    fn sound_click(
        editor: &mut AssetWorkbench,
        context: &eframe::egui::Context,
        position: eframe::egui::Pos2,
    ) {
        use eframe::egui;
        sound_frame(editor, context, vec![egui::Event::PointerMoved(position)]);
        for pressed in [true, false] {
            sound_frame(
                editor,
                context,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        sound_frame(editor, context, Vec::new());
    }

    fn sound_type(
        editor: &mut AssetWorkbench,
        context: &eframe::egui::Context,
        label: &str,
        number: &str,
    ) {
        use eframe::egui;
        let labels = sound_frame(editor, context, Vec::new());
        let row = labels.iter().find(|(text, _)| text == label).unwrap().1;
        let value = labels
            .iter()
            .filter(|(_, rect)| (rect.top() - row.top()).abs() < 3.0 && rect.left() > row.right())
            .min_by(|(_, left), (_, right)| left.left().total_cmp(&right.left()))
            .unwrap()
            .1;
        sound_click(editor, context, value.center());
        sound_frame(
            editor,
            context,
            vec![
                egui::Event::Text(number.to_owned()),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        sound_frame(editor, context, Vec::new());
    }

    #[test]
    fn real_sound_controls_write_integers_and_preserve_metadata_with_per_edit_undo() {
        use eframe::egui;
        let context = egui::Context::default();
        crate::ui_assets::install_font(&context);
        crate::ui_theme::install(&context);
        let mut editor = AssetWorkbench::new(AssetKind::Sound, &sound().to_string()).unwrap();
        for _ in 0..3 {
            sound_frame(&mut editor, &context, Vec::new());
        }
        let original = editor.document.clone();
        sound_type(&mut editor, &context, "DUTY", "");
        assert_eq!(
            editor.document, original,
            "focus/Enter must preserve full float precision"
        );
        sound_type(&mut editor, &context, "NOTE", "61.75");
        assert_eq!(
            editor.document, original,
            "fractional MIDI input must not be admitted"
        );
        assert!(editor.history.is_empty());
        for (label, path, number) in [
            ("NOTE", "note", 62),
            ("FRAMES", "durationFrames", 3599),
            ("ATTACK", "envelope.attackFrames", 240),
            ("DECAY", "envelope.decayFrames", 3600),
            ("RELEASE", "envelope.releaseFrames", 3600),
            ("VIBRATO PERIOD", "pitch.vibratoPeriodFrames", 240),
        ] {
            let before = editor.document.clone();
            sound_type(&mut editor, &context, label, &number.to_string());
            let edited = editor.document.clone();
            assert_eq!(
                path.split('.')
                    .fold(&edited, |value, key| &value[key])
                    .as_u64(),
                Some(number)
            );
            assert_ne!(edited, before);
            assert_eq!(edited["memo"], "keep patch");
            assert_eq!(edited["envelope"]["memo"], "keep envelope");
            editor.undo();
            assert_eq!(editor.document, before);
            editor.redo();
            assert_eq!(editor.document, edited);
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/api-tour/cart.m01w");
        let mut project = crate::project::NativeProject::open(&path).unwrap();
        let original_disk = std::fs::read(&path).unwrap();
        project.files.insert(
            "assets/beep.m01s".to_owned(),
            editor.source().unwrap().into_bytes(),
        );
        let report = modl_core::assets::analyze_project_assets(&project.manifest, &project.files);
        assert!(report.is_valid(), "{:?}", report.diagnostics);
        project.pack().unwrap();
        let bank =
            crate::live_audio::project_bank(&project, "beep", &editor.document, None).unwrap();
        let mut preview = crate::live_audio::AudioPreview::new();
        preview.audition(bank, "beep", 62.0, 1.0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(7);
        while preview.packet.is_null() {
            preview.poll(&context, None);
            assert!(std::time::Instant::now() < deadline, "{}", preview.status);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(preview.packet["activeVoices"], 1);
        assert_eq!(preview.packet["voices"][0]["note"], 62);
        preview.stop();
        assert_eq!(std::fs::read(path).unwrap(), original_disk);
    }

    fn choose_sound_wave(editor: &mut AssetWorkbench, context: &eframe::egui::Context, wave: &str) {
        let current = editor.document["waveform"].as_str().unwrap().to_owned();
        let labels = sound_frame(editor, context, Vec::new());
        let selector = labels.iter().find(|(text, _)| text == &current).unwrap().1;
        sound_click(editor, context, selector.center());
        let choices = sound_frame(editor, context, Vec::new());
        let choice = choices.iter().find(|(text, _)| text == wave).unwrap().1;
        sound_click(editor, context, choice.center());
        assert_eq!(editor.document["waveform"], wave, "choices: {choices:?}");
    }

    #[test]
    fn waveform_and_wavetable_controls_are_explicit_valid_and_undoable() {
        use eframe::egui;
        let context = egui::Context::default();
        crate::ui_assets::install_font(&context);
        crate::ui_theme::install(&context);
        let mut document = sound();
        document["waveform"] = json!("triangle");
        document.as_object_mut().unwrap().remove("wavetable");
        document.as_object_mut().unwrap().remove("duty");
        let mut editor = AssetWorkbench::new(AssetKind::Sound, &document.to_string()).unwrap();
        for _ in 0..3 {
            sound_frame(&mut editor, &context, Vec::new());
        }
        choose_sound_wave(&mut editor, &context, "wavetable");
        assert!(super::valid_wavetable(&editor.document["wavetable"]));
        assert_eq!(editor.document["wavetable"].as_array().unwrap().len(), 4);
        assert_eq!(editor.document["memo"], document["memo"]);
        let with_table = editor.document.clone();
        let labels = sound_frame(&mut editor, &context, Vec::new());
        let remove = labels
            .iter()
            .find(|(label, _)| label == "REMOVE SAMPLE")
            .unwrap()
            .1;
        sound_click(&mut editor, &context, remove.center());
        assert_eq!(
            editor.document, with_table,
            "four samples is the current minimum"
        );
        editor.undo();
        assert_eq!(editor.document, document);
        editor.redo();
        assert_eq!(editor.document, with_table);
        sound_type(&mut editor, &context, "SAMPLE 00", "0.25");
        assert_eq!(editor.document["wavetable"][0], 0.25);
        let edited_table = editor.document["wavetable"].clone();
        let labels = sound_frame(&mut editor, &context, Vec::new());
        let add = labels
            .iter()
            .find(|(label, _)| label == "ADD SAMPLE")
            .unwrap()
            .1;
        sound_click(&mut editor, &context, add.center());
        assert_eq!(editor.document["wavetable"].as_array().unwrap().len(), 5);
        editor.undo();
        assert_eq!(editor.document["wavetable"], edited_table);
        choose_sound_wave(&mut editor, &context, "pulse");
        assert_eq!(editor.document["duty"], 0.5);
        assert_eq!(editor.document["wavetable"], edited_table);
        choose_sound_wave(&mut editor, &context, "wavetable");
        assert_eq!(editor.document["wavetable"], edited_table);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/api-tour/cart.m01w");
        let mut project = crate::project::NativeProject::open(&path).unwrap();
        project.files.insert(
            "assets/beep.m01s".to_owned(),
            editor.source().unwrap().into_bytes(),
        );
        project.pack().unwrap();
        editor.document["wavetable"] = json!(vec![0.5; 32]);
        let full = editor.document.clone();
        let labels = sound_frame(&mut editor, &context, Vec::new());
        let add = labels
            .iter()
            .find(|(label, _)| label == "ADD SAMPLE")
            .unwrap()
            .1;
        sound_click(&mut editor, &context, add.center());
        assert_eq!(editor.document, full, "32 samples is the current maximum");
        let labels = sound_frame(&mut editor, &context, Vec::new());
        let remove = labels
            .iter()
            .find(|(label, _)| label == "REMOVE SAMPLE")
            .unwrap()
            .1;
        sound_click(&mut editor, &context, remove.center());
        assert_eq!(editor.document["wavetable"].as_array().unwrap().len(), 31);
        editor.undo();
        assert_eq!(editor.document, full);
    }

    #[test]
    fn native_tracker_keyboard_undo_redo_reports_changes_and_round_trips_optional_metadata() {
        use eframe::egui::{self, Key, Modifiers};
        let source = json!({"revision":1,"kind":"music","framesPerRow":240,"loop":true,"order":["A"],"patterns":{"A":{"rows":vec![vec![serde_json::Value::Null;8];16],"memo":"preserve pattern"}},"memo":"preserve song"}).to_string();
        let mut workbench = AssetWorkbench::new(AssetKind::Music, &source).unwrap();
        let original = workbench.document.clone();
        let context = egui::Context::default();
        crate::ui_assets::install_font(&context);
        crate::ui_theme::install(&context);
        let mut frame = |events: Vec<egui::Event>, focus: bool| {
            let mut changed = false;
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1280.0, 800.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        if focus {
                            ui.memory_mut(|memory| {
                                memory.request_focus(ui.make_persistent_id("tracker-grid"))
                            });
                        }
                        changed = workbench.ui(
                            ui,
                            &crate::sprite::palette(),
                            &[],
                            &["beep".to_owned()],
                            None,
                        );
                    },
                )
                .drop_without_applying_deltas();
            (
                changed,
                workbench.document.clone(),
                workbench.source().unwrap(),
            )
        };
        let key = |key, modifiers| {
            vec![
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers,
                },
            ]
        };
        assert!(!frame(Vec::new(), true).0);
        let (changed, edited, text) = frame(key(Key::Z, Modifiers::NONE), false);
        assert!(changed);
        assert_eq!(edited["patterns"]["A"]["rows"][0][0]["note"], 60);
        assert_eq!(edited["framesPerRow"], 240);
        assert_eq!(edited["memo"], "preserve song");
        assert_eq!(
            AssetWorkbench::new(AssetKind::Music, &text)
                .unwrap()
                .document,
            edited
        );
        let (changed, restored, _) =
            frame(key(Key::Z, Modifiers::CTRL | Modifiers::COMMAND), false);
        assert!(changed);
        assert_eq!(restored, original);
        let (changed, restored, _) =
            frame(key(Key::Y, Modifiers::CTRL | Modifiers::COMMAND), false);
        assert!(changed);
        assert_eq!(restored, edited);
    }
}
