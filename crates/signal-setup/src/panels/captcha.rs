//! Step 2: shown only when Signal demands a captcha. The user solves it in a
//! browser and pastes the resulting signalcaptcha:// token back here.

use eframe::egui;
use egui::RichText;
use signal_setup_core as core;

use crate::app::{SignalSetupApp, WorkResult};
use crate::backend;
use crate::theme::*;
use crate::widgets::{format_error_chain, instruction_box, open_url, step_header, submit_row};

impl SignalSetupApp {
    pub(crate) fn ui_captcha(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        step_header(ui, "Solve captcha", "Step 2 of 4");

        instruction_box(
            ui,
            &[
                "1. Click the button below to open the captcha page",
                "2. Complete the captcha challenge",
                "3. Right-click \"Open Signal\" > \"Copy link address\"",
                "4. Paste the signalcaptcha:// link in the field below",
            ],
        );

        ui.add_space(12.0);

        if ui
            .add(
                egui::Button::new(
                    RichText::new("🌐  Open captcha page")
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
            open_url("https://signalcaptchas.org/registration/generate.html");
        }

        ui.add_space(12.0);
        ui.label(RichText::new("Captcha token:").color(HEADING).size(14.0));
        ui.add_space(4.0);

        egui::TextEdit::multiline(&mut self.captcha_token)
            .desired_width(f32::INFINITY)
            .desired_rows(2)
            .hint_text("signalcaptcha://signal-hcaptcha....")
            .font(egui::FontId::monospace(11.0))
            .show(ui);

        ui.add_space(12.0);

        let ready = !self.captcha_token.is_empty();
        if submit_row(ui, ready, "Submit captcha") {
            let session_id = self.session_id.clone().unwrap_or_default();
            let token = self.captcha_token.trim().to_string();
            self.spawn(ctx.clone(), move || {
                match backend::submit_captcha(&session_id, &token) {
                    Ok(core::VerificationRequest::CodeSent { session_id }) => {
                        WorkResult::RegisterOk { session_id }
                    }
                    Ok(core::VerificationRequest::CaptchaRequired { session_id }) => {
                        WorkResult::RegisterNeedsCaptcha { session_id }
                    }
                    Err(e) => WorkResult::RegisterError(format_error_chain(&e)),
                }
            });
        }
    }
}
