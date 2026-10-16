//! Because this tools is kinda hard to test, I'm using a "backend" mechanism to
//! enable different run modes (production, staging, demo). This makes it easier to
//! test UI changes, for instance. The different modes can be passed as flags when
//! running the binary (--staging or --demo).
//!
//! I'm not particularly happy with how this is designed as of now, but at least it's
//! working :-)

use signal_setup_core as core;
use signal_setup_core::demo;
use signal_setup_core::{LinkedDevice, RegistrationOptions, SignalAccount};
use signal_setup_core::{SignalError, VerificationRequest};

#[derive(Clone, Copy, PartialEq, Default)]
pub(crate) enum Mode {
    #[default]
    Production,
    Staging,
    Demo,
}

pub(crate) static MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();

pub(crate) fn mode() -> Mode {
    MODE.get().copied().unwrap_or_default()
}

impl Mode {
    /// Label recorded in exported key files, so a staging or demo account
    /// is never imported into a production setup by mistake.
    pub(crate) fn environment_label(self) -> &'static str {
        match self {
            Mode::Production => "production",
            Mode::Staging => "staging",
            Mode::Demo => "demo",
        }
    }
}

pub(crate) fn request_verification_code(
    phone: &str,
    captcha: Option<&str>,
) -> Result<VerificationRequest, SignalError> {
    match mode() {
        Mode::Demo => demo::request_verification_code(phone, captcha),
        _ => core::request_verification_code(phone, captcha),
    }
}

pub(crate) fn submit_captcha(
    session_id: &str,
    token: &str,
) -> Result<VerificationRequest, SignalError> {
    match mode() {
        Mode::Demo => demo::submit_captcha(session_id, token),
        _ => core::submit_captcha(session_id, token),
    }
}

pub(crate) fn verify_and_register(
    phone: &str,
    session_id: &str,
    code: &str,
    options: &RegistrationOptions,
) -> Result<SignalAccount, SignalError> {
    match mode() {
        Mode::Demo => demo::verify_and_register(phone, session_id, code, options),
        _ => core::verify_and_register(phone, session_id, code, options),
    }
}

pub(crate) fn link_device(account: &SignalAccount, uri: &str) -> Result<LinkedDevice, SignalError> {
    match mode() {
        Mode::Demo => demo::link_device(account, uri),
        _ => core::link_device(account, uri),
    }
}

pub(crate) fn set_registration_lock(
    account: &mut SignalAccount,
    enabled: bool,
) -> Result<(), SignalError> {
    match mode() {
        Mode::Demo => demo::set_registration_lock(account, enabled),
        _ => core::set_registration_lock(account, enabled),
    }
}

pub(crate) fn list_devices(account: &SignalAccount) -> Result<Vec<LinkedDevice>, SignalError> {
    match mode() {
        Mode::Demo => demo::list_devices(account),
        _ => core::list_devices(account),
    }
}

pub(crate) fn unlink_device(account: &SignalAccount, device_id: u32) -> Result<(), SignalError> {
    match mode() {
        Mode::Demo => demo::unlink_device(account, device_id),
        _ => core::unlink_device(account, device_id),
    }
}

pub(crate) fn refresh_last_seen(account: &mut SignalAccount) -> Result<(), SignalError> {
    match mode() {
        Mode::Demo => demo::refresh_last_seen(account),
        _ => core::refresh_last_seen(account),
    }
}
