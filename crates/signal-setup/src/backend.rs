//! Run mode (production, staging, demo) and the dispatch layer that routes
//! each registration call to either the core crate or the demo backend.

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

// Dispatch helpers that route to the core crate or the demo backend based on
// the mode.

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
) -> Result<SignalAccount, SignalError> {
    match mode() {
        Mode::Demo => demo::verify_and_register(phone, session_id, code, skip_device_transfer),
        _ => core::verify_and_register(phone, session_id, code, skip_device_transfer),
    }
}

pub(crate) fn link_device(account: &SignalAccount, uri: &str) -> Result<(), SignalError> {
    match mode() {
        Mode::Demo => demo::link_device(account, uri),
        _ => core::link_device(account, uri),
    }
}
