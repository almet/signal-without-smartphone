//! Step 3: enter the SMS/voice verification code and register the account. If
//! Signal reports an existing transferable account, show the device-transfer
//! prompt instead and let the user skip it to register fresh.

use eframe::egui;
use egui::RichText;
use signal_setup_core as core;

use crate::app::{SignalSetupApp, WorkResult};
use crate::backend;
use crate::theme::*;
use crate::widgets::{format_error_chain, step_header, submit_row};

impl SignalSetupApp {
    pub(crate) fn ui_verify(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // When a device transfer is available, replace the whole form with the prompt.
        if self.device_transfer_available {
            step_header(ui, "Verify phone number", "Step 3 of 4");
            ui.add_space(16.0);

            egui::Frame::none()
                .fill(egui::Color32::from_rgb(255, 247, 237))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgb(253, 186, 116),
                ))
                .rounding(egui::Rounding::same(10.0))
                .inner_margin(egui::Margin::same(14.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("\u{26A0}  Device Transfer Available")
                            .strong()
                            .color(egui::Color32::from_rgb(154, 52, 18))
                            .size(14.0),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(
                            "Your phone number is linked to an existing Signal account whose \
                            device supports data transfer. Signal can copy your message history \
                            directly from the old device to a new one over your local network.\n\n\
                            Since this tool does not implement the transfer receiver, you can \
                            skip the transfer and register a fresh account. Your contacts will \
                            stay intact (they're tied to your phone number), but existing \
                            message history will not be transferred.",
                        )
                        .color(egui::Color32::from_rgb(120, 53, 15))
                        .size(12.0),
                    );
                    ui.add_space(10.0);
                    let skip_clicked = ui
                        .add(
                            egui::Button::new(
                                RichText::new("Skip Transfer & Register Fresh")
                                    .size(13.0)
                                    .color(egui::Color32::WHITE),
                            )
                            .fill(egui::Color32::from_rgb(234, 88, 12))
                            .rounding(egui::Rounding::same(8.0))
                            .min_size(egui::vec2(0.0, 32.0)),
                        )
                        .clicked();

                    if skip_clicked && !self.loading {
                        let phone = self.phone.clone();
                        let session_id = self.session_id.clone().unwrap_or_default();
                        let code = self.verification_code.clone();
                        self.device_transfer_available = false;
                        self.spawn(ctx.clone(), move || {
                            match backend::verify_and_register(&phone, &session_id, &code, true) {
                                Ok(account) => WorkResult::VerifyOk { account },
                                Err(e) => WorkResult::VerifyError(format_error_chain(&e)),
                            }
                        });
                    }
                });
            return;
        }

        step_header(ui, "Verify phone number", "Step 3 of 4");

        ui.label(
            RichText::new(format!("A verification code was sent to {}.", self.phone)).color(MUTED),
        );
        ui.add_space(12.0);

        ui.label(RichText::new("6-digit code:").color(HEADING).size(14.0));
        ui.add_space(4.0);

        let resp = egui::TextEdit::singleline(&mut self.verification_code)
            .desired_width(f32::INFINITY)
            .hint_text("123456")
            .font(egui::FontId::proportional(22.0))
            .show(ui)
            .response;

        ui.add_space(18.0);

        let ready = !self.verification_code.is_empty();
        let clicked = submit_row(ui, ready, "Verify");

        if clicked || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && ready)
        {
            let phone = self.phone.clone();
            let session_id = self.session_id.clone().unwrap_or_default();
            let code = self.verification_code.clone();
            self.device_transfer_available = false;
            self.spawn(ctx.clone(), move || {
                match backend::verify_and_register(&phone, &session_id, &code, false) {
                    Ok(account) => WorkResult::VerifyOk { account },
                    Err(core::SignalError::DeviceTransferAvailable) => {
                        WorkResult::DeviceTransferAvailable
                    }
                    Err(e) => WorkResult::VerifyError(format_error_chain(&e)),
                }
            });
        }
    }
}
