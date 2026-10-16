//! Encrypted export and import of an account key material.
//!
//! This holds:
//!
//! - the HTTP password;
//! - the master key (which also unlocks the registration lock);
//! - the profile key.
//!
//! This is encrypted using a passphrase, stretched with Argon2id
//! (parameters stored in the file) into a 32-byte key.
//!
//! The payload is serialized to JSON and encrypted with AES-256-GCM
//! with the format, version and environment bound as associated data.
//!
//! Nothing about the account is readable without the passphrase.

use crate::types::{now_unix, AccountSecrets, PersistedAccount, SignalAccount, SignalError};
use aes_gcm::aead::{Aead, Payload as AeadPayload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::prelude::*;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "signal-setup-account";
pub const VERSION: u32 = 1;

/// Argon2id parameters used for new exports.
const ARGON2_M_COST_KIB: u32 = 64 * 1024;
const ARGON2_T_COST: u32 = 3;
const ARGON2_P_COST: u32 = 1;
/// Upper bounds accepted when importing, so a malicious file cannot ask for
/// gigabytes of memory or hours of hashing.
const ARGON2_MAX_M_COST_KIB: u32 = 1024 * 1024;
const ARGON2_MAX_T_COST: u32 = 32;
const ARGON2_MAX_P_COST: u32 = 8;

/// On-disk envelope. Everything in here is public except `ciphertext`.
#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    version: u32,
    environment: String,
    kdf: Kdf,
    nonce_b64: String,
    ciphertext_b64: String,
}

#[derive(Serialize, Deserialize)]
struct Kdf {
    name: String,
    salt_b64: String,
    m_cost_kib: u32,
    t_cost: u32,
    p_cost: u32,
}

#[derive(Serialize, Deserialize)]
struct Payload {
    account: PersistedAccount,
    secrets: AccountSecrets,
    exported_at: u64,
}

pub struct ImportedAccount {
    /// The account, with no Signal Desktop profile assigned yet (profiles are
    /// bound to the machine the account is imported on).
    pub account: SignalAccount,
    pub environment: String,
    pub exported_at: u64,
}

pub fn export_account(
    account: &SignalAccount,
    passphrase: &str,
    environment: &str,
) -> Result<String, SignalError> {
    let mut rng = StdRng::from_os_rng();
    let mut salt = [0u8; 16];
    rng.fill_bytes(&mut salt);
    let mut nonce = [0u8; 12];
    rng.fill_bytes(&mut nonce);

    let kdf = Kdf {
        name: "argon2id".into(),
        salt_b64: BASE64_STANDARD.encode(salt),
        m_cost_kib: ARGON2_M_COST_KIB,
        t_cost: ARGON2_T_COST,
        p_cost: ARGON2_P_COST,
    };
    let key = derive_key(passphrase, &salt, &kdf)?;

    let (persisted, secrets) = account.to_persisted();
    let payload = Payload {
        account: PersistedAccount {
            // Machine-specific: the importing side picks its own profile.
            desktop_profile: None,
            ..persisted
        },
        secrets,
        exported_at: now_unix(),
    };
    let plaintext = serde_json::to_vec(&payload)
        .map_err(|e| SignalError::Other(format!("serialize key file: {e}")))?;

    let aad = associated_data(FORMAT, VERSION, environment);
    let ciphertext = Aes256Gcm::new((&key).into())
        .encrypt(
            Nonce::from_slice(&nonce),
            AeadPayload {
                msg: &plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| SignalError::Other("key file encryption failed".into()))?;

    let envelope = Envelope {
        format: FORMAT.into(),
        version: VERSION,
        environment: environment.into(),
        kdf,
        nonce_b64: BASE64_STANDARD.encode(nonce),
        ciphertext_b64: BASE64_STANDARD.encode(ciphertext),
    };
    serde_json::to_string_pretty(&envelope)
        .map_err(|e| SignalError::Other(format!("serialize key file: {e}")))
}

pub fn import_account(json: &str, passphrase: &str) -> Result<ImportedAccount, SignalError> {
    let envelope: Envelope = serde_json::from_str(json.trim()).map_err(|_| {
        SignalError::Other("This does not look like a key file exported by Signal Setup".into())
    })?;
    if envelope.format != FORMAT {
        return Err(SignalError::Other(format!(
            "Unknown key file format '{}'",
            envelope.format
        )));
    }
    if envelope.version != VERSION {
        return Err(SignalError::Other(format!(
            "Key file version {} is not supported by this build (expected {VERSION})",
            envelope.version
        )));
    }
    if envelope.kdf.name != "argon2id" {
        return Err(SignalError::Other(format!(
            "Unsupported key derivation '{}'",
            envelope.kdf.name
        )));
    }
    if envelope.kdf.m_cost_kib > ARGON2_MAX_M_COST_KIB
        || envelope.kdf.t_cost > ARGON2_MAX_T_COST
        || envelope.kdf.p_cost > ARGON2_MAX_P_COST
    {
        return Err(SignalError::Other(
            "Key file asks for unreasonable key derivation parameters".into(),
        ));
    }

    let decode = |field: &str, what: &str| {
        BASE64_STANDARD
            .decode(field)
            .map_err(|e| SignalError::Other(format!("key file {what}: {e}")))
    };
    let salt = decode(&envelope.kdf.salt_b64, "salt")?;
    let nonce = decode(&envelope.nonce_b64, "nonce")?;
    let ciphertext = decode(&envelope.ciphertext_b64, "ciphertext")?;
    if nonce.len() != 12 {
        return Err(SignalError::Other("key file nonce is malformed".into()));
    }

    let key = derive_key(passphrase, &salt, &envelope.kdf)?;
    let aad = associated_data(&envelope.format, envelope.version, &envelope.environment);
    let plaintext = Aes256Gcm::new((&key).into())
        .decrypt(
            Nonce::from_slice(&nonce),
            AeadPayload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| SignalError::Other("Wrong passphrase, or the key file is corrupted".into()))?;

    let payload: Payload = serde_json::from_slice(&plaintext)
        .map_err(|e| SignalError::Other(format!("parse key file contents: {e}")))?;
    let account = SignalAccount::try_from_persisted(payload.account, payload.secrets)?;
    Ok(ImportedAccount {
        account,
        environment: envelope.environment,
        exported_at: payload.exported_at,
    })
}

pub fn suggested_file_name(account: &SignalAccount) -> String {
    let phone: String = account
        .phone
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '+')
        .collect();
    format!("signal-account-{phone}.json")
}

fn associated_data(format: &str, version: u32, environment: &str) -> Vec<u8> {
    format!("{format}|{version}|{environment}").into_bytes()
}

fn derive_key(passphrase: &str, salt: &[u8], kdf: &Kdf) -> Result<[u8; 32], SignalError> {
    use argon2::{Algorithm, Argon2, Params, Version};

    let params = Params::new(kdf.m_cost_kib, kdf.t_cost, kdf.p_cost, Some(32))
        .map_err(|e| SignalError::Other(format!("key derivation parameters: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| SignalError::Other(format!("key derivation failed: {e}")))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_roundtrip() {
        let mut original = SignalAccount::dummy("+15555550123");
        original.desktop_profile = Some("Signal-15555550123".into());
        original.registration_lock = true;

        let json = export_account(&original, "correct horse", "production").unwrap();
        let imported = import_account(&json, "correct horse").unwrap();

        assert_eq!(imported.environment, "production");
        let restored = imported.account;
        assert_eq!(restored.phone, original.phone);
        assert_eq!(restored.password, original.password);
        assert_eq!(restored.aci, original.aci);
        assert_eq!(restored.pni, original.pni);
        assert_eq!(restored.registration_id, original.registration_id);
        assert_eq!(restored.master_key, original.master_key);
        assert_eq!(restored.profile_key, original.profile_key);
        assert!(restored.registration_lock);
        assert_eq!(
            restored.aci_identity.serialize(),
            original.aci_identity.serialize()
        );
        assert_eq!(
            restored.pni_identity.serialize(),
            original.pni_identity.serialize()
        );
        assert_eq!(restored.desktop_profile, None);
    }

    #[test]
    fn wrong_passphrase_is_rejected() {
        let account = SignalAccount::dummy("+15555550123");
        let json = export_account(&account, "right", "production").unwrap();
        let err = import_account(&json, "wrong").err().unwrap().to_string();
        assert!(err.contains("Wrong passphrase"), "{err}");
    }

    #[test]
    fn export_leaks_nothing_in_the_clear() {
        let account = SignalAccount::dummy("+15555550123");
        let json = export_account(&account, "pw", "production").unwrap();
        for needle in [
            "+15555550123",
            "5555550123",
            &account.password,
            "demo-aci-uuid",
            "master_key",
            "aci_identity",
        ] {
            assert!(!json.contains(needle), "key file leaks `{needle}`:\n{json}");
        }
    }

    #[test]
    fn tampered_environment_is_rejected() {
        let account = SignalAccount::dummy("+15555550123");
        let json = export_account(&account, "pw", "production").unwrap();
        let tampered = json.replace("\"production\"", "\"staging\"");
        assert!(import_account(&tampered, "pw").is_err());
    }

    #[test]
    fn garbage_is_rejected_with_a_clear_message() {
        let err = import_account("hello", "pw").err().unwrap().to_string();
        assert!(err.contains("does not look like a key file"), "{err}");
    }

    #[test]
    fn unreasonable_kdf_parameters_are_rejected() {
        let account = SignalAccount::dummy("+15555550123");
        let json = export_account(&account, "pw", "production").unwrap();
        let huge = json.replace(
            &format!("\"m_cost_kib\": {ARGON2_M_COST_KIB}"),
            "\"m_cost_kib\": 4194304",
        );
        assert_ne!(huge, json, "fixture did not change");
        let err = import_account(&huge, "pw").err().unwrap().to_string();
        assert!(err.contains("unreasonable"), "{err}");
    }

    #[test]
    fn suggested_file_name_is_filesystem_safe() {
        let account = SignalAccount::dummy("+1 (555) 555-0123");
        assert_eq!(
            suggested_file_name(&account),
            "signal-account-+15555550123.json"
        );
    }
}
