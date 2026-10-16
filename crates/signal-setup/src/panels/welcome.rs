//! Welcome screen: shown at startup when saved accounts exist. Lets the user
//! launch Signal Desktop for a saved account, link another Desktop to it,
//! open its management screen, forget it, import an account from a key
//! file, or start registering a new number.

use eframe::egui;
use egui::RichText;
use signal_setup_core::SignalAccount;
use signal_setup_core::{desktop, persistence, types};

use crate::app::{SignalSetupApp, Status, Step, WorkResult};
use crate::backend;
use crate::theme::*;
use crate::widgets::{primary_button, secondary_button, step_header, text_button};

const STALE_AFTER_DAYS: u64 = 14;

fn days_since_last_seen_refresh(account: &SignalAccount) -> Option<u64> {
    let last = account.last_seen_refreshed_at?;
    Some(types::now_unix().saturating_sub(last) / 86_400)
}

impl SignalSetupApp {
    pub(crate) fn ui_welcome(&mut self, ui: &mut egui::Ui) {
        step_header(ui, "Welcome back", "Saved Signal accounts");

        if self.accounts.is_empty() {
            // No accounts left, so drop straight into the registration flow.
            self.step = Step::PhoneInput;
            return;
        }

        ui.label(
            RichText::new(
                "Launch Signal Desktop for an account, link another computer to it, \
                 or manage its devices, registration lock and key file.",
            )
            .color(MUTED),
        );
        ui.add_space(12.0);

        enum WelcomeAction {
            Launch(String),
            Link(SignalAccount),
            Manage(SignalAccount),
            Forget(String),
            RefreshLastSeen(SignalAccount),
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
                        if account.registration_lock {
                            ui.label(
                                RichText::new("🔒 registration lock on")
                                    .size(12.0)
                                    .color(MUTED),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if text_button(ui, "Forget", ERROR_RED) {
                                action = Some(WelcomeAction::Forget(account.phone.clone()));
                            }
                        });
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let can_launch =
                            account.desktop_profile.is_some() && desktop::is_installed();
                        if primary_button(ui, "Launch", can_launch) {
                            if let Some(p) = account.desktop_profile.clone() {
                                action = Some(WelcomeAction::Launch(p));
                            }
                        }
                        if secondary_button(ui, "Link a Desktop", true) {
                            action = Some(WelcomeAction::Link(account.clone()));
                        }
                        if secondary_button(ui, "Manage", true) {
                            action = Some(WelcomeAction::Manage(account.clone()));
                        }
                    });

                    let days = days_since_last_seen_refresh(account);
                    let stale = days.is_none_or(|d| d >= STALE_AFTER_DAYS);
                    if stale {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let message = match days {
                                Some(d) => {
                                    format!("⚠ Signal has not seen this account in {d} days.")
                                }
                                None => "⚠ Signal may not have seen this account in a while."
                                    .to_string(),
                            };
                            ui.label(RichText::new(message).color(WARNING_AMBER).size(13.0));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new("Keep account active")
                                                    .color(egui::Color32::WHITE)
                                                    .size(13.0),
                                            )
                                            .fill(WARNING_AMBER)
                                            .rounding(egui::Rounding::same(6.0))
                                            .min_size(egui::vec2(140.0, 28.0)),
                                        )
                                        .clicked()
                                    {
                                        action =
                                            Some(WelcomeAction::RefreshLastSeen(account.clone()));
                                    }
                                },
                            );
                        });
                    } else if let Some(d) = days {
                        ui.add_space(4.0);
                        let label = match d {
                            0 => "Signal last saw this account today.".to_string(),
                            1 => "Signal last saw this account yesterday.".to_string(),
                            d => format!("Signal last saw this account {d} days ago."),
                        };
                        ui.label(RichText::new(label).color(MUTED).size(12.0));
                    }
                });
            ui.add_space(8.0);
        }

        ui.add_space(8.0);
        let mut register = false;
        let mut import = false;
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Button::new(RichText::new("Register a new number").size(15.0))
                        .rounding(egui::Rounding::same(8.0))
                        .min_size(egui::vec2(200.0, 40.0)),
                )
                .clicked()
            {
                register = true;
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("Import a key file").size(15.0))
                        .rounding(egui::Rounding::same(8.0))
                        .min_size(egui::vec2(160.0, 40.0)),
                )
                .clicked()
            {
                import = true;
            }
        });
        if register {
            self.phone.clear();
            self.signal_account = None;
            self.status = Status::None;
            self.step = Step::PhoneInput;
            return;
        }
        if import {
            self.status = Status::None;
            self.step = Step::Import;
            return;
        }

        match action {
            Some(WelcomeAction::Launch(profile)) => {
                if let Err(e) = desktop::launch(&profile) {
                    self.status = Status::Error(format!("Could not launch Signal Desktop: {e}"));
                } else {
                    self.status =
                        Status::Info(format!("Signal Desktop launched (profile: {profile})."));
                }
            }
            Some(WelcomeAction::Link(account)) => {
                self.start_linking(
                    account,
                    "Using the saved account. Paste a linking QR code from Signal Desktop.",
                );
            }
            Some(WelcomeAction::Manage(account)) => {
                let ctx = ui.ctx().clone();
                self.open_account(ctx, account);
            }
            Some(WelcomeAction::Forget(phone)) => {
                if let Err(e) = persistence::delete(&phone) {
                    self.status = Status::Error(format!("Could not remove the account: {e}"));
                } else {
                    self.forget_account(&phone);
                    self.status = Status::Success(format!(
                        "Forgot {phone} on this computer. The Signal account and its linked \
                         devices keep working."
                    ));
                }
            }
            Some(WelcomeAction::RefreshLastSeen(mut account)) => {
                let ctx = ui.ctx().clone();
                self.status = Status::None;
                self.spawn(ctx, move || {
                    match backend::refresh_last_seen(&mut account) {
                        Ok(()) => WorkResult::RefreshLastSeenOk { account },
                        Err(signal_setup_core::SignalError::Api { status: 401, .. }) => {
                            WorkResult::RefreshLastSeenError(
                                "Signal no longer recognizes this account. \
                             It was probably deregistered; register it again."
                                    .into(),
                            )
                        }
                        Err(e) => WorkResult::RefreshLastSeenError(e.to_string()),
                    }
                });
            }
            None => {}
        }
    }
}
