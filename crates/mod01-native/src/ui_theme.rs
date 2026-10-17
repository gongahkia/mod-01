//! The development unit's case, recessed displays, and control legends.
use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke};

pub const CASE: Color32 = Color32::from_rgb(41, 39, 37);
pub const CASE_LIGHT: Color32 = Color32::from_rgb(55, 51, 46);
pub const SCREEN: Color32 = Color32::from_rgb(23, 20, 31);
pub const SCREEN_RAISED: Color32 = Color32::from_rgb(48, 45, 42);
pub const CREAM: Color32 = Color32::from_rgb(244, 233, 206);
pub const MUTED: Color32 = Color32::from_rgb(174, 161, 135);
pub const AMBER: Color32 = Color32::from_rgb(231, 189, 80);
pub const CYAN: Color32 = Color32::from_rgb(121, 216, 207);
pub const CORAL: Color32 = Color32::from_rgb(240, 127, 106);
pub const EDGE: Color32 = Color32::from_rgb(109, 100, 82);

pub fn install(context: &egui::Context) {
    context.set_theme(egui::Theme::Dark);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = CASE;
    visuals.window_fill = CASE_LIGHT;
    visuals.window_stroke = Stroke::new(2.0, EDGE);
    visuals.window_corner_radius = CornerRadius::same(2);
    visuals.menu_corner_radius = CornerRadius::same(1);
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    visuals.popup_shadow = egui::epaint::Shadow::NONE;
    visuals.weak_text_color = Some(MUTED);
    visuals.faint_bg_color = CASE_LIGHT;
    visuals.extreme_bg_color = SCREEN;
    visuals.text_edit_bg_color = Some(SCREEN);
    visuals.code_bg_color = SCREEN;
    visuals.warn_fg_color = AMBER;
    visuals.error_fg_color = CORAL;
    visuals.hyperlink_color = CYAN;
    visuals.selection.bg_fill = CYAN;
    visuals.selection.stroke = Stroke::new(1.0, SCREEN);
    visuals.widgets.noninteractive.bg_fill = CASE;
    visuals.widgets.noninteractive.weak_bg_fill = CASE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, EDGE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, CREAM);
    visuals.widgets.inactive.bg_fill = SCREEN_RAISED;
    visuals.widgets.inactive.weak_bg_fill = SCREEN_RAISED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, EDGE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, CREAM);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(76, 69, 58);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(76, 69, 58);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, AMBER);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, CREAM);
    visuals.widgets.active.bg_fill = AMBER;
    visuals.widgets.active.weak_bg_fill = AMBER;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, CREAM);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, SCREEN);
    visuals.widgets.open = visuals.widgets.hovered;
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = CornerRadius::same(1);
        widget.expansion = 0.0;
    }
    context.set_visuals(visuals);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        context.style_mut_of(theme, |style| {
            style.spacing.item_spacing = egui::vec2(6.0, 6.0);
            style.spacing.button_padding = egui::vec2(9.0, 5.0);
            style.spacing.interact_size.y = 40.0;
            style.spacing.window_margin = Margin::same(12);
            style.spacing.menu_margin = Margin::same(7);
        });
    }
}

pub fn case_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(CASE)
        .stroke(Stroke::new(1.0, EDGE))
        .inner_margin(Margin::symmetric(12, 9))
}

pub fn screen_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(SCREEN)
        .stroke(Stroke::new(2.0, EDGE))
        .inner_margin(Margin::same(12))
}

pub fn legend(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text).size(13.0).color(AMBER));
}

pub fn led(ui: &mut egui::Ui, lit: bool, label: &str) {
    let color = if lit { CYAN } else { MUTED };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().rect_filled(rect.shrink(2.0), 1.0, color);
    ui.label(egui::RichText::new(label).size(13.0).color(color));
}
