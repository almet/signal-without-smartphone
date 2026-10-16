//! Account management screen: linked devices (with unlink), registration
//! lock, key file export, removal from this computer, and deletion of the
//! Signal account itself.

use eframe::egui;
use egui::RichText;
use signal_setup_core::{
    backup, desktop, persistence, LinkedDevice, SignalAccount, MAX_LINKED_DEVICES,
};

use crate::app::{SignalSetupApp, Status, Step, WorkResult};
use crate::backend::{self, mode};
use crate::theme::*;
use crate::widgets::{
    days_ago, format_error_chain, primary_button, secondary_button, section, step_header,
    text_button,
};

fn export_dir() -> Option<std::path::PathBuf> {
    dirs::download_dir().or_else(dirs::home_dir)
}

impl SignalSetupApp {
    pub(crate) fn ui_account(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(account) = self.selected_account() else {
            self.step = Step::Welcome;
            return;
        };

        enum AccountAction {
            Back,
            Launch,
            Link,
            RefreshDevices,
            Unlink(u32),
            SetLock(bool),
            Export,
            CopyExport,
            Forget,
            DeleteAccount,
            CancelDeleteAccount,
        }
        let mut action: Option<AccountAction> = None;

        ui.horizontal(|ui| {
            if text_button(ui, "Back to all accounts", SIGNAL_BLUE) {
                action = Some(AccountAction::Back);
            }
        });
        ui.add_space(4.0);
        step_header(ui, &account.phone, "Manage this account");

        section(ui, "Signal Desktop on this computer", |ui| {
            ui.label(
                RichText::new(
                    "Each account gets its own Signal Desktop profile, so several \
                     accounts can run side by side on this computer.",
                )
                .color(MUTED)
                .size(13.0),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let can_launch = account.desktop_profile.is_some() && desktop::is_installed();
                if primary_button(ui, "Launch Signal Desktop", can_launch) {
                    action = Some(AccountAction::Launch);
                }
                if secondary_button(ui, "Link a Signal Desktop", true) {
                    action = Some(AccountAction::Link);
                }
            });
        });

        section(ui, "Linked devices", |ui| {
            match &self.devices {
                None => {
                    ui.label(RichText::new("Loading…").color(MUTED).size(13.0));
                }
                Some(devices) => {
                    let linked: Vec<&LinkedDevice> =
                        devices.iter().filter(|d| !d.is_primary()).collect();
                    ui.label(
                        RichText::new(format!(
                            "{} of {MAX_LINKED_DEVICES} seats used.",
                            linked.len()
                        ))
                        .color(MUTED)
                        .size(13.0),
                    );
                    ui.add_space(6.0);
                    if linked.is_empty() {
                        ui.label(
                            RichText::new("No Signal Desktop linked yet.")
                                .color(HEADING)
                                .size(14.0),
                        );
                    }
                    for device in linked {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                let name = device
                                    .name
                                    .clone()
                                    .unwrap_or_else(|| format!("Device {}", device.id));
                                ui.label(RichText::new(name).color(HEADING).size(14.0).strong());
                                ui.label(
                                    RichText::new(format!(
                                        "Device {} · linked {} · last seen {}",
                                        device.id,
                                        days_ago(device.created),
                                        days_ago(device.last_seen)
                                    ))
                                    .color(MUTED)
                                    .size(12.0),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if text_button(ui, "Unlink", ERROR_RED) {
                                        action = Some(AccountAction::Unlink(device.id));
                                    }
                                },
                            );
                        });
                        ui.add_space(4.0);
                    }
                }
            }
            ui.add_space(6.0);
            if text_button(ui, "Refresh", SIGNAL_BLUE) {
                action = Some(AccountAction::RefreshDevices);
            }
        });

        section(ui, "Registration lock", |ui| {
            ui.label(
                RichText::new(
                    "While the lock is on, nobody can register this number on another \
                     Signal client, unless they hold this account's keys. \
                     Signal releases the lock after seven days without any \
                     device connecting, so keep the account active!",
                )
                .color(MUTED)
                .size(13.0),
            );
            ui.add_space(8.0);
            let mut enabled = account.registration_lock;
            if ui
                .checkbox(
                    &mut enabled,
                    RichText::new("Block re-registration of this number")
                        .color(HEADING)
                        .size(14.0),
                )
                .changed()
            {
                action = Some(AccountAction::SetLock(enabled));
            }
        });

        section(ui, "Key file", |ui| {
            ui.label(
                RichText::new(
                    "Export this account's keys to an encrypted file. Keep it as a backup!",
                )
                .color(MUTED)
                .size(13.0),
            );
            ui.add_space(8.0);
            ui.label(RichText::new("Passphrase:").color(HEADING).size(13.0));
            egui::TextEdit::singleline(&mut self.export_passphrase)
                .password(true)
                .desired_width(f32::INFINITY)
                .show(ui);
            ui.label(
                RichText::new("Confirm passphrase:")
                    .color(HEADING)
                    .size(13.0),
            );
            egui::TextEdit::singleline(&mut self.export_passphrase_confirm)
                .password(true)
                .desired_width(f32::INFINITY)
                .show(ui);
            ui.add_space(6.0);

            let long_enough = self.export_passphrase.chars().count() >= 8;
            let matching = self.export_passphrase == self.export_passphrase_confirm;
            if !self.export_passphrase.is_empty() && !long_enough {
                ui.label(
                    RichText::new("Use at least 8 characters.")
                        .color(WARNING_AMBER)
                        .size(12.0),
                );
            } else if !self.export_passphrase_confirm.is_empty() && !matching {
                ui.label(
                    RichText::new("The passphrases differ.")
                        .color(WARNING_AMBER)
                        .size(12.0),
                );
            }
            ui.horizontal(|ui| {
                if primary_button(ui, "Export key file", long_enough && matching) {
                    action = Some(AccountAction::Export);
                }
                if self.export_result.is_some() && secondary_button(ui, "Copy to clipboard", true) {
                    action = Some(AccountAction::CopyExport);
                }
            });
            if let Some((Some(path), _)) = &self.export_result {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("Saved as {}", path.display()))
                        .color(MUTED)
                        .size(12.0),
                );
            }
        });

        section(ui, "Remove this account", |ui| {
            ui.label(
                RichText::new(
                    "Forget on this computer: the keys are removed from here, but the \
                     Signal account and its linked devices keep working. Export a key \
                     file first if you may need them again.",
                )
                .color(MUTED)
                .size(13.0),
            );
            ui.add_space(6.0);
            if text_button(ui, "Forget this account on this computer", ERROR_RED) {
                action = Some(AccountAction::Forget);
            }

            ui.add_space(10.0);
            ui.label(
                RichText::new(
                    "Delete the Signal account: the number is unregistered from Signal, \
                     every linked Signal Desktop stops working. This cannot be undone.",
                )
                .color(MUTED)
                .size(13.0),
            );
            ui.add_space(6.0);
            if self.confirm_account_deletion {
                ui.label(
                    RichText::new(format!(
                        "⚠ Really delete the Signal account {}? ",
                        account.phone
                    ))
                    .color(WARNING_AMBER)
                    .size(13.0),
                );
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Yes, delete the Signal account")
                                    .color(egui::Color32::WHITE)
                                    .size(14.0),
                            )
                            .fill(ERROR_RED)
                            .rounding(egui::Rounding::same(6.0))
                            .min_size(egui::vec2(0.0, 32.0)),
                        )
                        .clicked()
                    {
                        action = Some(AccountAction::DeleteAccount);
                    }
                    if secondary_button(ui, "Cancel", true) {
                        action = Some(AccountAction::CancelDeleteAccount);
                    }
                });
            } else if text_button(ui, "Delete the Signal account", ERROR_RED) {
                self.confirm_account_deletion = true;
            }
        });

        match action {
            Some(AccountAction::Back) => {
                self.selected_phone = None;
                self.devices = None;
                self.status = Status::None;
                self.step = Step::Welcome;
            }
            Some(AccountAction::Launch) => {
                if let Some(p) = account.desktop_profile.as_deref() {
                    match desktop::launch(p) {
                        Ok(()) => {
                            self.status =
                                Status::Info(format!("Signal Desktop launched (profile: {p})."))
                        }
                        Err(e) => {
                            self.status =
                                Status::Error(format!("Could not launch Signal Desktop: {e}"))
                        }
                    }
                }
            }
            Some(AccountAction::Link) => {
                self.start_linking(
                    account,
                    "Paste a linking QR code from Signal Desktop to add it as a device.",
                );
            }
            Some(AccountAction::RefreshDevices) => {
                self.devices = None;
                self.load_devices(ctx.clone(), account);
            }
            Some(AccountAction::Unlink(device_id)) => {
                self.spawn(ctx.clone(), move || {
                    match backend::unlink_device(&account, device_id) {
                        Ok(()) => WorkResult::UnlinkOk {
                            phone: account.phone.clone(),
                            device_id,
                        },
                        Err(e) => WorkResult::UnlinkError(format_error_chain(&e)),
                    }
                });
            }
            Some(AccountAction::SetLock(enabled)) => {
                let mut account = account;
                self.spawn(ctx.clone(), move || {
                    match backend::set_registration_lock(&mut account, enabled) {
                        Ok(()) => WorkResult::LockOk { account },
                        Err(e) => WorkResult::LockError(format_error_chain(&e)),
                    }
                });
            }
            Some(AccountAction::Export) => {
                let passphrase = self.export_passphrase.clone();
                let environment = mode().environment_label();
                self.export_result = None;
                self.spawn(ctx.clone(), move || {
                    export_account_to_file(&account, &passphrase, environment)
                });
            }
            Some(AccountAction::CopyExport) => {
                if let Some((_, json)) = &self.export_result {
                    match arboard::Clipboard::new().and_then(|mut c| c.set_text(json.clone())) {
                        Ok(()) => {
                            self.status =
                                Status::Success("Key file copied to the clipboard.".into())
                        }
                        Err(e) => {
                            self.status = Status::Error(format!("Could not copy to clipboard: {e}"))
                        }
                    }
                }
            }
            Some(AccountAction::Forget) => {
                if let Err(e) = persistence::delete(&account.phone) {
                    self.status = Status::Error(format!("Could not remove the account: {e}"));
                } else {
                    self.forget_account(&account.phone);
                    self.status =
                        Status::Success(format!("Forgot {} on this computer.", account.phone));
                }
            }
            Some(AccountAction::DeleteAccount) => {
                self.status = Status::Info("Deleting the Signal account…".into());
                self.spawn(ctx.clone(), move || {
                    match backend::delete_account(&account) {
                        Ok(()) => WorkResult::AccountDeletedOk {
                            phone: account.phone.clone(),
                        },
                        Err(e) => WorkResult::AccountDeleteError(format_error_chain(&e)),
                    }
                });
            }
            Some(AccountAction::CancelDeleteAccount) => {
                self.confirm_account_deletion = false;
            }
            None => {}
        }
    }
}

fn export_account_to_file(
    account: &SignalAccount,
    passphrase: &str,
    environment: &str,
) -> WorkResult {
    let json = match backup::export_account(account, passphrase, environment) {
        Ok(json) => json,
        Err(e) => return WorkResult::ExportError(format_error_chain(&e)),
    };
    let path = export_dir().map(|d| d.join(backup::suggested_file_name(account)));
    let written = match &path {
        Some(p) => std::fs::write(p, &json)
            .map_err(|e| eprintln!("Could not write key file to {}: {e}", p.display()))
            .is_ok(),
        None => false,
    };
    WorkResult::ExportOk {
        path: if written { path } else { None },
        json,
    }
}
