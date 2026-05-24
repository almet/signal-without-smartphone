//! On-disk + keyring persistence of registered Signal accounts.
//!
//! Non-sensitive metadata (phone, ACI/PNI uuids, registration id) is stored
//! as a JSON array in the platform's config directory:
//!   macOS  : ~/Library/Application Support/signal-setup/accounts.json
//!   Linux  : $XDG_CONFIG_HOME/signal-setup/accounts.json
//!   Windows: %APPDATA%\signal-setup\accounts.json
//!
//! All cryptographic material (password, identity key pairs, master/profile
//! keys) lives in the OS-native keyring under service name "signal-setup",
//! with one entry per secret field per phone number. Usernames are
//! `{phone}:{field}` where field is one of:
//!   - password
//!   - aci_identity
//!   - pni_identity
//!   - master_key
//!   - profile_key
//!
//! Set `SIGNAL_SETUP_CONFIG_DIR` to override the file location (used by
//! tests). Tests also force the keyring backend to the in-memory sample
//! store via `init_keyring`.
//!
//! Dev migration: if a legacy `accounts.json` containing secret fields is
//! found on first run, secrets are moved into the keyring and the file is
//! rewritten with only public fields. Legacy `account.json` (single-account)
//! is also folded in.

use crate::signal_http::{AccountSecrets, PersistedAccount, SignalAccount, SignalError};
use keyring_core::Entry;
use std::path::PathBuf;
use std::sync::Once;

const ACCOUNTS_FILE: &str = "accounts.json";
const LEGACY_FILE: &str = "account.json";
const KEYRING_SERVICE: &str = "signal-setup";

const SECRET_FIELDS: &[&str] = &[
    "password",
    "aci_identity",
    "pni_identity",
    "master_key",
    "profile_key",
];

fn config_dir() -> Option<PathBuf> {
    if let Ok(override_dir) = std::env::var("SIGNAL_SETUP_CONFIG_DIR") {
        return Some(PathBuf::from(override_dir));
    }
    dirs::config_dir().map(|d| d.join("signal-setup"))
}

fn accounts_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(ACCOUNTS_FILE))
}

fn legacy_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join(LEGACY_FILE))
}

/// Register the keyring backend exactly once per process. Real builds use
/// the OS-native store; tests opt into the in-memory sample store by
/// setting `SIGNAL_SETUP_TEST_KEYRING=1` before any persistence call.
fn init_keyring() -> Result<(), SignalError> {
    static INIT: Once = Once::new();
    let mut result: Result<(), SignalError> = Ok(());
    INIT.call_once(|| {
        result = register_default_store();
    });
    result
}

fn register_default_store() -> Result<(), SignalError> {
    if std::env::var("SIGNAL_SETUP_TEST_KEYRING").is_ok() {
        let store = keyring_core::mock::Store::new()
            .map_err(|e| SignalError::Other(format!("mock keyring init: {e}")))?;
        keyring_core::set_default_store(store);
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let store = apple_native_keyring_store::keychain::Store::new_with_configuration(
            &std::collections::HashMap::new(),
        )
        .map_err(|e| SignalError::Other(format!("macOS keychain init: {e}")))?;
        keyring_core::set_default_store(store);
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        let store = windows_native_keyring_store::Store::new_with_configuration(
            &std::collections::HashMap::new(),
        )
        .map_err(|e| SignalError::Other(format!("Windows credential store init: {e}")))?;
        keyring_core::set_default_store(store);
        Ok(())
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        let store = dbus_secret_service_keyring_store::Store::new_with_configuration(
            &std::collections::HashMap::new(),
        )
        .map_err(|e| SignalError::Other(format!("Secret Service init: {e}")))?;
        keyring_core::set_default_store(store);
        Ok(())
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux",
        target_os = "freebsd",
    )))]
    {
        Err(SignalError::Other(
            "No native keyring backend available for this platform".into(),
        ))
    }
}

fn entry_for(phone: &str, field: &str) -> Result<Entry, SignalError> {
    init_keyring()?;
    Entry::new(KEYRING_SERVICE, &format!("{phone}:{field}"))
        .map_err(|e| SignalError::Other(format!("keyring entry {phone}:{field}: {e}")))
}

fn read_secrets(phone: &str) -> Result<AccountSecrets, SignalError> {
    let get = |field: &str| -> Result<String, SignalError> {
        let entry = entry_for(phone, field)?;
        entry
            .get_password()
            .map_err(|e| SignalError::Other(format!("keyring read {phone}:{field}: {e}")))
    };
    Ok(AccountSecrets {
        password: get("password")?,
        aci_identity_b64: get("aci_identity")?,
        pni_identity_b64: get("pni_identity")?,
        master_key_b64: get("master_key")?,
        profile_key_b64: get("profile_key")?,
    })
}

fn write_secrets(phone: &str, secrets: &AccountSecrets) -> Result<(), SignalError> {
    let set = |field: &str, value: &str| -> Result<(), SignalError> {
        let entry = entry_for(phone, field)?;
        entry
            .set_password(value)
            .map_err(|e| SignalError::Other(format!("keyring write {phone}:{field}: {e}")))
    };
    set("password", &secrets.password)?;
    set("aci_identity", &secrets.aci_identity_b64)?;
    set("pni_identity", &secrets.pni_identity_b64)?;
    set("master_key", &secrets.master_key_b64)?;
    set("profile_key", &secrets.profile_key_b64)?;
    Ok(())
}

fn delete_secrets(phone: &str) -> Result<(), SignalError> {
    for field in SECRET_FIELDS {
        let entry = entry_for(phone, field)?;
        match entry.delete_credential() {
            Ok(()) => {}
            // Missing entries are fine — the caller may be cleaning up
            // partial state, or this phone was never saved at all.
            Err(keyring_core::Error::NoEntry) => {}
            Err(e) => {
                return Err(SignalError::Other(format!(
                    "keyring delete {phone}:{field}: {e}"
                )));
            }
        }
    }
    Ok(())
}

/// Read the array of public account metadata from disk. Returns an empty Vec
/// if no file exists. Performs the one-time dev migration if needed.
fn read_all() -> Result<Vec<PersistedAccount>, SignalError> {
    migrate_dev_state_if_present()?;

    let Some(path) = accounts_path() else {
        return Ok(vec![]);
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(SignalError::Other(format!("read {}: {e}", path.display()))),
    };
    serde_json::from_slice(&bytes)
        .map_err(|e| SignalError::Other(format!("parse {}: {e}", path.display())))
}

/// Write the full array of public account metadata back to disk, replacing
/// any previous contents. Creates the config directory.
///
/// No 0600 chmod: the file no longer contains secrets.
fn write_all(accounts: &[PersistedAccount]) -> Result<(), SignalError> {
    let dir = config_dir().ok_or_else(|| {
        SignalError::Other("could not determine a config directory for this platform".into())
    })?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| SignalError::Other(format!("create config dir: {e}")))?;

    let path = dir.join(ACCOUNTS_FILE);
    let json = serde_json::to_string_pretty(accounts)
        .map_err(|e| SignalError::Other(format!("serialize accounts: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| SignalError::Other(format!("write {}: {e}", path.display())))?;
    Ok(())
}

/// Dev-only migration: if `accounts.json` was written by an earlier build that
/// kept secrets in the file, hoist them into the keyring and rewrite the file
/// with only the public fields. Same for any legacy single-account
/// `account.json`. Runs at most once because the source state is rewritten.
///
/// End-user migration is NOT supported — users on the old format should
/// relink their accounts.
fn migrate_dev_state_if_present() -> Result<(), SignalError> {
    migrate_legacy_account_json()?;
    migrate_accounts_json_with_secrets()
}

#[derive(serde::Deserialize)]
struct LegacyAccount {
    phone: String,
    password: String,
    aci: Option<String>,
    pni: Option<String>,
    registration_id: u32,
    aci_identity: String,
    pni_identity: String,
    master_key: String,
    profile_key: String,
}

impl LegacyAccount {
    fn split(self) -> (PersistedAccount, AccountSecrets) {
        (
            PersistedAccount {
                phone: self.phone,
                aci: self.aci,
                pni: self.pni,
                registration_id: self.registration_id,
                desktop_profile: None,
            },
            AccountSecrets {
                password: self.password,
                aci_identity_b64: self.aci_identity,
                pni_identity_b64: self.pni_identity,
                master_key_b64: self.master_key,
                profile_key_b64: self.profile_key,
            },
        )
    }
}

fn migrate_legacy_account_json() -> Result<(), SignalError> {
    let Some(legacy) = legacy_path() else {
        return Ok(());
    };
    let bytes = match std::fs::read(&legacy) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(SignalError::Other(format!(
                "read legacy {}: {e}",
                legacy.display()
            )))
        }
    };
    let legacy_account: LegacyAccount = serde_json::from_slice(&bytes).map_err(|e| {
        SignalError::Other(format!("parse legacy {}: {e}", legacy.display()))
    })?;
    let (public, secrets) = legacy_account.split();

    let mut existing = read_accounts_raw()?;
    if !existing.iter().any(|a| a.phone == public.phone) {
        write_secrets(&public.phone, &secrets)?;
        existing.push(public);
        write_all(&existing)?;
    }
    let _ = std::fs::remove_file(&legacy);
    Ok(())
}

fn migrate_accounts_json_with_secrets() -> Result<(), SignalError> {
    let Some(path) = accounts_path() else {
        return Ok(());
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(SignalError::Other(format!("read {}: {e}", path.display()))),
    };

    // Try the legacy (with-secrets) array first — its required fields are a
    // superset of the new format, so a legacy blob also parses as new but
    // the reverse isn't true. Test legacy first to detect the upgrade case.
    let Ok(legacy) = serde_json::from_slice::<Vec<LegacyAccount>>(&bytes) else {
        return Ok(());
    };

    let mut public_only = Vec::with_capacity(legacy.len());
    for entry in legacy {
        let (public, secrets) = entry.split();
        write_secrets(&public.phone, &secrets)?;
        public_only.push(public);
    }
    write_all(&public_only)?;
    Ok(())
}

/// Read the public-metadata file without triggering migration. Used during
/// the migration step itself to avoid infinite recursion.
fn read_accounts_raw() -> Result<Vec<PersistedAccount>, SignalError> {
    let Some(path) = accounts_path() else {
        return Ok(vec![]);
    };
    match std::fs::read(&path) {
        Ok(b) => serde_json::from_slice(&b)
            .map_err(|e| SignalError::Other(format!("parse {}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(SignalError::Other(format!("read {}: {e}", path.display()))),
    }
}

/// All saved accounts as live `SignalAccount` values. Any account whose
/// secrets fail to load is skipped (logged to stderr) so one missing or
/// corrupt keyring entry doesn't hide the rest.
pub fn list() -> Result<Vec<SignalAccount>, SignalError> {
    let persisted = read_all()?;
    let mut accounts = Vec::with_capacity(persisted.len());
    for p in persisted {
        let phone = p.phone.clone();
        match read_secrets(&phone).and_then(|s| SignalAccount::try_from_persisted(p, s)) {
            Ok(a) => accounts.push(a),
            Err(e) => eprintln!("Skipping unreadable saved account ({phone}): {e}"),
        }
    }
    Ok(accounts)
}

/// Add `account` to the saved list, replacing any existing entry with the
/// same phone number. Public metadata is written to disk; secrets are
/// written to the keyring (one entry per field).
pub fn save(account: &SignalAccount) -> Result<(), SignalError> {
    let (public, secrets) = account.to_persisted();
    write_secrets(&public.phone, &secrets)?;

    let mut all = read_all()?;
    all.retain(|a| a.phone != public.phone);
    all.push(public);
    write_all(&all)
}

/// Remove the saved account with the given phone number. Wipes its keyring
/// entries too. No-op if absent.
pub fn delete(phone: &str) -> Result<(), SignalError> {
    let mut all = read_all()?;
    let before = all.len();
    all.retain(|a| a.phone != phone);
    delete_secrets(phone)?;

    if all.len() == before {
        return Ok(());
    }
    if all.is_empty() {
        // Remove the file entirely when the last account is deleted, so a
        // fresh launch doesn't see a stale empty `[]`.
        if let Some(path) = accounts_path() {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(SignalError::Other(format!(
                        "delete {}: {e}",
                        path.display()
                    )))
                }
            }
        }
        Ok(())
    } else {
        write_all(&all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SIGNAL_SETUP_CONFIG_DIR` and the keyring default store are
    /// process-global, so persistence tests are serialized through this
    /// mutex to keep parallel `cargo test` runs sane.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_tempdir<F: FnOnce()>(f: F) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "signal-setup-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::env::set_var("SIGNAL_SETUP_CONFIG_DIR", &dir);
        std::env::set_var("SIGNAL_SETUP_TEST_KEYRING", "1");
        f();
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("SIGNAL_SETUP_CONFIG_DIR");
        // Note: SIGNAL_SETUP_TEST_KEYRING stays set across tests in the
        // process — that's fine, init_keyring runs once.
    }

    #[test]
    fn list_is_empty_on_fresh_install() {
        with_tempdir(|| {
            assert!(list().unwrap().is_empty());
        });
    }

    #[test]
    fn save_then_list_returns_account() {
        with_tempdir(|| {
            let a = SignalAccount::dummy("+15555550123");
            save(&a).unwrap();
            let listed = list().unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].phone, "+15555550123");
        });
    }

    #[test]
    fn save_multiple_accounts_keeps_all() {
        with_tempdir(|| {
            save(&SignalAccount::dummy("+15555550111")).unwrap();
            save(&SignalAccount::dummy("+15555550222")).unwrap();
            save(&SignalAccount::dummy("+15555550333")).unwrap();
            let listed = list().unwrap();
            let phones: Vec<&str> = listed.iter().map(|a| a.phone.as_str()).collect();
            assert!(phones.contains(&"+15555550111"));
            assert!(phones.contains(&"+15555550222"));
            assert!(phones.contains(&"+15555550333"));
        });
    }

    #[test]
    fn save_same_phone_replaces_not_duplicates() {
        with_tempdir(|| {
            let original = SignalAccount::dummy("+15555550123");
            save(&original).unwrap();
            let updated = SignalAccount::dummy("+15555550123");
            save(&updated).unwrap();
            let listed = list().unwrap();
            assert_eq!(listed.len(), 1, "same phone must not duplicate");
        });
    }

    #[test]
    fn delete_removes_only_the_named_phone() {
        with_tempdir(|| {
            save(&SignalAccount::dummy("+15555550111")).unwrap();
            save(&SignalAccount::dummy("+15555550222")).unwrap();
            delete("+15555550111").unwrap();
            let listed = list().unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].phone, "+15555550222");
        });
    }

    #[test]
    fn delete_missing_phone_is_idempotent() {
        with_tempdir(|| {
            delete("+15555550999").unwrap();
            save(&SignalAccount::dummy("+15555550111")).unwrap();
            delete("+15555550999").unwrap();
            assert_eq!(list().unwrap().len(), 1);
        });
    }

    #[test]
    fn delete_last_account_removes_file() {
        with_tempdir(|| {
            save(&SignalAccount::dummy("+15555550111")).unwrap();
            delete("+15555550111").unwrap();
            let path = accounts_path().unwrap();
            assert!(!path.exists(), "file should be gone after last delete");
        });
    }

    #[test]
    fn accounts_file_contains_no_secret_fields() {
        with_tempdir(|| {
            save(&SignalAccount::dummy("+15555550123")).unwrap();
            let path = accounts_path().unwrap();
            let body = std::fs::read_to_string(&path).unwrap();
            for needle in [
                "password",
                "aci_identity",
                "pni_identity",
                "master_key",
                "profile_key",
            ] {
                assert!(
                    !body.contains(needle),
                    "accounts.json must not mention secret field `{needle}`; got:\n{body}"
                );
            }
        });
    }

    #[test]
    fn legacy_account_json_migrates_into_keyring() {
        with_tempdir(|| {
            // Write a legacy single-account file by hand.
            let dir = config_dir().unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            let legacy = dir.join(LEGACY_FILE);
            let acct = SignalAccount::dummy("+15555550999");
            let (public, secrets) = acct.to_persisted();
            let legacy_blob = serde_json::json!({
                "phone": public.phone,
                "password": secrets.password,
                "aci": public.aci,
                "pni": public.pni,
                "registration_id": public.registration_id,
                "aci_identity": secrets.aci_identity_b64,
                "pni_identity": secrets.pni_identity_b64,
                "master_key": secrets.master_key_b64,
                "profile_key": secrets.profile_key_b64,
            });
            std::fs::write(&legacy, serde_json::to_string(&legacy_blob).unwrap()).unwrap();

            let listed = list().unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].phone, "+15555550999");

            assert!(!legacy.exists(), "legacy file must be removed");
            assert!(accounts_path().unwrap().exists());
        });
    }

    #[test]
    fn legacy_accounts_json_with_secrets_migrates_to_keyring() {
        with_tempdir(|| {
            // Hand-build an old-format array file (with secrets inlined).
            let dir = config_dir().unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            let acct = SignalAccount::dummy("+15555550555");
            let (public, secrets) = acct.to_persisted();
            let legacy_array = serde_json::json!([{
                "phone": public.phone,
                "password": secrets.password,
                "aci": public.aci,
                "pni": public.pni,
                "registration_id": public.registration_id,
                "aci_identity": secrets.aci_identity_b64,
                "pni_identity": secrets.pni_identity_b64,
                "master_key": secrets.master_key_b64,
                "profile_key": secrets.profile_key_b64,
            }]);
            let path = accounts_path().unwrap();
            std::fs::write(&path, serde_json::to_string(&legacy_array).unwrap()).unwrap();

            let listed = list().unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].phone, "+15555550555");

            // After migration, the file must no longer contain secrets.
            let body = std::fs::read_to_string(&path).unwrap();
            assert!(!body.contains("password"));
            assert!(!body.contains("aci_identity"));
        });
    }
}
