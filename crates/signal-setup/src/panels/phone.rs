//! Step 1: collect the phone number and request a verification code.

use eframe::egui;
use egui::RichText;

use crate::app::SignalSetupApp;
use crate::backend;
use crate::theme::*;
use crate::widgets::{step_header, submit_row, text_button};

impl SignalSetupApp {
    pub(crate) fn ui_phone(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        step_header(ui, "Phone number", "Step 1 of 4");

        ui.label(RichText::new("Enter your phone number with country code:").color(MUTED));
        ui.add_space(6.0);

        let resp = egui::TextEdit::singleline(&mut self.phone)
            .desired_width(f32::INFINITY)
            .hint_text("+1234567890")
            .font(egui::FontId::proportional(17.0))
            .show(ui)
            .response;

        ui.add_space(12.0);

        // Signal's phone number discovery is opt-in
        ui.checkbox(
            &mut self.discoverable_by_phone_number,
            RichText::new("Allow others to find this account by phone number")
                .color(MUTED)
                .size(13.0),
        );
        ui.checkbox(
            &mut self.registration_lock,
            RichText::new("Block re-registration of this number (registration lock)")
                .color(MUTED)
                .size(13.0),
        );
        ui.label(
            RichText::new(
                "With the lock on, receiving this number's SMS is not enough to take over \
                 the account: Signal also asks for this account's keys. Export a key file \
                 afterwards so you keep them.",
            )
            .color(MUTED)
            .size(12.0),
        );

        let already_known = self.accounts.iter().any(|a| a.phone == self.phone.trim());
        if already_known {
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "⚠ This number is already saved here. Registering again creates a new \
                     account with new keys: every linked Signal Desktop will be unlinked.",
                )
                .color(WARNING_AMBER)
                .size(12.0),
            );
        }

        ui.add_space(18.0);

        let ready = !self.phone.is_empty();
        let clicked = submit_row(ui, ready, "Register");

        if !self.accounts.is_empty() || self.step == crate::app::Step::PhoneInput {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if text_button(
                    ui,
                    "Have a key file from someone else? Import it",
                    SIGNAL_BLUE,
                ) {
                    self.status = crate::app::Status::None;
                    self.step = crate::app::Step::Import;
                }
            });
        }

        if clicked || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && ready)
        {
            let phone = self.phone.clone();
            self.spawn(ctx.clone(), move || {
                SignalSetupApp::code_request_result(backend::request_verification_code(
                    &phone, None,
                ))
            });
        }
    }
}
