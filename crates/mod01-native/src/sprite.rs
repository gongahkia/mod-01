use modl_core::assets::integer_value;
use std::collections::VecDeque;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

pub const DEFAULT_AXIS: usize = 64;
pub const MAX_AXIS: usize = 128;
pub const MAX_LAYERS: usize = 16;
pub const MAX_FRAMES: usize = 256;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    /// Straight-alpha RGBA8 pixels. Layers retain their original colours.
    #[serde(
        serialize_with = "serialize_pixels",
        deserialize_with = "deserialize_pixels"
    )]
    pub pixels: Vec<u8>,
}

fn serialize_pixels<S: Serializer>(pixels: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&STANDARD.encode(pixels))
}

fn deserialize_pixels<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let value = String::deserialize(deserializer)?;
    let bytes = STANDARD.decode(&value).map_err(D::Error::custom)?;
    if STANDARD.encode(&bytes) != value {
        return Err(D::Error::custom("pixels must use canonical base64"));
    }
    Ok(bytes)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpriteFrame {
    pub layers: Vec<Layer>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SpriteDocument {
    pub revision: u8,
    pub width: usize,
    pub height: usize,
    pub frames: Vec<SpriteFrame>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Pencil,
    Eraser,
    Eyedropper,
    Fill,
    Select,
}

pub struct SpriteEditor {
    pub name: String,
    pub asset_path: String,
    pub document: SpriteDocument,
    pub frame: usize,
    pub layer: usize,
    pub tool: Tool,
    pub color: [u8; 4],
    pub brush: usize,
    pub zoom: f32,
    pub pan: [f32; 2],
    pub onion: bool,
    pub show_grid: bool,
    pub selection: Option<[usize; 4]>,
    pub dirty: bool,
    undo: VecDeque<SpriteDocument>,
    redo: VecDeque<SpriteDocument>,
}

impl SpriteDocument {
    pub fn new(width: usize, height: usize) -> Self {
        let layer = Layer::blank("Layer 1", width, height);
        Self {
            revision: 1,
            width,
            height,
            frames: vec![SpriteFrame {
                layers: vec![layer],
            }],
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.revision != 1
            || !(1..=MAX_AXIS).contains(&self.width)
            || !(1..=MAX_AXIS).contains(&self.height)
            || !(1..=MAX_FRAMES).contains(&self.frames.len())
        {
            return Err("invalid layered sprite dimensions or frame count".to_owned());
        }
        for frame in &self.frames {
            if !(1..=MAX_LAYERS).contains(&frame.layers.len()) {
                return Err("invalid sprite layer count".to_owned());
            }
            for layer in &frame.layers {
                if layer.name.is_empty()
                    || layer.name.len() > 64
                    || layer.pixels.len() != self.width * self.height * 4
                {
                    return Err("invalid sprite layer".to_owned());
                }
            }
        }
        Ok(())
    }

    pub fn composite(&self, frame: usize) -> Vec<u8> {
        let mut pixels = vec![0; self.width * self.height * 4];
        if let Some(frame) = self.frames.get(frame) {
            for layer in frame.layers.iter().filter(|layer| layer.visible) {
                for (under, over) in pixels.chunks_exact_mut(4).zip(layer.pixels.chunks_exact(4)) {
                    let result = blend(
                        [under[0], under[1], under[2], under[3]],
                        [over[0], over[1], over[2], over[3]],
                    );
                    under.copy_from_slice(&result);
                }
            }
        }
        pixels
    }

    pub fn indexed_frames(&self) -> Vec<Vec<u8>> {
        (0..self.frames.len())
            .map(|frame| {
                self.composite(frame)
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .copied()
                    .map(modl_core::asset_import::palette_index)
                    .collect()
            })
            .collect()
    }
}

impl Layer {
    fn blank(name: &str, width: usize, height: usize) -> Self {
        Self {
            name: name.to_owned(),
            visible: true,
            locked: false,
            pixels: vec![0; width * height * 4],
        }
    }
}

impl SpriteEditor {
    pub fn retained_bytes(&self) -> usize {
        fn document_bytes(document: &SpriteDocument) -> usize {
            document.frames.capacity() * std::mem::size_of::<SpriteFrame>()
                + document
                    .frames
                    .iter()
                    .map(|frame| {
                        frame.layers.capacity() * std::mem::size_of::<Layer>()
                            + frame
                                .layers
                                .iter()
                                .map(|layer| layer.name.capacity() + layer.pixels.capacity())
                                .sum::<usize>()
                    })
                    .sum::<usize>()
        }
        std::mem::size_of::<Self>()
            + self.name.capacity()
            + self.asset_path.capacity()
            + document_bytes(&self.document)
            + self
                .undo
                .iter()
                .chain(&self.redo)
                .map(document_bytes)
                .sum::<usize>()
    }

    pub fn new(name: String, asset_path: String, document: SpriteDocument) -> Result<Self, String> {
        document.validate()?;
        Ok(Self {
            name,
            asset_path,
            document,
            frame: 0,
            layer: 0,
            tool: Tool::Pencil,
            color: [237, 123, 105, 255],
            brush: 1,
            zoom: 6.0,
            pan: [0.0, 0.0],
            onion: true,
            show_grid: true,
            selection: None,
            dirty: false,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        })
    }

    pub fn from_indexed(
        name: String,
        asset_path: String,
        bytes: &[u8],
        palette: &[[u8; 3]; 256],
    ) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        let width = integer_value(&value["width"], 1, MAX_AXIS as i32)
            .ok_or("sprite width is invalid")? as usize;
        let height = integer_value(&value["height"], 1, MAX_AXIS as i32)
            .ok_or("sprite height is invalid")? as usize;
        if !(1..=MAX_AXIS).contains(&width) || !(1..=MAX_AXIS).contains(&height) {
            return Err("sprite dimensions exceed native editor limit".to_owned());
        }
        let frames = value["frames"]
            .as_array()
            .ok_or("sprite frames are missing")?;
        if frames.is_empty() || frames.len() > MAX_FRAMES {
            return Err("invalid sprite frame count".to_owned());
        }
        let mut document = SpriteDocument {
            revision: 1,
            width,
            height,
            frames: Vec::new(),
        };
        for frame in frames {
            let indices = frame.as_array().ok_or("invalid sprite frame")?;
            if indices.len() != width * height {
                return Err("sprite frame size mismatch".to_owned());
            }
            let mut layer = Layer::blank("Layer 1", width, height);
            for (index, value) in indices.iter().enumerate() {
                let color =
                    integer_value(value, 0, 255).ok_or("invalid indexed sprite pixel")? as usize;
                if color >= palette.len() {
                    return Err("sprite color exceeds palette".to_owned());
                }
                let offset = index * 4;
                layer.pixels[offset..offset + 3].copy_from_slice(&palette[color]);
                layer.pixels[offset + 3] = if color == 0 { 0 } else { 255 };
            }
            document.frames.push(SpriteFrame {
                layers: vec![layer],
            });
        }
        Self::new(name, asset_path, document)
    }

    pub fn remember(&mut self) {
        self.undo.push_back(self.document.clone());
        while self.undo.len() > 32 {
            self.undo.pop_front();
        }
        self.redo.clear();
    }

    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop_back() {
            self.redo
                .push_back(std::mem::replace(&mut self.document, previous));
            self.clamp_selection();
            self.dirty = true;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop_back() {
            self.undo
                .push_back(std::mem::replace(&mut self.document, next));
            self.clamp_selection();
            self.dirty = true;
        }
    }

    fn clamp_selection(&mut self) {
        self.frame = self.frame.min(self.document.frames.len() - 1);
        self.layer = self
            .layer
            .min(self.document.frames[self.frame].layers.len() - 1);
        self.selection = None;
    }

    pub fn active_layer(&self) -> &Layer {
        &self.document.frames[self.frame].layers[self.layer]
    }
    fn active_layer_mut(&mut self) -> &mut Layer {
        &mut self.document.frames[self.frame].layers[self.layer]
    }

    pub fn paint_line(&mut self, from: (i32, i32), to: (i32, i32)) {
        if self.active_layer().locked || !self.active_layer().visible {
            return;
        }
        let (mut x, mut y) = from;
        let dx = (to.0 - x).abs();
        let dy = -(to.1 - y).abs();
        let sx = if x < to.0 { 1 } else { -1 };
        let sy = if y < to.1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            self.paint_brush(x, y);
            if x == to.0 && y == to.1 {
                break;
            }
            let doubled = 2 * error;
            if doubled >= dy {
                error += dy;
                x += sx;
            }
            if doubled <= dx {
                error += dx;
                y += sy;
            }
        }
        self.dirty = true;
    }

    fn paint_brush(&mut self, x: i32, y: i32) {
        let width = self.document.width;
        let height = self.document.height;
        let brush = self.brush as i32;
        let tool = self.tool;
        let color = self.color;
        let layer = self.active_layer_mut();
        for by in y - (brush - 1) / 2..=y + brush / 2 {
            for bx in x - (brush - 1) / 2..=x + brush / 2 {
                if bx < 0 || by < 0 || bx as usize >= width || by as usize >= height {
                    continue;
                }
                let offset = (by as usize * width + bx as usize) * 4;
                let old = [
                    layer.pixels[offset],
                    layer.pixels[offset + 1],
                    layer.pixels[offset + 2],
                    layer.pixels[offset + 3],
                ];
                let next = if tool == Tool::Eraser {
                    [0; 4]
                } else {
                    blend(old, color)
                };
                layer.pixels[offset..offset + 4].copy_from_slice(&next);
            }
        }
    }

    pub fn pick(&mut self, x: usize, y: usize) {
        if x >= self.document.width || y >= self.document.height {
            return;
        }
        let composite = self.document.composite(self.frame);
        let offset = (y * self.document.width + x) * 4;
        self.color.copy_from_slice(&composite[offset..offset + 4]);
    }

    pub fn fill(&mut self, x: usize, y: usize) {
        let width = self.document.width;
        let height = self.document.height;
        if x >= width || y >= height || self.active_layer().locked || !self.active_layer().visible {
            return;
        }
        let color = self.color;
        let layer = self.active_layer_mut();
        let index = (y * width + x) * 4;
        let target = [
            layer.pixels[index],
            layer.pixels[index + 1],
            layer.pixels[index + 2],
            layer.pixels[index + 3],
        ];
        if target == color {
            return;
        }
        let mut stack = vec![(x, y)];
        let mut seen = vec![false; width * height];
        while let Some((px, py)) = stack.pop() {
            let index = py * width + px;
            if seen[index] {
                continue;
            }
            seen[index] = true;
            let offset = index * 4;
            if layer.pixels[offset..offset + 4] != target {
                continue;
            }
            layer.pixels[offset..offset + 4].copy_from_slice(&color);
            if px > 0 {
                stack.push((px - 1, py));
            }
            if px + 1 < width {
                stack.push((px + 1, py));
            }
            if py > 0 {
                stack.push((px, py - 1));
            }
            if py + 1 < height {
                stack.push((px, py + 1));
            }
        }
        self.dirty = true;
    }

    pub fn clear_or_recolor_selection(&mut self, erase: bool) {
        let Some([x0, y0, x1, y1]) = self.selection else {
            return;
        };
        if self.active_layer().locked || !self.active_layer().visible {
            return;
        }
        self.remember();
        let color = self.color;
        let width = self.document.width;
        let layer = self.active_layer_mut();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let offset = (y * width + x) * 4;
                if erase {
                    layer.pixels[offset..offset + 4].fill(0);
                } else if layer.pixels[offset + 3] > 0 {
                    layer.pixels[offset..offset + 3].copy_from_slice(&color[..3]);
                }
            }
        }
        self.dirty = true;
    }

    pub fn add_layer(&mut self) {
        if self.document.frames[self.frame].layers.len() >= MAX_LAYERS {
            return;
        }
        self.remember();
        let count = self.document.frames[self.frame].layers.len() + 1;
        self.document.frames[self.frame].layers.push(Layer::blank(
            &format!("Layer {count}"),
            self.document.width,
            self.document.height,
        ));
        self.layer = count - 1;
        self.dirty = true;
    }

    pub fn delete_layer(&mut self) {
        if self.document.frames[self.frame].layers.len() <= 1 {
            return;
        }
        self.remember();
        self.document.frames[self.frame].layers.remove(self.layer);
        self.layer = self
            .layer
            .min(self.document.frames[self.frame].layers.len() - 1);
        self.dirty = true;
    }

    pub fn move_layer(&mut self, direction: i32) {
        let next = self.layer as i32 + direction;
        if next < 0 || next as usize >= self.document.frames[self.frame].layers.len() {
            return;
        }
        self.remember();
        self.document.frames[self.frame]
            .layers
            .swap(self.layer, next as usize);
        self.layer = next as usize;
        self.dirty = true;
    }

    pub fn add_frame(&mut self) {
        if self.document.frames.len() >= MAX_FRAMES {
            return;
        }
        self.remember();
        let copy = self.document.frames[self.frame].clone();
        self.document.frames.insert(self.frame + 1, copy);
        self.frame += 1;
        self.selection = None;
        self.dirty = true;
    }

    pub fn delete_frame(&mut self) {
        if self.document.frames.len() <= 1 {
            return;
        }
        self.remember();
        self.document.frames.remove(self.frame);
        self.clamp_selection();
        self.dirty = true;
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        if !(1..=MAX_AXIS).contains(&width) || !(1..=MAX_AXIS).contains(&height) {
            return;
        }
        if self.document.width == width && self.document.height == height {
            return;
        }
        self.remember();
        self.resize_pixels(width, height);
    }

    fn resize_pixels(&mut self, width: usize, height: usize) {
        let old_width = self.document.width;
        let old_height = self.document.height;
        for frame in &mut self.document.frames {
            for layer in &mut frame.layers {
                let mut pixels = vec![0; width * height * 4];
                for y in 0..height.min(old_height) {
                    for x in 0..width.min(old_width) {
                        let from = (y * old_width + x) * 4;
                        let to = (y * width + x) * 4;
                        pixels[to..to + 4].copy_from_slice(&layer.pixels[from..from + 4]);
                    }
                }
                layer.pixels = pixels;
            }
        }
        self.document.width = width;
        self.document.height = height;
        self.selection = None;
        self.dirty = true;
    }

    pub fn export_png(&self) -> Result<Vec<u8>, String> {
        let mut output = Vec::new();
        let image = image::RgbaImage::from_raw(
            self.document.width as u32,
            self.document.height as u32,
            self.document.composite(self.frame),
        )
        .ok_or("invalid flattened sprite")?;
        image
            .write_to(
                &mut std::io::Cursor::new(&mut output),
                image::ImageFormat::Png,
            )
            .map_err(|error| error.to_string())?;
        Ok(output)
    }

    pub fn import_png(&mut self, bytes: &[u8]) -> Result<(), String> {
        use modl_core::asset_import::{PngAssetKind, decode_png};
        if self.active_layer().locked {
            return Err("UNLOCK THE CURRENT LAYER BEFORE IMPORTING".to_owned());
        }
        let source = decode_png(bytes, PngAssetKind::Sprite).map_err(|error| error.to_string())?;
        self.remember();
        if self.document.width != source.width || self.document.height != source.height {
            self.resize_pixels(source.width, source.height);
        }
        self.active_layer_mut().pixels = source.pixels;
        self.selection = None;
        self.dirty = true;
        Ok(())
    }
}

pub fn palette() -> [[u8; 3]; 256] {
    *modl_core::hardware::master_palette()
}

fn blend(under: [u8; 4], over: [u8; 4]) -> [u8; 4] {
    let sa = u32::from(over[3]);
    let da = u32::from(under[3]);
    if sa == 0 {
        return under;
    }
    if sa == 255 {
        return over;
    }
    let alpha = sa + (da * (255 - sa) + 127) / 255;
    if alpha == 0 {
        return [0; 4];
    }
    let mut output = [0; 4];
    for channel in 0..3 {
        let value = (u32::from(over[channel]) * sa * 255
            + u32::from(under[channel]) * da * (255 - sa)
            + alpha * 127)
            / (alpha * 255);
        output[channel] = value.min(255) as u8;
    }
    output[3] = alpha as u8;
    output
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_AXIS, MAX_AXIS, SpriteDocument, SpriteEditor, Tool, palette};

    #[test]
    fn integer_asset_spellings_preserve_indexed_sprite_pixels() {
        let bytes =
            br#"{"revision":2.0,"kind":"sprite","width":2.0,"height":1e0,"frames":[[0.0,255e0]]}"#;
        let colors = palette();
        let editor =
            SpriteEditor::from_indexed("hero".into(), "assets/hero.m01g".into(), bytes, &colors)
                .unwrap();
        assert_eq!(editor.document.width, 2);
        assert_eq!(editor.document.height, 1);
        assert_eq!(
            &editor.active_layer().pixels[..4],
            &[colors[0][0], colors[0][1], colors[0][2], 0]
        );
        assert_eq!(
            &editor.active_layer().pixels[4..],
            &[colors[255][0], colors[255][1], colors[255][2], 255]
        );
        assert!(!editor.dirty);
        for bad in ["0.5", "256.0", "-1.0", "true", "\"7\""] {
            let source = format!(r#"{{"width":1.0,"height":1.0,"frames":[[{bad}]]}}"#);
            assert!(
                SpriteEditor::from_indexed(
                    "hero".into(),
                    "assets/hero.m01g".into(),
                    source.as_bytes(),
                    &colors
                )
                .is_err()
            );
        }
    }

    #[test]
    fn png_import_matches_the_shared_converter_and_resizes_as_one_undo_transaction() {
        use modl_core::asset_import::{PngAssetKind, decode_png, indexed_asset};
        let pixels = vec![
            119, 37, 92, 255, 23, 20, 31, 255, 0, 255, 0, 127, 0, 255, 0, 128, 9, 55, 129, 0, 244,
            233, 206, 255, 0, 0, 0, 255, 255, 255, 255, 255,
        ];
        let image = image::RgbaImage::from_raw(4, 2, pixels.clone()).unwrap();
        let mut png = std::io::Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = png.into_inner();
        let decoded = decode_png(&png, PngAssetKind::Sprite).unwrap();
        let expected = indexed_asset(&decoded, PngAssetKind::Sprite, None).unwrap();
        let mut editor = SpriteEditor::new(
            "hero".into(),
            "assets/hero.m01g".into(),
            SpriteDocument::new(2, 1),
        )
        .unwrap();
        let before = serde_json::to_value(&editor.document).unwrap();
        editor.import_png(&png).unwrap();
        assert_eq!(editor.active_layer().pixels, pixels);
        assert_eq!(
            serde_json::to_value(editor.document.indexed_frames()).unwrap(),
            expected["frames"]
        );
        editor.undo();
        assert_eq!(serde_json::to_value(&editor.document).unwrap(), before);
        editor.undo();
        assert_eq!(serde_json::to_value(&editor.document).unwrap(), before);
        editor.redo();
        assert_eq!(editor.document.width, 4);
        assert_eq!(editor.active_layer().pixels, pixels);
        let current = serde_json::to_value(&editor.document).unwrap();
        let history = (editor.undo.len(), editor.redo.len());
        let mut huge = png.clone();
        huge[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(editor.import_png(&huge).is_err());
        assert_eq!(serde_json::to_value(&editor.document).unwrap(), current);
        assert_eq!((editor.undo.len(), editor.redo.len()), history);
        editor.active_layer_mut().locked = true;
        assert!(editor.import_png(&png).is_err());
    }

    #[test]
    fn revision_two_supports_the_default_and_maximum_sprite_detail() {
        assert_eq!(DEFAULT_AXIS, 64);
        assert_eq!(MAX_AXIS, 128);
        assert!(SpriteDocument::new(129, 128).validate().is_err());
        let mut editor = SpriteEditor::new(
            "large".into(),
            "assets/large.m01g".into(),
            SpriteDocument::new(128, 128),
        )
        .unwrap();
        let last = palette()[255];
        editor.color = [last[0], last[1], last[2], 255];
        editor.paint_line((127, 127), (127, 127));
        let frames = editor.document.indexed_frames();
        assert_eq!(frames[0].len(), 128 * 128);
        assert_eq!(frames[0][128 * 128 - 1], 255);
        assert_eq!(frames[0][0], 0);
    }

    #[test]
    fn layered_source_requires_compact_canonical_pixels() {
        let document = SpriteDocument::new(64, 64);
        let compact = serde_json::to_vec(&document).unwrap();
        assert!(compact.len() < 23_000);
        let restored: SpriteDocument = serde_json::from_slice(&compact).unwrap();
        assert_eq!(restored.frames[0].layers[0].pixels.len(), 64 * 64 * 4);
        let old = r#"{"revision":1,"width":1,"height":1,"frames":[{"layers":[{"name":"Base","visible":true,"locked":false,"pixels":[1,2,3,255]}]}]}"#;
        assert!(serde_json::from_str::<SpriteDocument>(old).is_err());
    }

    #[test]
    fn layered_source_flattens_and_preserves_transparency() {
        let mut editor = SpriteEditor::new(
            "hero".into(),
            "assets/hero.m01g".into(),
            SpriteDocument::new(2, 2),
        )
        .unwrap();
        editor.color = [255, 0, 0, 255];
        editor.paint_line((0, 0), (0, 0));
        editor.add_layer();
        editor.color = [0, 0, 255, 128];
        editor.paint_line((0, 0), (0, 0));
        let composite = editor.document.composite(0);
        assert_eq!(composite[3], 255);
        assert!(composite[0] > 0 && composite[2] > 0);
        assert_eq!(composite[7], 0);
        assert_ne!(editor.document.indexed_frames()[0][0], 0);
    }

    #[test]
    fn undo_and_fill_restore_layer_pixels() {
        let mut editor = SpriteEditor::new(
            "hero".into(),
            "assets/hero.m01g".into(),
            SpriteDocument::new(4, 4),
        )
        .unwrap();
        editor.tool = Tool::Fill;
        editor.color = [1, 2, 3, 255];
        editor.remember();
        editor.fill(0, 0);
        assert_eq!(editor.active_layer().pixels[3], 255);
        editor.undo();
        assert_eq!(editor.active_layer().pixels[3], 0);
        editor.redo();
        assert_eq!(editor.active_layer().pixels[3], 255);
    }
}
