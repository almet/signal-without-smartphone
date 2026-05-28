//! Final screen after a successful link: launch Signal Desktop, re-link with a
//! fresh QR, or finish and reset back to the start.

use eframe::egui;
use egui::RichText;
use signal_setup_core::desktop;

use crate::app::{SignalSetupApp, Status, Step};
use crate::theme::*;

impl SignalSetupApp {
    pub(crate) fn ui_complete(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);

        // Snapshot the account ahead of the closure; both the launch and
        // re-link buttons need it, and the closure borrows `self` mutably.
        let profile = self
            .signal_account
            .as_ref()
            .and_then(|a| a.desktop_profile.clone());

        enum CompleteAction {
            Launch,
            Relink,
            Done,
        }
        let mut action: Option<CompleteAction> = None;

        ui.vertical_centered(|ui| {
            ui.label(RichText::new("🎉").size(52.0));
            ui.add_space(12.0);
            ui.label(
                RichText::new("Setup complete!")
                    .size(24.0)
                    .color(SUCCESS_GREEN)
                    .strong(),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("Your Signal Desktop is now linked. The first run might take some time to sync, but should be ready soon!")
                    .size(15.0)
                    .color(MUTED),
            );
            ui.add_space(28.0);

            ui.horizontal(|ui| {
                let can_launch = profile.is_some() && desktop::is_installed();
                if ui
                    .add_enabled(
                        can_launch,
                        egui::Button::new(
                            RichText::new("🚀  Launch Signal Desktop")
                                .size(14.0)
                                .color(egui::Color32::WHITE),
                        )
                        .fill(SIGNAL_BLUE)
                        .rounding(egui::Rounding::same(8.0))
                        .min_size(egui::vec2(200.0, 40.0)),
                    )
                    .clicked()
                {
                    action = Some(CompleteAction::Launch);
                }

                if ui
                    .add(
                        egui::Button::new(RichText::new("Re-link").size(14.0).color(HEADING))
                            .fill(INSET_BG)
                            .stroke(egui::Stroke::new(1.0, BORDER))
                            .rounding(egui::Rounding::same(8.0))
                            .min_size(egui::vec2(120.0, 40.0)),
                    )
                    .clicked()
                {
                    action = Some(CompleteAction::Relink);
                }
            });

            ui.add_space(12.0);
            if ui
                .add(
                    egui::Button::new(RichText::new("Done").size(13.0).color(MUTED)).frame(false),
                )
                .clicked()
            {
                action = Some(CompleteAction::Done);
            }
        });
        ui.add_space(16.0);

        match action {
            Some(CompleteAction::Launch) => {
                if let Some(p) = profile {
                    if let Err(e) = desktop::launch(&p) {
                        self.status =
                            Status::Error(format!("Could not launch Signal Desktop: {e}"));
                    }
                }
            }
            Some(CompleteAction::Relink) => {
                // Keep the signal_account; clear the QR text and go back to
                // step 4 so the user can paste a fresh QR.
                self.device_uri.clear();
                self.status = Status::Info(
                    "Re-linking. Show a fresh QR code from Signal Desktop, then paste it below."
                        .into(),
                );
                self.step = Step::Linking;
            }
            Some(CompleteAction::Done) => {
                *self = SignalSetupApp::default();
            }
            None => {}
        }
    }
}
