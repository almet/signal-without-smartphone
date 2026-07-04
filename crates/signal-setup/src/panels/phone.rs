//! Step 1: collect the phone number and request a verification code.

use eframe::egui;
use egui::RichText;
use signal_setup_core as core;

use crate::app::{SignalSetupApp, WorkResult};
use crate::backend;
use crate::theme::*;
use crate::widgets::{format_error_chain, step_header, submit_row};

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

        ui.add_space(18.0);

        let ready = !self.phone.is_empty();
        let clicked = submit_row(ui, ready, "Register");

        if clicked || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && ready)
        {
            let phone = self.phone.clone();
            self.spawn(
                ctx.clone(),
                move || match backend::request_verification_code(&phone, None) {
                    Ok(core::VerificationRequest::CodeSent { session_id }) => {
                        WorkResult::RegisterOk { session_id }
                    }
                    Ok(core::VerificationRequest::CaptchaRequired { session_id }) => {
                        WorkResult::RegisterNeedsCaptcha { session_id }
                    }
                    Err(e) => WorkResult::RegisterError(format_error_chain(&e)),
                },
            );
        }
    }
}
