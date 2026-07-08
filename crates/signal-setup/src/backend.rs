//! Because this tools is kinda hard to test, I'm using a "backend" mechanism to
//! enable different run modes (production, staging, demo). This makes it easier to
//! test UI changes, for instance. The different modes can be passed as flags when
//! running the binary (--staging or --demo).
//!
//! I'm not particularly happy with how this is designed as of now, but at least it's
//! working :-)

use signal_setup_core as core;
use signal_setup_core::demo;
use signal_setup_core::SignalAccount;
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
    skip_device_transfer: bool,
    discoverable_by_phone_number: bool,
) -> Result<SignalAccount, SignalError> {
    match mode() {
        Mode::Demo => demo::verify_and_register(
            phone,
            session_id,
            code,
            skip_device_transfer,
            discoverable_by_phone_number,
        ),
        _ => core::verify_and_register(
            phone,
            session_id,
            code,
            skip_device_transfer,
            discoverable_by_phone_number,
        ),
    }
}

pub(crate) fn link_device(account: &SignalAccount, uri: &str) -> Result<(), SignalError> {
    match mode() {
        Mode::Demo => demo::link_device(account, uri),
        _ => core::link_device(account, uri),
    }
}

pub(crate) fn refresh_last_seen(account: &mut SignalAccount) -> Result<(), SignalError> {
    match mode() {
        Mode::Demo => demo::refresh_last_seen(account),
        _ => core::refresh_last_seen(account),
    }
}
