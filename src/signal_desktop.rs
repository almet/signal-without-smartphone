//! Detection helpers for Signal Desktop.
//!
//! Used by the GUI to surface a soft warning if Desktop isn't installed/
//! configured, since the user will need it for the linking step.

use std::path::PathBuf;

/// Returns true if a Signal Desktop application bundle/binary is on disk.
pub fn is_installed() -> bool {
    install_paths().iter().any(|p| p.exists())
}

/// Returns true if Signal Desktop has been launched at least once on this
/// machine (i.e. its user-data directory has been created).
pub fn is_configured() -> bool {
    user_data_dir()
        .map(|p| p.join("config.json").exists())
        .unwrap_or(false)
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

fn user_data_dir() -> Option<PathBuf> {
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
