//! Stateless widgets that can be reused in different places.
use eframe::egui;
use egui::RichText;
use signal_setup_core::desktop;
use std::process::Command;

use crate::app::Status;
use crate::theme::*;

/// Draw the step indicator (circles and connecting lines) and return the
/// step number the user clicked, if any.
///
/// FIXME: remove the step names from here and pass them as parameters instead
/// so that this function is completely reusable in other contexts.
pub(crate) fn draw_step_indicator(
    ui: &mut egui::Ui,
    current: usize,
    captcha_was_required: bool,
) -> Option<usize> {
    let names = ["Phone", "Captcha", "Verify", "Link"];
    let n = names.len();
    let circle_radius = 13.0_f32;
    let height = circle_radius * 2.0 + 22.0;
    let available = ui.available_width();

    let (outer_rect, _) =
        ui.allocate_exact_size(egui::vec2(available, height), egui::Sense::hover());

    let step_width = available / n as f32;
    let mut clicked: Option<usize> = None;

    // Connector lines first so the circles render on top of them.
    for i in 0..n - 1 {
        let step = i + 1;
        let cx = outer_rect.left() + step_width * (i as f32 + 0.5);
        let cy = outer_rect.top() + circle_radius;
        let next_cx = outer_rect.left() + step_width * (i as f32 + 1.5);
        let line_color = if step < current {
            SUCCESS_GREEN
        } else {
            egui::Color32::from_rgb(209, 213, 219)
        };
        ui.painter().line_segment(
            [
                egui::pos2(cx + circle_radius + 4.0, cy),
                egui::pos2(next_cx - circle_radius - 4.0, cy),
            ],
            egui::Stroke::new(2.0, line_color),
        );
    }

    for i in 0..n {
        let step = i + 1;
        let is_done = step < current;
        let is_active = step == current;
        // Past steps are clickable, except Captcha when it was skipped.
        let is_clickable = is_done && (captcha_was_required || step != 2);

        let cx = outer_rect.left() + step_width * (i as f32 + 0.5);
        let cy = outer_rect.top() + circle_radius;
        let center = egui::pos2(cx, cy);

        // The circle and its label are clickable
        let hit_rect = egui::Rect::from_center_size(
            egui::pos2(cx, cy + 6.0),
            egui::vec2(step_width.min(80.0), height),
        );
        let sense = if is_clickable {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let response = ui.interact(hit_rect, ui.id().with(("step", step)), sense);

        if is_clickable && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            clicked = Some(step);
        }

        let hovered = is_clickable && response.hovered();
        let mut circle_color = if is_done {
            SUCCESS_GREEN
        } else if is_active {
            SIGNAL_BLUE
        } else {
            egui::Color32::from_rgb(209, 213, 219)
        };
        if hovered {
            // Brighten on hover to signal interactivity.
            circle_color = egui::Color32::from_rgb(
                circle_color.r().saturating_add(20),
                circle_color.g().saturating_add(20),
                circle_color.b().saturating_add(20),
            );
        }
        let label_color = if is_done || is_active { HEADING } else { MUTED };

        let painter = ui.painter();
        painter.circle_filled(center, circle_radius, circle_color);
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            step.to_string(),
            egui::FontId::proportional(11.0),
            egui::Color32::WHITE,
        );
        painter.text(
            egui::pos2(cx, cy + circle_radius + 5.0),
            egui::Align2::CENTER_TOP,
            names[i],
            egui::FontId::proportional(12.0),
            label_color,
        );
    }

    clicked
}

/// Status banner. It displays different colors depending on the status passed.
pub(crate) fn show_status(ui: &mut egui::Ui, status: &Status) {
    let (icon, text, text_color, bg, border) = match status {
        Status::None => return,
        Status::Error(m) => ("!", m.as_str(), ERROR_RED, ERROR_BG, ERROR_BORDER),
        Status::Success(m) => ("OK", m.as_str(), SUCCESS_GREEN, SUCCESS_BG, SUCCESS_BORDER),
        Status::Info(m) => ("i", m.as_str(), INFO_TEXT, INFO_BG, INFO_BORDER),
    };

    egui::Frame::none()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, border))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(format!("{icon} {text}"))
                    .color(text_color)
                    .size(14.0),
            );
        });
    ui.add_space(12.0);
}

/// Heading.
pub(crate) fn step_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(19.0).color(HEADING).strong());
    ui.add_space(2.0);
    ui.label(RichText::new(subtitle).size(13.0).color(MUTED));
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(12.0);
}

/// Section showing if Signal Desktop is detected on this machine.
pub(crate) fn show_signal_desktop_status(ui: &mut egui::Ui) {
    let installed = desktop::is_installed();
    let configured = desktop::is_configured();

    let (msg, bg, border, fg) = match (installed, configured) {
        (true, true) => (
            "Signal Desktop is installed.".to_string(),
            SUCCESS_BG,
            SUCCESS_BORDER,
            SUCCESS_GREEN,
        ),
        (true, false) => (
            "Signal Desktop is installed and configured.".to_string(),
            INFO_BG,
            INFO_BORDER,
            INFO_TEXT,
        ),
        (false, _) => (
            "Signal Desktop was not detected. Please install it.".to_string(),
            INFO_BG,
            INFO_BORDER,
            INFO_TEXT,
        ),
    };
    egui::Frame::none()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, border))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.label(RichText::new(msg).size(13.0).color(fg));
        });
}

/// Box with bullet instructions.
pub(crate) fn instruction_box(ui: &mut egui::Ui, lines: &[&str]) {
    egui::Frame::none()
        .fill(INSET_BG)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            for line in lines {
                ui.label(RichText::new(*line).size(14.0).color(MUTED));
            }
        });
}

/// Right-aligned primary action button. Returns `true` if clicked.
pub(crate) fn submit_row(ui: &mut egui::Ui, enabled: bool, label: &str) -> bool {
    let mut clicked = false;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let btn = egui::Button::new(RichText::new(label).color(egui::Color32::WHITE).size(15.0))
            .fill(SIGNAL_BLUE)
            .rounding(egui::Rounding::same(8.0))
            .min_size(egui::vec2(140.0, 40.0));
        clicked = ui.add_enabled(enabled, btn).clicked();
    });
    clicked
}

/// Open an URL in the default detected browser.
pub(crate) fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(url).spawn();

    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();

    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
}

/// Format an error as text.
///
/// If a `cause()` is provided, read and inline it.
///
/// This has been especially useful for `reqwest` errors, that hides the details of the
/// error otherwise.
pub(crate) fn format_error_chain(err: &dyn std::error::Error) -> String {
    let mut out = err.to_string();
    let mut src = err.source();
    while let Some(cause) = src {
        out.push_str(&format!(", caused by: {cause}"));
        src = cause.source();
    }
    out
}
