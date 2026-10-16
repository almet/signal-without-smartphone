use eframe::egui;
use egui::RichText;
use signal_setup_core::{desktop, persistence};
use signal_setup_core::{LinkedDevice, SignalAccount};
use std::sync::mpsc;

use crate::backend;
use crate::theme::*;
use crate::widgets::{draw_step_indicator, format_error_chain, show_status};

#[derive(Default, PartialEq, Clone, Copy)]
pub(crate) enum Step {
    /// The welcome screen is only shown when a previous account was linked
    Welcome,
    Account,
    #[default]
    PhoneInput,
    Captcha,
    Verification,
    Linking,
    Complete,
}

impl Step {
    /// Position in the registration step indicator. `0` means the step is
    /// outside the registration flow and shows no indicator.
    fn number(self) -> usize {
        match self {
            Step::Welcome | Step::Account => 0,
            Step::PhoneInput => 1,
            Step::Captcha => 2,
            Step::Verification => 3,
            Step::Linking => 4,
            Step::Complete => 5,
        }
    }
}

// Background-work result types

pub(crate) enum WorkResult {
    RegisterOk {
        session_id: String,
    },
    RegisterNeedsCaptcha {
        session_id: String,
    },
    RegisterError(String),
    VerifyOk {
        account: SignalAccount,
        warning: Option<String>,
    },
    VerifyError(String),
    DeviceTransferAvailable,
    LinkOk {
        device: LinkedDevice,
    },
    LinkError(String),
    RefreshLastSeenOk {
        account: SignalAccount,
    },
    RefreshLastSeenError(String),
    DevicesOk {
        phone: String,
        devices: Vec<LinkedDevice>,
    },
    DevicesError(String),
    UnlinkOk {
        phone: String,
        device_id: u32,
    },
    UnlinkError(String),
    LockOk {
        account: SignalAccount,
    },
    LockError(String),
    ExportOk {
        path: Option<std::path::PathBuf>,
        json: String,
    },
    ExportError(String),
}

#[derive(Default, Clone)]
pub(crate) enum Status {
    #[default]
    None,
    Info(String),
    Success(String),
    Error(String),
}

#[derive(Default)]
pub(crate) struct SignalSetupApp {
    pub(crate) step: Step,
    pub(crate) phone: String,
    pub(crate) captcha_token: String,
    pub(crate) verification_code: String,
    pub(crate) device_uri: String,
    pub(crate) status: Status,
    pub(crate) loading: bool,
    /// Session ID returned by the verification session API.
    pub(crate) session_id: Option<String>,
    /// Account key material after successful registration.
    pub(crate) signal_account: Option<SignalAccount>,
    pub(crate) result_rx: Option<mpsc::Receiver<WorkResult>>,
    /// Set to true when an existing account supports device transfer.
    pub(crate) device_transfer_available: bool,
    /// Did we require a captcha in this session?
    pub(crate) captcha_was_required: bool,
    /// Whether other Signal users can find this account by its phone number.
    /// Off by default: new accounts are registered as not discoverable.
    pub(crate) discoverable_by_phone_number: bool,
    /// Protect the number with a registration lock.
    pub(crate) registration_lock: bool,
    /// The (cached) list of accounts. Stored here to avoid re-reading from the keychain.
    pub(crate) accounts: Vec<SignalAccount>,
    pub(crate) selected_phone: Option<String>,
    pub(crate) devices: Option<Vec<LinkedDevice>>,
    pub(crate) export_passphrase: String,
    pub(crate) export_passphrase_confirm: String,
    /// Last export location and content (for the clipboard).
    pub(crate) export_result: Option<(Option<std::path::PathBuf>, String)>,
}

impl SignalSetupApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        let mut app = Self::fresh();
        app.refresh_accounts();
        if !app.accounts.is_empty() {
            app.step = Step::Welcome;
        }
        app
    }

    /// Default state with the non-`false` defaults applied.
    pub(crate) fn fresh() -> Self {
        Self {
            registration_lock: true,
            ..Self::default()
        }
    }

    pub(crate) fn reset(&mut self) {
        let accounts = std::mem::take(&mut self.accounts);
        *self = Self::fresh();
        self.accounts = accounts;
        if !self.accounts.is_empty() {
            self.step = Step::Welcome;
        }
    }

    pub(crate) fn remember_account(&mut self, account: &SignalAccount) {
        if let Err(e) = persistence::save(account) {
            eprintln!("Warning: could not save account to disk: {e}");
            self.status = Status::Error(format!("Could not save the account: {e}"));
        }
        self.accounts.retain(|a| a.phone != account.phone);
        self.accounts.push(account.clone());
    }

    pub(crate) fn assign_desktop_profile(&self, account: &mut SignalAccount) {
        if account.desktop_profile.is_some() {
            return;
        }
        let taken: Vec<String> = self
            .accounts
            .iter()
            .filter(|a| a.phone != account.phone)
            .filter_map(|a| a.desktop_profile.clone())
            .collect();
        account.desktop_profile = Some(desktop::choose_profile_for_new_account(
            &account.phone,
            &taken,
        ));
    }

    pub(crate) fn open_account(&mut self, ctx: egui::Context, account: SignalAccount) {
        self.selected_phone = Some(account.phone.clone());
        self.devices = None;
        self.export_passphrase.clear();
        self.export_passphrase_confirm.clear();
        self.export_result = None;
        self.status = Status::None;
        self.step = Step::Account;
        self.load_devices(ctx, account);
    }

    pub(crate) fn load_devices(&mut self, ctx: egui::Context, account: SignalAccount) {
        self.spawn(ctx, move || match backend::list_devices(&account) {
            Ok(devices) => WorkResult::DevicesOk {
                phone: account.phone.clone(),
                devices,
            },
            Err(e) => WorkResult::DevicesError(format_error_chain(&e)),
        });
    }

    pub(crate) fn start_linking(&mut self, account: SignalAccount, message: &str) {
        self.phone = account.phone.clone();
        self.signal_account = Some(account);
        self.device_uri.clear();
        self.status = Status::Info(message.into());
        self.step = Step::Linking;
    }

    pub(crate) fn selected_account(&self) -> Option<SignalAccount> {
        let phone = self.selected_phone.as_deref()?;
        self.accounts.iter().find(|a| a.phone == phone).cloned()
    }

    /// Reload the cached account list from keyring.
    pub(crate) fn refresh_accounts(&mut self) {
        match persistence::list() {
            Ok(accounts) => self.accounts = accounts,
            Err(e) => {
                self.status = Status::Error(format!("Could not read saved accounts: {e}"));
            }
        }
    }

    pub(crate) fn spawn<F>(&mut self, ctx: egui::Context, f: F)
    where
        F: FnOnce() -> WorkResult + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        self.result_rx = Some(rx);
        self.loading = true;
        std::thread::spawn(move || {
            let _ = tx.send(f());
            ctx.request_repaint();
        });
    }

    pub(crate) fn poll_result(&mut self) -> Option<WorkResult> {
        let result = self.result_rx.as_ref()?.try_recv().ok()?;
        self.result_rx = None;
        self.loading = false;
        Some(result)
    }

    /// Make the UI display a previous step, invalidating all previous steps.
    /// (This is a bit brittle because it needs to be updated manually).
    pub(crate) fn jump_back_to_step(&mut self, target: usize) {
        // Drop any in-flight worker; its result would land in the wrong step.
        self.result_rx = None;
        self.loading = false;
        self.status = Status::None;

        match target {
            1 => {
                self.captcha_token.clear();
                self.verification_code.clear();
                self.device_uri.clear();
                self.session_id = None;
                self.signal_account = None;
                self.device_transfer_available = false;
                self.captcha_was_required = false;
                self.step = Step::PhoneInput;
            }
            2 => {
                self.verification_code.clear();
                self.device_uri.clear();
                self.signal_account = None;
                self.device_transfer_available = false;
                self.step = Step::Captcha;
            }
            3 => {
                self.device_uri.clear();
                self.signal_account = None;
                self.device_transfer_available = false;
                self.step = Step::Verification;
            }
            4 => {
                self.step = Step::Linking;
            }
            _ => {}
        }
    }
}

impl eframe::App for SignalSetupApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.loading {
            ctx.request_repaint();
        }

        if let Some(result) = self.poll_result() {
            match result {
                WorkResult::RegisterOk { session_id } => {
                    self.session_id = Some(session_id);
                    self.status = Status::Info("Verification code sent to your phone.".into());
                    self.step = Step::Verification;
                }
                WorkResult::RegisterNeedsCaptcha { session_id } => {
                    self.session_id = Some(session_id);
                    self.captcha_was_required = true;
                    self.status =
                        Status::Info("A captcha is required to complete registration.".into());
                    self.step = Step::Captcha;
                }
                WorkResult::RegisterError(e) => {
                    self.status = Status::Error(format!("Registration failed: {e}"));
                }
                WorkResult::VerifyOk {
                    mut account,
                    warning,
                } => {
                    // Bind a Signal Desktop profile to this account phone number.
                    self.assign_desktop_profile(&mut account);
                    self.remember_account(&account);
                    self.signal_account = Some(account);
                    self.device_transfer_available = false;
                    self.status = match warning {
                        Some(w) => Status::Error(format!("Phone number verified, but: {w}")),
                        None => Status::Success("Phone number verified.".into()),
                    };
                    self.step = Step::Linking;
                }
                WorkResult::VerifyError(e) => {
                    self.status = Status::Error(format!("Verification failed: {e}"));
                }
                WorkResult::DeviceTransferAvailable => {
                    self.device_transfer_available = true;
                    self.status = Status::None;
                }
                WorkResult::LinkOk { device } => {
                    let name = device
                        .name
                        .as_deref()
                        .map(|n| format!(" \"{n}\""))
                        .unwrap_or_default();
                    self.status = Status::Success(format!(
                        "Device{name} linked successfully as device {}.",
                        device.id
                    ));
                    self.devices = None;
                    self.step = Step::Complete;
                }
                WorkResult::LinkError(e) => {
                    self.status = Status::Error(format!("Linking failed: {e}"));
                }
                WorkResult::RefreshLastSeenOk { account } => {
                    if let Err(e) = persistence::update_metadata(&account) {
                        eprintln!("Warning: could not save last-seen refresh time: {e}");
                    }
                    if let Some(slot) = self.accounts.iter_mut().find(|a| a.phone == account.phone)
                    {
                        *slot = account.clone();
                    }
                    self.status = Status::Success(format!(
                        "Signal now sees {} as active again.",
                        account.phone
                    ));
                }
                WorkResult::RefreshLastSeenError(e) => {
                    self.status = Status::Error(format!("Could not refresh account activity: {e}"));
                }
                WorkResult::DevicesOk { phone, devices } => {
                    if self.selected_phone.as_deref() == Some(phone.as_str()) {
                        self.devices = Some(devices);
                    }
                }
                WorkResult::DevicesError(e) => {
                    self.devices = Some(vec![]);
                    self.status = Status::Error(format!("Could not list linked devices: {e}"));
                }
                WorkResult::UnlinkOk { phone, device_id } => {
                    if self.selected_phone.as_deref() == Some(phone.as_str()) {
                        if let Some(devices) = self.devices.as_mut() {
                            devices.retain(|d| d.id != device_id);
                        }
                    }
                    self.status = Status::Success(format!("Device {device_id} unlinked."));
                }
                WorkResult::UnlinkError(e) => {
                    self.status = Status::Error(format!("Could not unlink the device: {e}"));
                }
                WorkResult::LockOk { account } => {
                    self.remember_account(&account);
                    self.status = Status::Success(if account.registration_lock {
                        "Registration lock enabled.".into()
                    } else {
                        "Registration lock disabled.".into()
                    });
                }
                WorkResult::LockError(e) => {
                    self.status =
                        Status::Error(format!("Could not change the registration lock: {e}"));
                }
                WorkResult::ExportOk { path, json } => {
                    self.export_passphrase.clear();
                    self.export_passphrase_confirm.clear();
                    self.status = Status::Success(match &path {
                        Some(p) => format!("Key file written to {}", p.display()),
                        None => "Key file ready. Copy it to the clipboard below.".into(),
                    });
                    self.export_result = Some((path, json));
                }
                WorkResult::ExportError(e) => {
                    self.status = Status::Error(format!("Could not export the keys: {e}"));
                }
            }
        }

        let step_num = self.step.number();
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::none()
                    .fill(egui::Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(28.0, 18.0))
                    .stroke(egui::Stroke::new(1.0, BORDER)),
            )
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Signal Setup Tool")
                        .size(22.0)
                        .color(SIGNAL_BLUE)
                        .strong(),
                );
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Register a Signal account without a smartphone")
                        .size(13.0)
                        .color(MUTED),
                );
                if step_num > 0 {
                    ui.add_space(14.0);
                    if let Some(target) =
                        draw_step_indicator(ui, step_num, self.captcha_was_required)
                    {
                        self.jump_back_to_step(target);
                    }
                }
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(PAGE_BG)
                    .inner_margin(egui::Margin::same(24.0)),
            )
            .show(ctx, |ui| {
                let status = self.status.clone();
                let loading = self.loading;

                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Frame::none()
                        .fill(CARD_BG)
                        .rounding(egui::Rounding::same(12.0))
                        .stroke(egui::Stroke::new(1.0, BORDER))
                        .inner_margin(egui::Margin::same(28.0))
                        .show(ui, |ui| {
                            // Stretch card to fill the available width.
                            ui.set_width(ui.available_width());
                            show_status(ui, &status);

                            if loading {
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.add_space(8.0);
                                    ui.label(RichText::new("Working…").color(MUTED).size(15.0));
                                });
                                return;
                            }

                            match self.step {
                                Step::Welcome => self.ui_welcome(ui),
                                Step::Account => self.ui_account(ui, ctx),
                                Step::PhoneInput => self.ui_phone(ui, ctx),
                                Step::Captcha => self.ui_captcha(ui, ctx),
                                Step::Verification => self.ui_verify(ui, ctx),
                                Step::Linking => self.ui_linking(ui, ctx),
                                Step::Complete => self.ui_complete(ui),
                            }
                        });
                });
            });
    }
}
