mod signal_demo;
mod signal_http;

use arboard::Clipboard;
use eframe::egui;
use egui::RichText;
use image::DynamicImage;
use rxing::{
    common::HybridBinarizer, BinaryBitmap, BufferedImageLuminanceSource, DecodeHintType,
    DecodeHintValue, MultiFormatReader, Reader,
};
use signal_http::SignalAccount;
use std::process::Command;
use std::sync::mpsc;

#[derive(Clone, Copy, PartialEq, Default)]
enum Mode {
    #[default]
    Production,
    Staging,
    Demo,
}

static MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();

fn mode() -> Mode {
    MODE.get().copied().unwrap_or_default()
}

// Dispatch helpers that route to signal_http or signal_demo based on the mode.
mod api {
    use super::*;
    use signal_http::{SignalError, VerificationRequest};

    pub fn request_verification_code(
        phone: &str,
        captcha: Option<&str>,
    ) -> Result<VerificationRequest, SignalError> {
        match mode() {
            Mode::Demo => signal_demo::request_verification_code(phone, captcha),
            _ => signal_http::request_verification_code(phone, captcha),
        }
    }

    pub fn submit_captcha(
        session_id: &str,
        token: &str,
    ) -> Result<VerificationRequest, SignalError> {
        match mode() {
            Mode::Demo => signal_demo::submit_captcha(session_id, token),
            _ => signal_http::submit_captcha(session_id, token),
        }
    }

    pub fn verify_and_register(
        phone: &str,
        session_id: &str,
        code: &str,
        skip_device_transfer: bool,
    ) -> Result<SignalAccount, SignalError> {
        match mode() {
            Mode::Demo => {
                signal_demo::verify_and_register(phone, session_id, code, skip_device_transfer)
            }
            _ => signal_http::verify_and_register(phone, session_id, code, skip_device_transfer),
        }
    }

    pub fn link_device(account: &SignalAccount, uri: &str) -> Result<(), SignalError> {
        match mode() {
            Mode::Demo => signal_demo::link_device(account, uri),
            _ => signal_http::link_device(account, uri),
        }
    }
}

// Color palette

const SIGNAL_BLUE: egui::Color32 = egui::Color32::from_rgb(59, 130, 246);
const SUCCESS_GREEN: egui::Color32 = egui::Color32::from_rgb(22, 163, 74);
const SUCCESS_BG: egui::Color32 = egui::Color32::from_rgb(240, 253, 244);
const SUCCESS_BORDER: egui::Color32 = egui::Color32::from_rgb(187, 247, 208);
const ERROR_RED: egui::Color32 = egui::Color32::from_rgb(220, 38, 38);
const ERROR_BG: egui::Color32 = egui::Color32::from_rgb(254, 242, 242);
const ERROR_BORDER: egui::Color32 = egui::Color32::from_rgb(254, 202, 202);
const INFO_TEXT: egui::Color32 = egui::Color32::from_rgb(29, 78, 216);
const INFO_BG: egui::Color32 = egui::Color32::from_rgb(239, 246, 255);
const INFO_BORDER: egui::Color32 = egui::Color32::from_rgb(191, 219, 254);
const MUTED: egui::Color32 = egui::Color32::from_rgb(107, 114, 128);
const HEADING: egui::Color32 = egui::Color32::from_rgb(17, 24, 39);
const PAGE_BG: egui::Color32 = egui::Color32::from_rgb(243, 244, 246);
const CARD_BG: egui::Color32 = egui::Color32::WHITE;
const BORDER: egui::Color32 = egui::Color32::from_rgb(229, 231, 235);
const INSET_BG: egui::Color32 = egui::Color32::from_rgb(249, 250, 251);

// Step state machine

#[derive(Default, PartialEq, Clone, Copy)]
enum Step {
    #[default]
    PhoneInput,
    Captcha,
    Verification,
    Linking,
    Complete,
}

impl Step {
    fn number(self) -> usize {
        match self {
            Step::PhoneInput => 1,
            Step::Captcha => 2,
            Step::Verification => 3,
            Step::Linking => 4,
            Step::Complete => 5,
        }
    }
}

// Background-work result types

enum WorkResult {
    RegisterOk { session_id: String },
    RegisterNeedsCaptcha { session_id: String },
    RegisterError(String),
    VerifyOk { account: SignalAccount },
    VerifyError(String),
    DeviceTransferAvailable,
    LinkOk,
    LinkError(String),
}

// Status banner

#[derive(Default, Clone)]
enum Status {
    #[default]
    None,
    Info(String),
    Success(String),
    Error(String),
}

// App state

#[derive(Default)]
struct SignalSetupApp {
    step: Step,
    phone: String,
    captcha_token: String,
    verification_code: String,
    device_uri: String,
    status: Status,
    loading: bool,
    /// Session ID returned by the verification session API (steps 1–3).
    session_id: Option<String>,
    /// Account key material after successful registration (steps 3–4).
    signal_account: Option<SignalAccount>,
    result_rx: Option<mpsc::Receiver<WorkResult>>,
    /// Set to true when registration returns 409 (existing account supports
    /// device transfer). The UI shows an explanation and a "Skip Transfer" button.
    device_transfer_available: bool,
}

impl SignalSetupApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        Self::default()
    }

    fn spawn<F>(&mut self, ctx: egui::Context, f: F)
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

    fn poll_result(&mut self) -> Option<WorkResult> {
        let result = self.result_rx.as_ref()?.try_recv().ok()?;
        self.result_rx = None;
        self.loading = false;
        Some(result)
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
                    self.status =
                        Status::Info("A captcha is required to complete registration.".into());
                    self.step = Step::Captcha;
                }
                WorkResult::RegisterError(e) => {
                    self.status = Status::Error(format!("Registration failed: {e}"));
                }
                WorkResult::VerifyOk { account } => {
                    self.signal_account = Some(account);
                    self.device_transfer_available = false;
                    self.status = Status::Success("Phone number verified.".into());
                    self.step = Step::Linking;
                }
                WorkResult::VerifyError(e) => {
                    self.status = Status::Error(format!("Verification failed: {e}"));
                }
                WorkResult::DeviceTransferAvailable => {
                    self.device_transfer_available = true;
                    self.status = Status::None;
                }
                WorkResult::LinkOk => {
                    self.status = Status::Success("Device linked successfully!".into());
                    self.step = Step::Complete;
                }
                WorkResult::LinkError(e) => {
                    self.status = Status::Error(format!("Linking failed: {e}"));
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
                ui.add_space(14.0);
                draw_step_indicator(ui, step_num);
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

// Per-step UI panels

impl SignalSetupApp {
    fn ui_phone(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        step_header(ui, "Phone number", "Step 1 of 4");

        ui.label(RichText::new("Enter your phone number with country code:").color(MUTED));
        ui.add_space(6.0);

        let resp = egui::TextEdit::singleline(&mut self.phone)
            .desired_width(f32::INFINITY)
            .hint_text("+1234567890")
            .font(egui::FontId::proportional(17.0))
            .show(ui)
            .response;

        ui.add_space(18.0);

        let ready = !self.phone.is_empty();
        let clicked = submit_row(ui, ready, "Register");

        if clicked || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && ready)
        {
            let phone = self.phone.clone();
            self.spawn(ctx.clone(), move || {
                match api::request_verification_code(&phone, None) {
                    Ok(signal_http::VerificationRequest::CodeSent { session_id }) => {
                        WorkResult::RegisterOk { session_id }
                    }
                    Ok(signal_http::VerificationRequest::CaptchaRequired { session_id }) => {
                        WorkResult::RegisterNeedsCaptcha { session_id }
                    }
                    Err(e) => WorkResult::RegisterError(e.to_string()),
                }
            });
        }
    }

    fn ui_captcha(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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
                match api::submit_captcha(&session_id, &token) {
                    Ok(signal_http::VerificationRequest::CodeSent { session_id }) => {
                        WorkResult::RegisterOk { session_id }
                    }
                    Ok(signal_http::VerificationRequest::CaptchaRequired { session_id }) => {
                        WorkResult::RegisterNeedsCaptcha { session_id }
                    }
                    Err(e) => WorkResult::RegisterError(e.to_string()),
                }
            });
        }
    }

    fn ui_verify(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        // When a device transfer is available, replace the whole form with the prompt.
        if self.device_transfer_available {
            step_header(ui, "Verify phone number", "Step 3 of 4");
            ui.add_space(16.0);

            egui::Frame::none()
                .fill(egui::Color32::from_rgb(255, 247, 237))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(253, 186, 116)))
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
                            match api::verify_and_register(
                                &phone,
                                &session_id,
                                &code,
                                true,
                            ) {
                                Ok(account) => WorkResult::VerifyOk { account },
                                Err(e) => WorkResult::VerifyError(e.to_string()),
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
                match api::verify_and_register(&phone, &session_id, &code, false) {
                    Ok(account) => WorkResult::VerifyOk { account },
                    Err(signal_http::SignalError::DeviceTransferAvailable) => {
                        WorkResult::DeviceTransferAvailable
                    }
                    Err(e) => WorkResult::VerifyError(e.to_string()),
                }
            });
        }
    }

    fn ui_linking(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        step_header(ui, "Link Signal Desktop", "Step 4 of 4");

        instruction_box(
            ui,
            &[
                "1. Open Signal Desktop and click \"Link to an existing device\"",
                "2. Take a screenshot of the QR code (Cmd+Shift+4 on Mac, Win+Shift+S on Windows)",
                "3. Click \"Paste QR Image\" below to automatically decode it",
                "   OR manually scan with a QR app and paste the tsdevice:// link",
            ],
        );

        ui.add_space(12.0);

        // Add "Paste QR Image" button
        if ui
            .add(
                egui::Button::new(
                    RichText::new("📋  Paste QR Image")
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
            match paste_and_decode_qr() {
                Ok(uri) => {
                    self.device_uri = uri;
                    self.status = Status::Success("QR code decoded successfully!".into());
                }
                Err(e) => {
                    self.status = Status::Error(format!("Failed to decode QR code: {}", e));
                }
            }
        }

        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "💡 Tip: Make sure the QR code is clearly visible and well-lit in your screenshot",
            )
            .color(MUTED)
            .size(12.0),
        );
        ui.add_space(12.0);

        ui.label(RichText::new("Device link:").color(HEADING).size(14.0));
        ui.add_space(4.0);

        egui::TextEdit::multiline(&mut self.device_uri)
            .desired_width(f32::INFINITY)
            .desired_rows(4)
            .hint_text("tsdevice://?uuid=...")
            .font(egui::FontId::monospace(13.0))
            .show(ui);

        ui.add_space(18.0);

        let ready = !self.device_uri.trim().is_empty() && self.signal_account.is_some();
        if submit_row(ui, ready, "Link device") {
            let account = self.signal_account.clone().unwrap();
            let uri = self.device_uri.trim().to_string();
            self.spawn(ctx.clone(), move || {
                match api::link_device(&account, &uri) {
                    Ok(()) => WorkResult::LinkOk,
                    Err(e) => WorkResult::LinkError(e.to_string()),
                }
            });
        }
    }

    fn ui_complete(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
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
                RichText::new("Your Signal Desktop is now linked and ready to use.")
                    .size(15.0)
                    .color(MUTED),
            );
            ui.add_space(28.0);
            if ui
                .add(
                    egui::Button::new(RichText::new("Start over").size(14.0).color(MUTED))
                        .fill(INSET_BG)
                        .stroke(egui::Stroke::new(1.0, BORDER))
                        .rounding(egui::Rounding::same(8.0)),
                )
                .clicked()
            {
                *self = SignalSetupApp::default();
            }
        });
        ui.add_space(16.0);
    }
}

// UI helpers

fn setup_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();

    // Backgrounds
    visuals.panel_fill = PAGE_BG;
    visuals.window_fill = CARD_BG;
    visuals.extreme_bg_color = egui::Color32::WHITE; // text input backgrounds

    // Consistent rounding everywhere
    let r = egui::Rounding::same(8.0);
    visuals.window_rounding = r;
    visuals.menu_rounding = r;
    visuals.widgets.noninteractive.rounding = r;
    visuals.widgets.inactive.rounding = r;
    visuals.widgets.hovered.rounding = r;
    visuals.widgets.active.rounding = r;
    visuals.widgets.open.rounding = r;

    // Widget fill / border colours
    visuals.widgets.noninteractive.bg_fill = egui::Color32::WHITE;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.bg_fill = egui::Color32::WHITE;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.bg_fill = INFO_BG;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, SIGNAL_BLUE);
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(219, 234, 254);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.5, SIGNAL_BLUE);

    // Selection (text highlight)
    visuals.selection.bg_fill = egui::Color32::from_rgb(219, 234, 254);
    visuals.selection.stroke = egui::Stroke::new(1.0, SIGNAL_BLUE);

    ctx.set_visuals(visuals);

    // Font sizes and spacing
    let mut style = (*ctx.style()).clone();
    use egui::FontFamily::Proportional;
    use egui::TextStyle::*;
    style.text_styles = [
        (Heading, egui::FontId::new(22.0, Proportional)),
        (Body, egui::FontId::new(15.0, Proportional)),
        (
            Monospace,
            egui::FontId::new(14.0, egui::FontFamily::Monospace),
        ),
        (Button, egui::FontId::new(15.0, Proportional)),
        (Small, egui::FontId::new(13.0, Proportional)),
    ]
    .into();
    style.spacing.button_padding = egui::vec2(20.0, 10.0);
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    ctx.set_style(style);
}

/// Visual step indicator drawn with egui's painter (circles + connecting lines).
fn draw_step_indicator(ui: &mut egui::Ui, current: usize) {
    let names = ["Phone", "Captcha", "Verify", "Link"];
    let n = names.len();
    let circle_radius = 13.0_f32;
    let height = circle_radius * 2.0 + 22.0;
    let available = ui.available_width();

    let (outer_rect, _) =
        ui.allocate_exact_size(egui::vec2(available, height), egui::Sense::hover());

    let step_width = available / n as f32;
    let painter = ui.painter();

    for i in 0..n {
        let step = i + 1;
        let is_done = step < current;
        let is_active = step == current;

        let cx = outer_rect.left() + step_width * (i as f32 + 0.5);
        let cy = outer_rect.top() + circle_radius;
        let center = egui::pos2(cx, cy);

        let circle_color = if is_done {
            SUCCESS_GREEN
        } else if is_active {
            SIGNAL_BLUE
        } else {
            egui::Color32::from_rgb(209, 213, 219)
        };
        let label_color = if is_done || is_active { HEADING } else { MUTED };

        painter.circle_filled(center, circle_radius, circle_color);

        let num_str = step.to_string();

        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            &num_str,
            egui::FontId::proportional(11.0),
            egui::Color32::WHITE,
        );

        painter.text(
            egui::pos2(cx, cy + circle_radius + 5.0),
            egui::Align2::CENTER_TOP,
            names[i],
            egui::FontId::proportional(12.0),
            label_color,
        );

        // Connector line between circles
        if i + 1 < n {
            let next_cx = outer_rect.left() + step_width * (i as f32 + 1.5);
            let line_color = if step < current {
                SUCCESS_GREEN
            } else {
                egui::Color32::from_rgb(209, 213, 219)
            };
            painter.line_segment(
                [
                    egui::pos2(cx + circle_radius + 4.0, cy),
                    egui::pos2(next_cx - circle_radius - 4.0, cy),
                ],
                egui::Stroke::new(2.0, line_color),
            );
        }
    }
}

/// Coloured status banner (error / success / info).
fn show_status(ui: &mut egui::Ui, status: &Status) {
    let (icon, text, text_color, bg, border) = match status {
        Status::None => return,
        Status::Error(m) => ("\u{26A0}", m.as_str(), ERROR_RED, ERROR_BG, ERROR_BORDER),
        Status::Success(m) => ("\u{2713}", m.as_str(), SUCCESS_GREEN, SUCCESS_BG, SUCCESS_BORDER),
        Status::Info(m) => ("\u{2139}", m.as_str(), INFO_TEXT, INFO_BG, INFO_BORDER),
    };

    egui::Frame::none()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, border))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(14.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Merge icon + text into one label so long messages wrap naturally.
            ui.label(
                RichText::new(format!("{icon}  {text}"))
                    .color(text_color)
                    .size(14.0),
            );
        });
    ui.add_space(12.0);
}

/// Section title + subtitle + separator.
fn step_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(19.0).color(HEADING).strong());
    ui.add_space(2.0);
    ui.label(RichText::new(subtitle).size(13.0).color(MUTED));
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(12.0);
}

/// Light inset box with bullet instructions.
fn instruction_box(ui: &mut egui::Ui, lines: &[&str]) {
    egui::Frame::none()
        .fill(INSET_BG)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            for line in lines {
                ui.label(RichText::new(*line).size(14.0).color(MUTED));
            }
        });
}

/// Right-aligned primary action button. Returns `true` if clicked.
fn submit_row(ui: &mut egui::Ui, enabled: bool, label: &str) -> bool {
    let mut clicked = false;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let btn = egui::Button::new(RichText::new(label).color(egui::Color32::WHITE).size(15.0))
            .fill(SIGNAL_BLUE)
            .rounding(egui::Rounding::same(8.0))
            .min_size(egui::vec2(140.0, 40.0));
        clicked = ui.add_enabled(enabled, btn).clicked();
    });
    clicked
}

/// Open a URL in the system default browser, cross-platform.
fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = Command::new("xdg-open").arg(url).spawn();

    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();

    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd").args(["/C", "start", "", url]).spawn();
}

// QR Code decoding helpers

/// Try to decode a QR code from a grayscale image using rxing.
fn try_decode_gray(gray: &image::GrayImage) -> Option<String> {
    let lum_source = BufferedImageLuminanceSource::new(DynamicImage::ImageLuma8(gray.clone()));
    let mut bitmap = BinaryBitmap::new(HybridBinarizer::new(lum_source));
    let mut hints = std::collections::HashMap::new();
    hints.insert(DecodeHintType::TRY_HARDER, DecodeHintValue::TryHarder(true));
    MultiFormatReader::default()
        .decode_with_hints(&mut bitmap, &hints)
        .ok()
        .map(|r| r.getText().to_string())
}

fn invert(gray: &image::GrayImage) -> image::GrayImage {
    let mut out = gray.clone();
    for p in out.pixels_mut() {
        p.0[0] = 255 - p.0[0];
    }
    out
}

fn apply_threshold(gray: &image::GrayImage, threshold: u8) -> image::GrayImage {
    let mut out = gray.clone();
    for p in out.pixels_mut() {
        p.0[0] = if p.0[0] > threshold { 255 } else { 0 };
    }
    out
}

fn adjust_brightness_contrast(
    gray: &image::GrayImage,
    brightness: i32,
    contrast: f32,
) -> image::GrayImage {
    let mut out = gray.clone();
    for p in out.pixels_mut() {
        let v = ((p.0[0] as i32 - 128) as f32 * contrast) as i32 + 128 + brightness;
        p.0[0] = v.clamp(0, 255) as u8;
    }
    out
}

fn upscale(gray: &image::GrayImage, scale: u32) -> image::GrayImage {
    image::imageops::resize(
        gray,
        gray.width() * scale,
        gray.height() * scale,
        image::imageops::FilterType::Nearest,
    )
}

/// Decode a QR code, trying a series of preprocessing variants for robustness.
fn decode_qr_from_image(img: &DynamicImage) -> Result<String, String> {
    let gray = img.to_luma8();

    let try_it = |g: image::GrayImage| -> Option<String> { try_decode_gray(&g) };

    if let Some(s) = try_it(gray.clone()) { return Ok(s); }
    if let Some(s) = try_it(invert(&gray)) { return Ok(s); }

    for t in [100u8, 128, 150, 180, 200] {
        if let Some(s) = try_it(apply_threshold(&gray, t)) { return Ok(s); }
    }
    for (b, c) in [(20, 1.5_f32), (-20, 1.5), (0, 2.0)] {
        if let Some(s) = try_it(adjust_brightness_contrast(&gray, b, c)) { return Ok(s); }
    }
    for scale in [2u32, 3, 4] {
        if let Some(s) = try_it(upscale(&gray, scale)) { return Ok(s); }
    }
    for (scale, t) in [(2u32, 128u8), (3, 128), (2, 150)] {
        if let Some(s) = try_it(apply_threshold(&upscale(&gray, scale), t)) { return Ok(s); }
    }
    for (scale, b, c) in [(2u32, 0, 2.0_f32), (3, 0, 2.0)] {
        if let Some(s) = try_it(adjust_brightness_contrast(&upscale(&gray, scale), b, c)) {
            return Ok(s);
        }
    }
    if gray.width() > 800 || gray.height() > 800 {
        let scale = 800.0 / gray.width().max(gray.height()) as f32;
        let down = image::imageops::resize(
            &gray,
            (gray.width() as f32 * scale) as u32,
            (gray.height() as f32 * scale) as u32,
            image::imageops::FilterType::Lanczos3,
        );
        if let Some(s) = try_it(down) { return Ok(s); }
    }
    let blurred = image::imageops::blur(&gray, 1.0);
    if let Some(s) = try_it(apply_threshold(&blurred, 128)) { return Ok(s); }

    Err("Could not decode QR code. The image may be damaged, too blurry, or partially obscured.".into())
}

/// Get image from clipboard and decode QR code.
fn paste_and_decode_qr() -> Result<String, String> {
    let mut clipboard =
        Clipboard::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;

    let img_data = clipboard
        .get_image()
        .map_err(|e| format!("No image in clipboard: {}. Try taking a screenshot with Cmd+Shift+4 and selecting the QR code area.", e))?;

    let width = img_data.width as u32;
    let height = img_data.height as u32;
    let bytes = img_data.bytes.to_vec();

    let dynamic_img = match bytes.len() / (width as usize * height as usize) {
        4 => image::RgbaImage::from_raw(width, height, bytes)
            .map(DynamicImage::ImageRgba8)
            .ok_or_else(|| format!("Failed to create RGBA image ({}x{})", width, height))?,
        3 => image::RgbImage::from_raw(width, height, bytes)
            .map(DynamicImage::ImageRgb8)
            .ok_or_else(|| format!("Failed to create RGB image ({}x{})", width, height))?,
        n => return Err(format!("Unexpected pixel format: {} bytes per pixel.", n)),
    };

    decode_qr_from_image(&dynamic_img)
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--demo") {
        MODE.set(Mode::Demo).ok();
    } else if args.iter().any(|a| a == "--staging") {
        MODE.set(Mode::Staging).ok();
        signal_http::enable_staging();
    }

    let title = match mode() {
        Mode::Demo => "Signal Setup Tool [DEMO]",
        Mode::Staging => "Signal Setup Tool [STAGING]",
        Mode::Production => "Signal Setup Tool",
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([580.0, 600.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        title,
        options,
        Box::new(|cc| Ok(Box::new(SignalSetupApp::new(cc)))),
    )
}


