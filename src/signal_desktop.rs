//! Detection, profile management, and launching for Signal Desktop.
//!
//! Each Signal account registered through this tool gets its own Signal
//! Desktop profile (a `--user-data-dir` directory), so multiple numbers can
//! coexist on one machine. The first account registered on a machine that
//! already has a Signal Desktop install adopts the existing default
//! profile dir and is labelled `default`; subsequent accounts get a
//! `Signal-{sanitized-phone}` dir of their own.

use std::path::PathBuf;
use std::process::Command;

/// Profile name reserved for the pre-existing Signal Desktop install.
pub const DEFAULT_PROFILE: &str = "default";

/// Returns true if a Signal Desktop application bundle/binary is on disk.
pub fn is_installed() -> bool {
    install_paths().iter().any(|p| p.exists())
}

/// Returns true if Signal Desktop's default profile has been launched at
/// least once on this machine.
pub fn is_configured() -> bool {
    default_user_data_dir()
        .map(|p| p.join("config.json").exists())
        .unwrap_or(false)
}

/// Pick a profile name for a brand-new account being registered.
///
/// If Signal Desktop's default profile dir already exists and is not
/// already claimed (i.e. no other saved account is using it), reuse it —
/// the user almost certainly wants their existing Desktop install to be
/// the home for this number rather than getting a fresh empty profile.
/// Otherwise derive a per-phone name.
pub fn choose_profile_for_new_account(phone: &str, taken: &[String]) -> String {
    let default_taken = taken.iter().any(|p| p == DEFAULT_PROFILE);
    let default_exists = default_user_data_dir()
        .map(|p| p.exists())
        .unwrap_or(false);
    if default_exists && !default_taken {
        DEFAULT_PROFILE.to_string()
    } else {
        format!("Signal-{}", sanitize_phone(phone))
    }
}

/// Absolute path of the `--user-data-dir` to pass to Signal Desktop for the
/// given profile name. `default` resolves to Signal Desktop's standard
/// per-OS location; any other name maps to a sibling directory.
pub fn profile_path(profile: &str) -> Option<PathBuf> {
    if profile == DEFAULT_PROFILE {
        return default_user_data_dir();
    }
    default_user_data_dir().and_then(|p| p.parent().map(|parent| parent.join(profile)))
}

/// Launch Signal Desktop pointing at the given profile's `--user-data-dir`.
///
/// Spawned detached: the GUI returns immediately, Desktop keeps running
/// after this process exits. The directory is created if missing so
/// Desktop's first-run flow can populate it.
pub fn launch(profile: &str) -> Result<(), String> {
    let data_dir = profile_path(profile).ok_or_else(|| {
        "Could not determine Signal Desktop data directory for this platform".to_string()
    })?;
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        return Err(format!("create {}: {e}", data_dir.display()));
    }

    let exe = install_paths()
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| "Signal Desktop is not installed".to_string())?;

    spawn_detached(&exe, &data_dir).map_err(|e| format!("launch Signal Desktop: {e}"))
}

#[cfg(target_os = "macos")]
fn spawn_detached(app_bundle: &std::path::Path, data_dir: &std::path::Path) -> std::io::Result<()> {
    // `open -n -a Signal.app --args …` lets multiple instances run
    // concurrently — required when several accounts each have their own
    // profile dir.
    Command::new("open")
        .arg("-n")
        .arg("-a")
        .arg(app_bundle)
        .arg("--args")
        .arg(format!("--user-data-dir={}", data_dir.display()))
        .spawn()?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn spawn_detached(exe: &std::path::Path, data_dir: &std::path::Path) -> std::io::Result<()> {
    Command::new(exe)
        .arg(format!("--user-data-dir={}", data_dir.display()))
        .spawn()?;
    Ok(())
}

fn install_paths() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![PathBuf::from("/Applications/Signal.app")]
    }
    #[cfg(target_os = "linux")]
    {
        vec![
            PathBuf::from("/usr/bin/signal-desktop"),
            PathBuf::from("/usr/local/bin/signal-desktop"),
            PathBuf::from("/snap/bin/signal-desktop"),
            PathBuf::from("/var/lib/flatpak/exports/bin/org.signal.Signal"),
            dirs::home_dir()
                .map(|h| h.join(".local/share/flatpak/exports/bin/org.signal.Signal"))
                .unwrap_or_default(),
        ]
    }
    #[cfg(target_os = "windows")]
    {
        let mut v = vec![];
        if let Some(local) = dirs::data_local_dir() {
            v.push(local.join("Programs").join("signal-desktop").join("Signal.exe"));
        }
        v
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        vec![]
    }
}

/// Signal Desktop's standard `--user-data-dir` for this platform.
fn default_user_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::data_dir().map(|d| d.join("Signal"))
    }
    #[cfg(target_os = "linux")]
    {
        dirs::config_dir().map(|d| d.join("Signal"))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_dir().map(|d| d.join("Signal"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

/// Turn a phone number like `+15551234567` into a filesystem-safe segment
/// `15551234567`. Strips every char that isn't ASCII-alphanumeric so the
/// result is portable across macOS/Linux/Windows path rules.
fn sanitize_phone(phone: &str) -> String {
    phone.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_phone_strips_punctuation() {
        assert_eq!(sanitize_phone("+1 (555) 123-4567"), "15551234567");
        assert_eq!(sanitize_phone("+33612345678"), "33612345678");
    }

    #[test]
    fn choose_profile_uses_phone_when_default_already_claimed() {
        let chosen =
            choose_profile_for_new_account("+15551234567", &["default".to_string()]);
        assert_eq!(chosen, "Signal-15551234567");
    }
}
