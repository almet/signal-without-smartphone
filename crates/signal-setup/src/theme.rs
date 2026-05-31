use eframe::egui;

pub(crate) const SIGNAL_BLUE: egui::Color32 = egui::Color32::from_rgb(59, 130, 246);
pub(crate) const SUCCESS_GREEN: egui::Color32 = egui::Color32::from_rgb(22, 163, 74);
pub(crate) const SUCCESS_BG: egui::Color32 = egui::Color32::from_rgb(240, 253, 244);
pub(crate) const SUCCESS_BORDER: egui::Color32 = egui::Color32::from_rgb(187, 247, 208);
pub(crate) const ERROR_RED: egui::Color32 = egui::Color32::from_rgb(220, 38, 38);
pub(crate) const ERROR_BG: egui::Color32 = egui::Color32::from_rgb(254, 242, 242);
pub(crate) const ERROR_BORDER: egui::Color32 = egui::Color32::from_rgb(254, 202, 202);
pub(crate) const INFO_TEXT: egui::Color32 = egui::Color32::from_rgb(29, 78, 216);
pub(crate) const INFO_BG: egui::Color32 = egui::Color32::from_rgb(239, 246, 255);
pub(crate) const INFO_BORDER: egui::Color32 = egui::Color32::from_rgb(191, 219, 254);
pub(crate) const MUTED: egui::Color32 = egui::Color32::from_rgb(107, 114, 128);
pub(crate) const HEADING: egui::Color32 = egui::Color32::from_rgb(17, 24, 39);
pub(crate) const PAGE_BG: egui::Color32 = egui::Color32::from_rgb(243, 244, 246);
pub(crate) const CARD_BG: egui::Color32 = egui::Color32::WHITE;
pub(crate) const BORDER: egui::Color32 = egui::Color32::from_rgb(229, 231, 235);
pub(crate) const INSET_BG: egui::Color32 = egui::Color32::from_rgb(249, 250, 251);

pub(crate) fn setup_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();

    // Backgrounds
    visuals.panel_fill = PAGE_BG;
    visuals.window_fill = CARD_BG;
    visuals.extreme_bg_color = egui::Color32::WHITE; // text input backgrounds

    // Consistent rounding everywhere, so 2025.
    let r = egui::Rounding::same(8.0);
    visuals.window_rounding = r;
    visuals.menu_rounding = r;
    visuals.widgets.noninteractive.rounding = r;
    visuals.widgets.inactive.rounding = r;
    visuals.widgets.hovered.rounding = r;
    visuals.widgets.active.rounding = r;
    visuals.widgets.open.rounding = r;

    // Widget fill / border colors
    visuals.widgets.noninteractive.bg_fill = egui::Color32::WHITE;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.bg_fill = egui::Color32::WHITE;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.bg_fill = INFO_BG;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, SIGNAL_BLUE);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(219, 234, 254);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.5, SIGNAL_BLUE);

    // Selection (text highlight)
    visuals.selection.bg_fill = egui::Color32::from_rgb(219, 234, 254);
    visuals.selection.stroke = egui::Stroke::new(1.0, SIGNAL_BLUE);

    ctx.set_visuals(visuals);

    // Font sizes and spacing
    let mut style = (*ctx.style()).clone();
    use egui::FontFamily::Proportional;
    use egui::TextStyle::*;
    style.text_styles = [
        (Heading, egui::FontId::new(22.0, Proportional)),
        (Body, egui::FontId::new(15.0, Proportional)),
        (
            Monospace,
            egui::FontId::new(14.0, egui::FontFamily::Monospace),
        ),
        (Button, egui::FontId::new(15.0, Proportional)),
        (Small, egui::FontId::new(13.0, Proportional)),
    ]
    .into();
    style.spacing.button_padding = egui::vec2(20.0, 10.0);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    ctx.set_style(style);
}
