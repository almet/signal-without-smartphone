//! Core data types shared across the crate: the in-memory `SignalAccount`,
//! its serializable split (`PersistedAccount` + `AccountSecrets`), the
//! `SignalError` type returned by all flows, and `VerificationRequest`.

use base64::prelude::*;
use libsignal_protocol::{self as sigprot, IdentityKeyPair};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use serde::{Deserialize, Serialize};

/// All cryptographic material for a registered Signal account.
///
/// Created by `verify_and_register` and required for `link_device`.
#[derive(Clone)]
pub struct SignalAccount {
    pub phone: String,
    /// Base64-encoded random password used for HTTP basic auth.
    pub password: String,
    /// ACI identity key pair (Account Identifier), a libsignal-protocol type.
    pub(crate) aci_identity: IdentityKeyPair,
    /// PNI identity key pair (Phone Number Identity), a libsignal-protocol type.
    pub(crate) pni_identity: IdentityKeyPair,
    /// ACI UUID returned by Signal after successful registration.
    pub aci: Option<String>,
    /// PNI uuid returned by Signal after successful registration.
    pub pni: Option<String>,
    /// 32-byte master key, generated once and included in every provisioning message.
    pub(crate) master_key: Vec<u8>,
    /// 32-byte random profile key.
    pub(crate) profile_key: Vec<u8>,
    /// 14-bit random registration ID, included in Signal Protocol message headers.
    pub(crate) registration_id: u32,
    /// Name of the Signal Desktop `--user-data-dir` profile bound to this
    /// account. `None` for accounts saved by older builds; assigned at
    /// registration time for new ones. See `crate::desktop`.
    pub desktop_profile: Option<String>,
}

/// Non-sensitive metadata for a saved account, serialized to disk.
///
/// Secret material (password, identity key pairs, master/profile keys) is
/// stored in the OS keyring as separate entries. See `AccountSecrets` and
/// `crate::persistence` for the keyring schema.
#[derive(Serialize, Deserialize)]
pub struct PersistedAccount {
    pub phone: String,
    pub aci: Option<String>,
    pub pni: Option<String>,
    pub registration_id: u32,
    #[serde(default)]
    pub desktop_profile: Option<String>,
}

/// Sensitive fields for a `SignalAccount`. Serialized to JSON and stored as
/// a single keyring entry per phone (one Keychain prompt per account rather
/// than one per field).
///
/// Binary fields are base64-encoded because keyring backends store strings.
#[derive(Serialize, Deserialize)]
pub struct AccountSecrets {
    pub password: String,
    pub aci_identity_b64: String,
    pub pni_identity_b64: String,
    pub master_key_b64: String,
    pub profile_key_b64: String,
}

impl SignalAccount {
    /// Split into the on-disk metadata and the keyring-bound secrets.
    pub fn to_persisted(&self) -> (PersistedAccount, AccountSecrets) {
        let public = PersistedAccount {
            phone: self.phone.clone(),
            aci: self.aci.clone(),
            pni: self.pni.clone(),
            registration_id: self.registration_id,
            desktop_profile: self.desktop_profile.clone(),
        };
        let secrets = AccountSecrets {
            password: self.password.clone(),
            aci_identity_b64: BASE64_STANDARD.encode(self.aci_identity.serialize()),
            pni_identity_b64: BASE64_STANDARD.encode(self.pni_identity.serialize()),
            master_key_b64: BASE64_STANDARD.encode(&self.master_key),
            profile_key_b64: BASE64_STANDARD.encode(&self.profile_key),
        };
        (public, secrets)
    }

    /// Inverse of `to_persisted`. Fails if any base64 field is malformed or
    /// the identity key bytes aren't a valid `IdentityKeyPair`.
    pub fn try_from_persisted(
        p: PersistedAccount,
        s: AccountSecrets,
    ) -> Result<Self, SignalError> {
        let decode = |s: &str, what: &str| {
            BASE64_STANDARD
                .decode(s)
                .map_err(|e| SignalError::Other(format!("decode {what}: {e}")))
        };
        let aci_bytes = decode(&s.aci_identity_b64, "aci_identity")?;
        let pni_bytes = decode(&s.pni_identity_b64, "pni_identity")?;
        let aci_identity = IdentityKeyPair::try_from(aci_bytes.as_slice())
            .map_err(|e| SignalError::Other(format!("parse aci_identity: {e}")))?;
        let pni_identity = IdentityKeyPair::try_from(pni_bytes.as_slice())
            .map_err(|e| SignalError::Other(format!("parse pni_identity: {e}")))?;
        Ok(Self {
            phone: p.phone,
            password: s.password,
            aci_identity,
            pni_identity,
            aci: p.aci,
            pni: p.pni,
            master_key: decode(&s.master_key_b64, "master_key")?,
            profile_key: decode(&s.profile_key_b64, "profile_key")?,
            registration_id: p.registration_id,
            desktop_profile: p.desktop_profile,
        })
    }

    /// Create a fake account for demo/testing purposes.
    pub fn dummy(phone: &str) -> Self {
        let mut rng = StdRng::from_os_rng();
        let aci_identity = IdentityKeyPair::generate(&mut rng);
        let pni_identity = IdentityKeyPair::generate(&mut rng);
        let mut master_key = vec![0u8; 32];
        rng.fill_bytes(&mut master_key);
        let mut profile_key = vec![0u8; 32];
        rng.fill_bytes(&mut profile_key);
        Self {
            phone: phone.to_string(),
            password: "demo-password".into(),
            aci_identity,
            pni_identity,
            aci: Some("demo-aci-uuid".into()),
            pni: Some("demo-pni-uuid".into()),
            master_key,
            profile_key,
            registration_id: 12345,
            desktop_profile: None,
        }
    }
}

/// Error returned by all Signal API calls.
#[derive(Debug, thiserror::Error)]
pub enum SignalError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Signal API error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("Captcha required")]
    CaptchaRequired,
    /// The existing account on this number supports device-to-device data
    /// transfer. Signal requires the caller to explicitly opt out before
    /// allowing a fresh registration. Retry with `skip_device_transfer = true`.
    #[error("Device transfer available")]
    DeviceTransferAvailable,
    #[error("Invalid URI: {0}")]
    InvalidUri(String),
    #[error("Signal Protocol error: {0}")]
    Protocol(#[from] sigprot::SignalProtocolError),
    #[error("{0}")]
    Other(String),
}

/// Result of `request_verification_code`.
pub enum VerificationRequest {
    /// Code sent; caller must supply the session id to `verify_and_register`.
    CodeSent { session_id: String },
    /// Signal requires captcha before sending the code.
    CaptchaRequired { session_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persisted_account_roundtrip() {
        let original = SignalAccount::dummy("+15555550123");
        let (public, secrets) = original.to_persisted();
        // Only the public half is serialized to disk; secrets travel out of
        // band via the OS keyring (`persistence` module).
        let json = serde_json::to_string(&public).unwrap();
        let parsed: PersistedAccount = serde_json::from_str(&json).unwrap();
        let restored = SignalAccount::try_from_persisted(parsed, secrets).unwrap();

        assert_eq!(restored.phone, original.phone);
        assert_eq!(restored.password, original.password);
        assert_eq!(restored.aci, original.aci);
        assert_eq!(restored.pni, original.pni);
        assert_eq!(restored.registration_id, original.registration_id);
        assert_eq!(restored.master_key, original.master_key);
        assert_eq!(restored.profile_key, original.profile_key);
        assert_eq!(
            restored.aci_identity.serialize(),
            original.aci_identity.serialize()
        );
        assert_eq!(
            restored.pni_identity.serialize(),
            original.pni_identity.serialize()
        );
    }
}
