//! Native, source-visible editors for the non-sprite cartridge assets.

use eframe::egui::{self, Color32, Sense};
use modl_core::AssetKind;
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
    pattern: String,
    order_text: String,
    row: usize,
    channel: usize,
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
        let order_text = document["order"]
            .as_array()
            .map_or_else(String::new, |order| {
                order
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            });
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
            pattern: String::new(),
            order_text,
            row: 0,
            channel: 0,
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

    pub fn tracker_cursor(&self) -> Option<(String, usize, usize)> {
        if self.kind != AssetKind::Music {
            return None;
        }
        let patterns = self.document["patterns"].as_object()?;
        let pattern = if patterns.contains_key(&self.pattern) {
            self.pattern.clone()
        } else {
            patterns.keys().next()?.clone()
        };
        let rows = patterns[&pattern]["rows"].as_array()?;
        if rows.is_empty() {
            return None;
        }
        Some((pattern, self.row.min(rows.len() - 1), self.channel.min(7)))
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
                    let color = pixels[y * 8 + x].as_u64().unwrap_or(0).min(255) as usize;
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

    pub fn import_tile_png(&mut self, png: &[u8], palette: &[[u8; 3]; 256]) -> Result<(), String> {
        if self.kind != AssetKind::TileSet || self.display {
            return Err("OPEN A TILE SET FIRST".to_owned());
        }
        if png.len() < 24 || png[..8] != [137, 80, 78, 71, 13, 10, 26, 10] {
            return Err("FILE IS NOT A PNG".to_owned());
        }
        let width =
            u32::from_be_bytes(png[16..20].try_into().map_err(|_| "invalid PNG width")?) as usize;
        let height =
            u32::from_be_bytes(png[20..24].try_into().map_err(|_| "invalid PNG height")?) as usize;
        let count = (width / 8).saturating_mul(height / 8);
        if width == 0
            || height == 0
            || width % 8 != 0
            || height % 8 != 0
            || !(1..=4096).contains(&count)
        {
            return Err("TILE PNG MUST CONTAIN 1-4096 COMPLETE 8x8 TILES".to_owned());
        }
        let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .map_err(|error| error.to_string())?
            .into_rgba8();
        if image.width() as usize != width || image.height() as usize != height {
            return Err("PNG DIMENSIONS CHANGED WHILE DECODING".to_owned());
        }
        let columns = width / 8;
        let rows = height / 8;
        let mut tiles = Vec::with_capacity(count);
        for tile_y in 0..rows {
            for tile_x in 0..columns {
                let mut tile = Vec::with_capacity(64);
                for y in 0..8 {
                    for x in 0..8 {
                        let pixel = image
                            .get_pixel((tile_x * 8 + x) as u32, (tile_y * 8 + y) as u32)
                            .0;
                        tile.push(json!(if pixel[3] <= 127 {
                            0
                        } else {
                            nearest_palette([pixel[0], pixel[1], pixel[2]], palette)
                        }));
                    }
                }
                tiles.push(Value::Array(tile));
            }
        }
        let previous = self.document.clone();
        let mut flags = self.document["flags"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        flags.resize(count, json!(0));
        flags.truncate(count);
        self.document["tiles"] = Value::Array(tiles);
        self.document["flags"] = Value::Array(flags);
        self.history.push(previous);
        if self.history.len() > 32 {
            self.history.remove(0);
        }
        self.future.clear();
        self.tile = 0;
        self.dirty = true;
        Ok(())
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        palette: &[[u8; 3]; 256],
        tile_sets: &[(String, Value)],
    ) -> bool {
        ui.horizontal(|ui| {
            if ui.button("UNDO").clicked() {
                if let Some(previous) = self.history.pop() {
                    self.future
                        .push(std::mem::replace(&mut self.document, previous));
                }
            }
            if ui.button("REDO").clicked() {
                if let Some(next) = self.future.pop() {
                    self.history
                        .push(std::mem::replace(&mut self.document, next));
                }
            }
            ui.label(if self.display {
                "DISPLAY / SOURCE-VISIBLE".to_owned()
            } else {
                format!("{:?} / SOURCE-VISIBLE", self.kind)
            });
        });
        let before = self.document.clone();
        egui::ScrollArea::both().show(ui, |ui| {
            if self.display {
                self.palette_ui(ui, palette);
            } else {
                match self.kind {
                    AssetKind::Map => self.map_ui(ui, palette, tile_sets),
                    AssetKind::TileSet => self.tiles_ui(ui, palette),
                    AssetKind::Font => self.font_ui(ui),
                    AssetKind::Sound => self.sound_ui(ui),
                    AssetKind::Music => self.music_ui(ui),
                    AssetKind::Sprite | AssetKind::Animation => {}
                }
            }
        });
        if self.document != before {
            self.history.push(before);
            if self.history.len() > 32 {
                self.history.remove(0);
            }
            self.future.clear();
            self.dirty = true;
            return true;
        }
        self.dirty = !self.history.is_empty() || !self.future.is_empty();
        false
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
                let tile_index = cells.get(index).and_then(Value::as_u64).unwrap_or(0) as usize;
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
                let mut value = raster[index][key].as_i64().unwrap_or(0);
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
            .and_then(|index| raster[index]["remap"][self.logical_color].as_u64())
            .or_else(|| self.document["remap"][self.logical_color].as_u64())
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
        let mut flag = self.document["flags"][self.tile]
            .as_u64()
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
        let mut wave = self.document["waveform"]
            .as_str()
            .unwrap_or("pulse")
            .to_owned();
        egui::ComboBox::from_id_salt("waveform")
            .selected_text(&wave)
            .show_ui(ui, |ui| {
                for choice in ["pulse", "triangle", "saw", "noise", "wavetable"] {
                    ui.selectable_value(&mut wave, choice.to_owned(), choice);
                }
            });
        self.document["waveform"] = json!(wave);
        for (label, path, low, high) in [
            ("NOTE", "note", 0.0, 127.0),
            ("FRAMES", "durationFrames", 1.0, 3600.0),
            ("VOLUME", "volume", 0.0, 1.0),
            ("PAN", "pan", -1.0, 1.0),
            ("DUTY", "duty", 0.0, 1.0),
            ("ATTACK", "envelope.attackFrames", 0.0, 120.0),
            ("DECAY", "envelope.decayFrames", 0.0, 120.0),
            ("SUSTAIN", "envelope.sustainLevel", 0.0, 1.0),
            ("RELEASE", "envelope.releaseFrames", 0.0, 120.0),
            ("SLIDE", "pitch.slideSemitonesPerFrame", -8.0, 8.0),
            ("VIBRATO DEPTH", "pitch.vibratoDepthSemitones", 0.0, 12.0),
            ("VIBRATO PERIOD", "pitch.vibratoPeriodFrames", 0.0, 120.0),
        ] {
            let mut value = number_path(&self.document, path);
            ui.horizontal(|ui| {
                ui.label(label);
                if ui.add(egui::Slider::new(&mut value, low..=high)).changed() {
                    set_number_path(&mut self.document, path, value);
                }
            });
        }
    }

    fn music_ui(&mut self, ui: &mut egui::Ui) {
        let mut speed = integer(&self.document["framesPerRow"]).clamp(1, 60);
        ui.horizontal(|ui| {
            ui.label("FRAMES / ROW");
            if ui
                .add(egui::DragValue::new(&mut speed).range(1..=60))
                .changed()
            {
                self.document["framesPerRow"] = json!(speed);
            }
            let mut looping = self.document["loop"].as_bool().unwrap_or(false);
            if ui.checkbox(&mut looping, "LOOP").changed() {
                self.document["loop"] = json!(looping);
            }
        });
        let Some(patterns) = self.document["patterns"].as_object_mut() else {
            return;
        };
        if self.pattern.is_empty() || !patterns.contains_key(&self.pattern) {
            self.pattern = patterns.keys().next().cloned().unwrap_or_default();
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("pattern")
                .selected_text(&self.pattern)
                .show_ui(ui, |ui| {
                    for name in patterns.keys() {
                        ui.selectable_value(&mut self.pattern, name.clone(), name);
                    }
                });
            if ui.button("+ PATTERN").clicked() && patterns.len() < 256 {
                if let Some(name) = (0..=255)
                    .map(|n| format!("{n:02X}"))
                    .find(|name| !patterns.contains_key(name))
                {
                    patterns.insert(name.clone(), json!({"rows":vec![vec![Value::Null;8];16]}));
                    self.pattern = name;
                }
            }
        });
        let Some(rows) = patterns
            .get_mut(&self.pattern)
            .and_then(|pattern| pattern["rows"].as_array_mut())
        else {
            return;
        };
        if rows.is_empty() {
            return;
        }
        self.row = self.row.min(rows.len() - 1);
        self.channel = self.channel.min(7);
        ui.horizontal(|ui| {
            ui.label("ROW");
            ui.add(egui::DragValue::new(&mut self.row).range(0..=rows.len() - 1));
            ui.label("CHANNEL");
            ui.add(egui::DragValue::new(&mut self.channel).range(0..=7));
            if ui.button("+ ROW").clicked() && rows.len() < 256 {
                rows.push(json!(vec![Value::Null; 8]));
            }
            if ui.button("- ROW").clicked() && rows.len() > 1 {
                rows.remove(self.row);
                self.row = self.row.min(rows.len() - 1);
            }
        });
        let Some(cells) = rows[self.row].as_array_mut() else {
            return;
        };
        if cells.len() != 8 {
            return;
        }
        let cell = &mut cells[self.channel];
        let mut enabled = !cell.is_null();
        if ui.checkbox(&mut enabled, "NOTE ON").changed() {
            *cell = if enabled {
                json!({"note":60,"sound":"blip"})
            } else {
                Value::Null
            };
        }
        if enabled {
            let mut note = integer(&cell["note"]).clamp(0, 127);
            let mut sound = cell["sound"].as_str().unwrap_or("").to_owned();
            ui.horizontal(|ui| {
                ui.label("NOTE");
                if ui
                    .add(egui::DragValue::new(&mut note).range(0..=127))
                    .changed()
                {
                    cell["note"] = json!(note);
                }
                ui.label("SOUND");
                if ui.text_edit_singleline(&mut sound).changed() {
                    cell["sound"] = json!(sound);
                }
            });
        }
        ui.label("ORDER: edit the pattern sequence below");
        let pattern_names = patterns.keys().cloned().collect::<Vec<_>>();
        if let Some(order) = self.document["order"].as_array_mut() {
            let response = ui.text_edit_singleline(&mut self.order_text);
            if response.lost_focus() || ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                let listed = self
                    .order_text
                    .split_whitespace()
                    .filter(|name| pattern_names.iter().any(|pattern| pattern == name))
                    .map(|name| json!(name))
                    .collect::<Vec<_>>();
                if !listed.is_empty() {
                    *order = listed;
                }
            }
        }
    }
}

fn integer(value: &Value) -> usize {
    value.as_u64().unwrap_or(0) as usize
}

fn nearest_palette(rgb: [u8; 3], palette: &[[u8; 3]; 256]) -> usize {
    palette
        .iter()
        .enumerate()
        .skip(1)
        .min_by_key(|(_, color)| {
            rgb.iter()
                .zip(color.iter())
                .map(|(a, b)| (i32::from(*a) - i32::from(*b)).pow(2))
                .sum::<i32>()
        })
        .map_or(1, |(index, _)| index)
}

fn number_path(value: &Value, path: &str) -> f64 {
    path.split('.')
        .fold(value, |current, key| &current[key])
        .as_f64()
        .unwrap_or(0.0)
}

fn set_number_path(value: &mut Value, path: &str, number: f64) {
    let mut parts = path.split('.');
    let first = parts.next().unwrap_or("");
    if let Some(second) = parts.next() {
        value[first][second] = json!(number);
    } else {
        value[first] = json!(number);
    }
}

fn rgb_for_tile(tile: &Value, palette: &[[u8; 3]; 256]) -> [u8; 3] {
    let index = tile
        .as_array()
        .and_then(|pixels| pixels.get(27))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    palette[index.min(255)]
}

fn color_for_tile(tile: &Value, palette: &[[u8; 3]; 256]) -> Color32 {
    let [r, g, b] = rgb_for_tile(tile, palette);
    Color32::from_rgb(r, g, b)
}

fn flood_fill(cells: &mut [Value], width: usize, height: usize, start: usize, tile: u64) {
    let Some(target) = cells.get(start).and_then(Value::as_u64) else {
        return;
    };
    if target == tile {
        return;
    }
    let mut stack = vec![start];
    while let Some(index) = stack.pop() {
        if cells[index].as_u64() != Some(target) {
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
                    if glyph["pixels"][y * width + x].as_u64().unwrap_or(0) != 0 {
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
            let selected = pixels.get(index).and_then(Value::as_u64).unwrap_or(0) as usize;
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
        imported.import_tile_png(&png, &palette).unwrap();
        assert_eq!(imported.document["tiles"], editor.document["tiles"]);
        assert_eq!(imported.document["flags"], editor.document["flags"]);
    }

    #[test]
    fn structured_editor_keeps_source_visible() {
        let workbench =
            AssetWorkbench::new(AssetKind::Sound, r#"{"kind":"sound","note":60}"#).unwrap();
        assert!(workbench.source().unwrap().contains("\"note\":60"));
    }
}
