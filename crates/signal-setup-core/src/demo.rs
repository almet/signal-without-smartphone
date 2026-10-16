//! Drop-in replacement for the `http` flows that simulates the full
//! registration and linking flow with fake delays. This is what runs when the cli
//! is passed the `--demo` flag.
//!
//! Codes:
//! - `000000` forces a device transfer
//! - `111111` forces a registration lock.
//!
//! Captcha tokens:
//! - `429` simulates the SMS throttle
//! - `403` simulates a rejected captcha.

use crate::types::{
    now_unix, LinkedDevice, RegistrationOptions, SignalAccount, SignalError, VerificationRequest,
};
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
    captcha_token: &str,
) -> Result<VerificationRequest, SignalError> {
    fake_delay();
    if captcha_token.contains("403") {
        return Err(SignalError::CaptchaRejected(
            "the captcha link was already used or has expired. Open the captcha page \
             again, solve it, and paste the new link"
                .into(),
        ));
    }
    if captcha_token.contains("429") {
        return Ok(VerificationRequest::CodeAlreadySent {
            session_id: "demo-session-after-captcha".into(),
            next_sms_secs: 42,
        });
    }
    Ok(VerificationRequest::CodeSent {
        session_id: "demo-session-after-captcha".into(),
    })
}

pub fn verify_and_register(
    phone: &str,
    _session_id: &str,
    code: &str,
    options: &RegistrationOptions,
) -> Result<SignalAccount, SignalError> {
    fake_delay();

    if code == "000000" && !options.skip_device_transfer {
        return Err(SignalError::DeviceTransferAvailable);
    }

    if code == "111111" && options.unlock_master_key.is_none() {
        return Err(SignalError::RegistrationLocked {
            time_remaining_ms: 5 * 86_400 * 1000,
        });
    }

    if code != "000000" && !code.chars().all(|c| c.is_ascii_digit()) {
        return Err(SignalError::Other("Invalid verification code".into()));
    }

    Ok(SignalAccount::dummy(phone))
}

pub fn refresh_last_seen(account: &mut SignalAccount) -> Result<(), SignalError> {
    fake_delay();
    account.last_seen_refreshed_at = Some(crate::types::now_unix());
    Ok(())
}

pub fn set_registration_lock(
    account: &mut SignalAccount,
    enabled: bool,
) -> Result<(), SignalError> {
    fake_delay();
    account.registration_lock = enabled;
    Ok(())
}

pub fn delete_account(_account: &SignalAccount) -> Result<(), SignalError> {
    fake_delay();
    Ok(())
}

pub fn list_devices(_account: &SignalAccount) -> Result<Vec<LinkedDevice>, SignalError> {
    fake_delay();
    let now = now_unix();
    Ok(vec![
        LinkedDevice {
            id: 1,
            name: None,
            created: now - 30 * 86_400,
            last_seen: now,
        },
        LinkedDevice {
            id: 2,
            name: Some("Office laptop".into()),
            created: now - 29 * 86_400,
            last_seen: now - 86_400,
        },
        LinkedDevice {
            id: 3,
            name: Some("Kitchen computer".into()),
            created: now - 3 * 86_400,
            last_seen: now - 20 * 86_400,
        },
    ])
}

pub fn unlink_device(_account: &SignalAccount, device_id: u32) -> Result<(), SignalError> {
    fake_delay();
    if device_id == LinkedDevice::PRIMARY_ID {
        return Err(SignalError::Other(
            "The primary device cannot be unlinked".into(),
        ));
    }
    Ok(())
}

pub fn link_device(_account: &SignalAccount, uri: &str) -> Result<LinkedDevice, SignalError> {
    fake_delay();

    if !["tsdevice://", "sgnl://"]
        .iter()
        .any(|scheme| uri.contains(scheme))
    {
        return Err(SignalError::InvalidUri(
            "Expected a tsdevice:// or sgnl:// URI".into(),
        ));
    }

    Ok(LinkedDevice {
        id: 4,
        name: Some("Demo desktop".into()),
        created: now_unix(),
        last_seen: now_unix(),
    })
}
