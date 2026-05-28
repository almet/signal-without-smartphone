//! Welcome screen: shown at startup when saved accounts exist. Lets the user
//! launch or re-link Signal Desktop for a saved account, delete one, or start
//! registering a new number.

use eframe::egui;
use egui::RichText;
use signal_setup_core::{desktop, persistence};
use signal_setup_core::SignalAccount;

use crate::app::{SignalSetupApp, Status, Step};
use crate::theme::*;
use crate::widgets::step_header;

impl SignalSetupApp {
    pub(crate) fn ui_welcome(&mut self, ui: &mut egui::Ui) {
        step_header(ui, "Welcome back", "Saved Signal accounts");

        if self.accounts.is_empty() {
            // No accounts left, so drop straight into the registration flow.
            self.step = Step::PhoneInput;
            return;
        }

        ui.label(
            RichText::new("Pick an account to re-link Signal Desktop, or register a new one:")
                .color(MUTED),
        );
        ui.add_space(12.0);

        enum WelcomeAction {
            Launch(String),
            Relink(SignalAccount),
            Delete(String),
        }
        let mut action: Option<WelcomeAction> = None;

        for account in &self.accounts {
            egui::Frame::none()
                .fill(INSET_BG)
                .stroke(egui::Stroke::new(1.0, BORDER))
                .rounding(egui::Rounding::same(8.0))
                .inner_margin(egui::Margin::same(12.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&account.phone)
                                .size(17.0)
                                .color(HEADING)
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Delete").color(ERROR_RED).size(13.0),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                action = Some(WelcomeAction::Delete(account.phone.clone()));
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Re-link Desktop")
                                            .color(HEADING)
                                            .size(14.0),
                                    )
                                    .fill(INSET_BG)
                                    .stroke(egui::Stroke::new(1.0, BORDER))
                                    .rounding(egui::Rounding::same(6.0))
                                    .min_size(egui::vec2(120.0, 32.0)),
                                )
                                .clicked()
                            {
                                action = Some(WelcomeAction::Relink(account.clone()));
                            }
                            let can_launch = account.desktop_profile.is_some()
                                && desktop::is_installed();
                            if ui
                                .add_enabled(
                                    can_launch,
                                    egui::Button::new(
                                        RichText::new("🚀  Launch")
                                            .color(egui::Color32::WHITE)
                                            .size(14.0),
                                    )
                                    .fill(SIGNAL_BLUE)
                                    .rounding(egui::Rounding::same(6.0))
                                    .min_size(egui::vec2(120.0, 32.0)),
                                )
                                .clicked()
                            {
                                if let Some(p) = account.desktop_profile.clone() {
                                    action = Some(WelcomeAction::Launch(p));
                                }
                            }
                        });
                    });
                });
            ui.add_space(8.0);
        }

        ui.add_space(8.0);
        if ui
            .add(
                egui::Button::new(RichText::new("Register a new number").size(15.0))
                    .rounding(egui::Rounding::same(8.0))
                    .min_size(egui::vec2(200.0, 40.0)),
            )
            .clicked()
        {
            self.phone.clear();
            self.signal_account = None;
            self.status = Status::None;
            self.step = Step::PhoneInput;
            return;
        }

        match action {
            Some(WelcomeAction::Launch(profile)) => {
                if let Err(e) = desktop::launch(&profile) {
                    self.status =
                        Status::Error(format!("Could not launch Signal Desktop: {e}"));
                } else {
                    self.status =
                        Status::Info(format!("Signal Desktop launched (profile: {profile})."));
                }
            }
            Some(WelcomeAction::Relink(account)) => {
                self.phone = account.phone.clone();
                self.signal_account = Some(account);
                self.status = Status::Info(
                    "Using the saved account. Paste a new linking QR code from Signal Desktop."
                        .into(),
                );
                self.step = Step::Linking;
            }
            Some(WelcomeAction::Delete(phone)) => {
                if let Err(e) = persistence::delete(&phone) {
                    self.status = Status::Error(format!("Could not delete account: {e}"));
                } else {
                    self.accounts.retain(|a| a.phone != phone);
                    self.status = Status::Success(format!("Deleted {phone}."));
                }
            }
            None => {}
        }
    }
}
