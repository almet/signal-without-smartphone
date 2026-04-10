// Drop-in replacement for signal_http that simulates the full registration and
// linking flow with fake delays. Used with `--demo` to test the UI without
// hitting Signal's servers or needing a real phone number.

use crate::signal_http::{SignalAccount, SignalError, VerificationRequest};
use std::thread;
use std::time::Duration;

fn fake_delay() {
    thread::sleep(Duration::from_millis(800));
}

pub fn request_verification_code(
    phone: &str,
    _captcha: Option<&str>,
) -> Result<VerificationRequest, SignalError> {
    fake_delay();

    // Trigger the captcha flow if the phone number starts with +0
    if phone.starts_with("+0") {
        return Ok(VerificationRequest::CaptchaRequired {
            session_id: "demo-session-captcha".into(),
        });
    }

    Ok(VerificationRequest::CodeSent {
        session_id: "demo-session-123".into(),
    })
}

pub fn submit_captcha(
    _session_id: &str,
    _captcha_token: &str,
) -> Result<VerificationRequest, SignalError> {
    fake_delay();
    Ok(VerificationRequest::CodeSent {
        session_id: "demo-session-after-captcha".into(),
    })
}

pub fn verify_and_register(
    phone: &str,
    _session_id: &str,
    code: &str,
    skip_device_transfer: bool,
) -> Result<SignalAccount, SignalError> {
    fake_delay();

    if code == "000000" && !skip_device_transfer {
        return Err(SignalError::DeviceTransferAvailable);
    }

    if code != "000000" && !code.chars().all(|c| c.is_ascii_digit()) {
        return Err(SignalError::Other("Invalid verification code".into()));
    }

    Ok(SignalAccount::dummy(phone))
}

pub fn link_device(
    _account: &SignalAccount,
    uri: &str,
) -> Result<(), SignalError> {
    fake_delay();

    if !uri.contains("tsdevice://") && !uri.contains("sgnl://") {
        return Err(SignalError::InvalidUri(
            "Expected a tsdevice:// or sgnl:// URI".into(),
        ));
    }

    Ok(())
}
