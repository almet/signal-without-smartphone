//! Step 4: link Signal Desktop. Launch Desktop with this account's profile,
//! then paste (or decode from the clipboard) the device-linking QR code and
//! provision Desktop as a secondary device.

use eframe::egui;
use egui::RichText;
use signal_setup_core::desktop;

use crate::app::{SignalSetupApp, Status, WorkResult};
use crate::backend;
use crate::qr::paste_and_decode_qr;
use crate::theme::*;
use crate::widgets::{
    format_error_chain, instruction_box, show_signal_desktop_status, step_header, submit_row,
};

impl SignalSetupApp {
    pub(crate) fn ui_linking(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        step_header(ui, "Link Signal Desktop", "Step 4 of 4");

        show_signal_desktop_status(ui);
        ui.add_space(12.0);

        instruction_box(
            ui,
            &[
                "1. Click \"Launch Signal Desktop\" below to open it with this account's profile",
                "2. In Signal Desktop, click \"Link to an existing device\"",
                "3. Take a screenshot of the QR code (Cmd+Shift+4 on Mac, Win+Shift+S on Windows)",
                "4. Click \"Paste QR Image\" to automatically decode it",
                "   OR manually scan with a QR app and paste the tsdevice:// link",
            ],
        );

        ui.add_space(12.0);

        let profile = self
            .signal_account
            .as_ref()
            .and_then(|a| a.desktop_profile.clone());
        ui.horizontal(|ui| {
            let can_launch = profile.is_some() && desktop::is_installed();
            if ui
                .add_enabled(
                    can_launch,
                    egui::Button::new(
                        RichText::new("🚀  Launch Signal Desktop")
                            .size(14.0)
                            .color(SIGNAL_BLUE),
                    )
                    .fill(INFO_BG)
                    .stroke(egui::Stroke::new(1.0, INFO_BORDER))
                    .rounding(egui::Rounding::same(8.0))
                    .min_size(egui::vec2(0.0, 36.0)),
                )
                .clicked()
            {
                let profile = profile.clone().unwrap();
                match desktop::launch(&profile) {
                    Ok(()) => {
                        self.status = Status::Info(format!(
                            "Signal Desktop launched (profile: {profile}). Show the QR code, then paste it below."
                        ));
                    }
                    Err(e) => {
                        self.status = Status::Error(format!("Could not launch Signal Desktop: {e}"));
                    }
                }
            }

            if ui
                .add(
                    egui::Button::new(
                        RichText::new("📋  Paste QR Image")
                            .size(14.0)
                            .color(SIGNAL_BLUE),
                    )
                    .fill(INFO_BG)
                    .stroke(egui::Stroke::new(1.0, INFO_BORDER))
                    .rounding(egui::Rounding::same(8.0))
                    .min_size(egui::vec2(0.0, 36.0)),
                )
                .clicked()
            {
                match paste_and_decode_qr() {
                    Ok(uri) => {
                        self.device_uri = uri;
                        self.status = Status::Success("QR code decoded successfully!".into());
                    }
                    Err(e) => {
                        self.status = Status::Error(format!("Failed to decode QR code: {}", e));
                    }
                }
            }
        });

        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "💡 Tip: Make sure the QR code is clearly visible and well-lit in your screenshot",
            )
            .color(MUTED)
            .size(12.0),
        );
        ui.add_space(12.0);

        ui.label(RichText::new("Device link:").color(HEADING).size(14.0));
        ui.add_space(4.0);

        egui::TextEdit::multiline(&mut self.device_uri)
            .desired_width(f32::INFINITY)
            .desired_rows(4)
            .hint_text("tsdevice://?uuid=...")
            .font(egui::FontId::monospace(13.0))
            .show(ui);

        ui.add_space(18.0);

        let ready = !self.device_uri.trim().is_empty() && self.signal_account.is_some();
        if submit_row(ui, ready, "Link device") {
            let account = self.signal_account.clone().unwrap();
            let uri = self.device_uri.trim().to_string();
            self.spawn(ctx.clone(), move || {
                match backend::link_device(&account, &uri) {
                    Ok(()) => WorkResult::LinkOk,
                    Err(e) => WorkResult::LinkError(format_error_chain(&e)),
                }
            });
        }
    }
}
