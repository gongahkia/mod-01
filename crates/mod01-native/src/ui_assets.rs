use std::sync::Arc;

use eframe::egui::{
    self, Color32, ColorImage, CursorIcon, FontData, FontDefinitions, FontFamily, FontId, LayerId,
    Order, Rect, TextStyle, TextureHandle, TextureOptions,
};

pub(crate) const BIG_BLUE: &[u8] = include_bytes!("../assets/bigblue/BigBlue_TerminalPlus.ttf");
const POINTER: &[u8] = include_bytes!("../assets/kenney/tile_0028.png");
const ACTION: &[u8] = include_bytes!("../assets/kenney/tile_0134.png");
const TEXT: &[u8] = include_bytes!("../assets/kenney/tile_0120.png");
const WAIT: &[u8] = include_bytes!("../assets/kenney/tile_0012.png");
const CANVAS: &[u8] = include_bytes!("../assets/kenney/tile_0048.png");

pub fn install_font(context: &egui::Context) {
    let mut definitions = FontDefinitions::default();
    definitions.font_data.insert(
        "BigBlueTerm".to_owned(),
        Arc::new(FontData::from_static(BIG_BLUE)),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        definitions
            .families
            .entry(family)
            .or_default()
            .insert(0, "BigBlueTerm".to_owned());
    }
    context.set_fonts(definitions);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        context.style_mut_of(theme, |style| {
            for (text_style, size) in [
                (TextStyle::Small, 13.0),
                (TextStyle::Body, 16.0),
                (TextStyle::Button, 16.0),
                (TextStyle::Heading, 22.0),
                (TextStyle::Monospace, 16.0),
            ] {
                let family = if text_style == TextStyle::Monospace {
                    FontFamily::Monospace
                } else {
                    FontFamily::Proportional
                };
                style
                    .text_styles
                    .insert(text_style, FontId::new(size, family));
            }
        });
    }
}

pub struct PixelCursors {
    pointers: [TextureHandle; 5],
}

impl PixelCursors {
    pub fn new(context: &egui::Context) -> Self {
        let pointers = [POINTER, ACTION, TEXT, WAIT, CANVAS].map(|bytes| {
            let decoded = image::load_from_memory(bytes)
                .expect("bundled Kenney cursor is a valid PNG")
                .into_rgba8();
            let dimensions = [decoded.width() as usize, decoded.height() as usize];
            let image = ColorImage::from_rgba_unmultiplied(dimensions, decoded.as_raw());
            context.load_texture("kenney-cursor", image, TextureOptions::NEAREST)
        });
        Self { pointers }
    }

    pub fn paint(&self, context: &egui::Context) {
        let Some(pointer) = context.input(|input| input.pointer.hover_pos()) else {
            return;
        };
        let requested = context.output(|output| output.cursor_icon);
        let index = match requested {
            CursorIcon::PointingHand | CursorIcon::Grab | CursorIcon::Grabbing => 1,
            CursorIcon::Text | CursorIcon::VerticalText => 2,
            CursorIcon::Progress | CursorIcon::Wait => 3,
            CursorIcon::Crosshair | CursorIcon::Move => 4,
            _ => 0,
        };
        context.output_mut(|output| output.cursor_icon = CursorIcon::None);
        let image = &self.pointers[index];
        let rect = Rect::from_min_size(pointer, egui::vec2(32.0, 32.0));
        context
            .layer_painter(LayerId::new(
                Order::Foreground,
                egui::Id::unique("kenney-pointer"),
            ))
            .image(
                image.id(),
                rect,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
    }
}
