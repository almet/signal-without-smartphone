//! Import an account from a key file exported on another computer, then go
//! straight to linking Signal Desktop with it.

use eframe::egui;
use egui::RichText;
use signal_setup_core::backup;

use crate::app::{SignalSetupApp, Status, Step, WorkResult};
use crate::backend::mode;
use crate::theme::*;
use crate::widgets::{format_error_chain, instruction_box, step_header, submit_row, text_button};

impl SignalSetupApp {
    pub(crate) fn ui_import(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if text_button(ui, "Back", SIGNAL_BLUE) {
                self.status = Status::None;
                self.step = if self.accounts.is_empty() {
                    Step::PhoneInput
                } else {
                    Step::Welcome
                };
            }
        });
        ui.add_space(4.0);
        step_header(
            ui,
            "Import a key file",
            "Use an account registered elsewhere",
        );

        instruction_box(
            ui,
            &[
                "1. Get the key file exported by the person who registered the account",
                "2. Drop it here, or past its contents below",
                "3. Enter the passphrase and click \"Import\"",
                "4. Then link your own Signal Desktop to the account",
            ],
        );
        ui.add_space(12.0);

        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        let label = match &self.import_file_name {
            Some(name) => format!("Key file: {name}"),
            None if hovering => "Drop the key file here".into(),
            None => "Key file (path or contents):".into(),
        };
        ui.label(RichText::new(label).color(HEADING).size(14.0));
        ui.add_space(4.0);

        let text_response = egui::TextEdit::multiline(&mut self.import_text)
            .desired_width(f32::INFINITY)
            .desired_rows(4)
            .hint_text("/path/to/signal-account-+15551234567.json  or  { \"format\": ... }")
            .font(egui::FontId::monospace(11.0))
            .show(ui)
            .response;
        if text_response.changed() {
            self.import_file_name = None;
        }

        ui.add_space(8.0);
        ui.label(RichText::new("Passphrase:").color(HEADING).size(14.0));
        ui.add_space(4.0);
        egui::TextEdit::singleline(&mut self.import_passphrase)
            .password(true)
            .desired_width(f32::INFINITY)
            .show(ui);

        ui.add_space(18.0);
        let ready = !self.import_text.trim().is_empty() && !self.import_passphrase.is_empty();
        if submit_row(ui, ready, "Import") {
            let input = self.import_text.clone();
            let passphrase = self.import_passphrase.clone();
            let environment = mode().environment_label();
            self.spawn(ctx.clone(), move || {
                import_from_input(&input, &passphrase, environment)
            });
        }
    }
}

/// `input` is either the key file contents or a path to it.
fn import_from_input(input: &str, passphrase: &str, environment: &str) -> WorkResult {
    let trimmed = input.trim();
    let contents = if trimmed.starts_with('{') {
        trimmed.to_string()
    } else {
        let path = trimmed.trim_matches(|c| c == '"' || c == '\'');
        match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                return WorkResult::ImportError(format!(
                    "Could not read {path}: {e}. Paste the key file contents instead."
                ))
            }
        }
    };

    // Don't import staging into production and vice versa.
    match backup::import_account(&contents, passphrase) {
        Ok(imported) if imported.environment != environment => WorkResult::ImportError(format!(
            "This key file was exported from the {} environment, but this tool is running \
             against {environment}.",
            imported.environment
        )),
        Ok(imported) => WorkResult::ImportOk {
            account: imported.account,
        },
        Err(e) => WorkResult::ImportError(format_error_chain(&e)),
    }
}
