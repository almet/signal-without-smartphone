//! On-disk persistence of registered Signal accounts.
//!
//! Stores accounts as a JSON array in the platform's config directory:
//!   macOS  : ~/Library/Application Support/signal-setup/accounts.json
//!   Linux  : $XDG_CONFIG_HOME/signal-setup/accounts.json
//!   Windows: %APPDATA%\signal-setup\accounts.json
//!
//! Identity private keys live in this file, so on Unix it is written with
//! mode 0600. Set `SIGNAL_SETUP_CONFIG_DIR` to override the location
//! (used by tests).
//!
//! Migration: if a legacy `account.json` (single-account format) exists at
//! the same location, it's read once on first use and folded into the new
//! array, then the old file is removed.

use crate::signal_http::{PersistedAccount, SignalAccount, SignalError};
use std::path::PathBuf;

const ACCOUNTS_FILE: &str = "accounts.json";
const LEGACY_FILE: &str = "account.json";

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

/// Read the array of persisted accounts from disk. Returns an empty Vec if
/// no file exists. Performs the one-time legacy migration if needed.
fn read_all() -> Result<Vec<PersistedAccount>, SignalError> {
    migrate_legacy_if_present()?;

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

/// Write the full array of persisted accounts back to disk, replacing any
/// previous contents. Creates the config directory and sets 0600 on Unix.
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

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)
            .map_err(|e| SignalError::Other(format!("stat {}: {e}", path.display())))?
            .permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&path, perms)
            .map_err(|e| SignalError::Other(format!("chmod {}: {e}", path.display())))?;
    }
    Ok(())
}

/// If a legacy single-account `account.json` exists, fold it into the array
/// file and delete it. Idempotent — runs at most once because the source
/// file is removed after a successful merge.
fn migrate_legacy_if_present() -> Result<(), SignalError> {
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
    let legacy_account: PersistedAccount = serde_json::from_slice(&bytes).map_err(|e| {
        SignalError::Other(format!("parse legacy {}: {e}", legacy.display()))
    })?;

    // Read whatever's in the new file (without re-triggering migration).
    let mut existing: Vec<PersistedAccount> = match accounts_path() {
        Some(p) => match std::fs::read(&p) {
            Ok(b) => serde_json::from_slice(&b)
                .map_err(|e| SignalError::Other(format!("parse {}: {e}", p.display())))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
            Err(e) => return Err(SignalError::Other(format!("read {}: {e}", p.display()))),
        },
        None => vec![],
    };
    if !existing.iter().any(|a| a.phone == legacy_account.phone) {
        existing.push(legacy_account);
        write_all(&existing)?;
    }
    let _ = std::fs::remove_file(&legacy);
    Ok(())
}

/// All saved accounts as live `SignalAccount` values. Any account whose
/// persisted form fails to decode is skipped (logged to stderr) so one
/// corrupt entry doesn't hide the rest.
pub fn list() -> Result<Vec<SignalAccount>, SignalError> {
    let persisted = read_all()?;
    let mut accounts = Vec::with_capacity(persisted.len());
    for p in persisted {
        let phone = p.phone.clone();
        match SignalAccount::try_from_persisted(p) {
            Ok(a) => accounts.push(a),
            Err(e) => eprintln!("Skipping unreadable saved account ({phone}): {e}"),
        }
    }
    Ok(accounts)
}

/// Add `account` to the saved list, replacing any existing entry with the
/// same phone number.
pub fn save(account: &SignalAccount) -> Result<(), SignalError> {
    let mut all = read_all()?;
    all.retain(|a| a.phone != account.phone);
    all.push(account.to_persisted());
    write_all(&all)
}

/// Remove the saved account with the given phone number. No-op if absent.
pub fn delete(phone: &str) -> Result<(), SignalError> {
    let mut all = read_all()?;
    let before = all.len();
    all.retain(|a| a.phone != phone);
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

    /// `SIGNAL_SETUP_CONFIG_DIR` is process-global, so persistence tests are
    /// serialized through this mutex to keep parallel `cargo test` runs sane.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_tempdir<F: FnOnce()>(f: F) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!(
            "signal-setup-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::env::set_var("SIGNAL_SETUP_CONFIG_DIR", &dir);
        f();
        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("SIGNAL_SETUP_CONFIG_DIR");
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
            // Save again with the same phone (different identity keys).
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

    #[cfg(unix)]
    #[test]
    fn saved_file_is_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        with_tempdir(|| {
            save(&SignalAccount::dummy("+15555550123")).unwrap();
            let path = accounts_path().unwrap();
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        });
    }

    #[test]
    fn legacy_account_json_migrates_into_array() {
        with_tempdir(|| {
            // Write a legacy single-account file by hand.
            let dir = config_dir().unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            let legacy = dir.join(LEGACY_FILE);
            let acct = SignalAccount::dummy("+15555550999");
            std::fs::write(
                &legacy,
                serde_json::to_string(&acct.to_persisted()).unwrap(),
            )
            .unwrap();

            // First call triggers migration.
            let listed = list().unwrap();
            assert_eq!(listed.len(), 1);
            assert_eq!(listed[0].phone, "+15555550999");

            // Legacy file is gone, new file is in place.
            assert!(!legacy.exists(), "legacy file must be removed");
            assert!(accounts_path().unwrap().exists());
        });
    }
}
