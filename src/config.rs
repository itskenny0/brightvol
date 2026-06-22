//! Persisted application state and first-run detection.
//!
//! State lives in `%APPDATA%\brightvol\config.json` on Windows. The file is
//! created on the first successful [`Config::save`]. A missing or corrupt file
//! is treated as "use defaults" so the app always starts.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Whether the first-run autostart prompt has already been shown.
    pub first_run_complete: bool,
    /// Whether the volume keys are currently remapped to brightness.
    pub intercept_enabled: bool,
    /// Whether brightvol is registered to start on login.
    pub autostart: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            first_run_complete: false,
            intercept_enabled: true,
            autostart: false,
        }
    }
}

impl Config {
    /// Path to the config file: `%APPDATA%\brightvol\config.json` on Windows,
    /// falling back to a per-user/temp location elsewhere (so tests and
    /// non-Windows builds work).
    pub fn config_path() -> PathBuf {
        config_dir().join("config.json")
    }

    /// Load config from the default path, returning defaults if the file is
    /// missing or cannot be parsed.
    pub fn load() -> Config {
        Self::load_from(&Self::config_path())
    }

    /// Load config from a specific path. Missing file or parse error yields
    /// [`Config::default`].
    pub fn load_from(path: &Path) -> Config {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    /// Persist config to the default path, creating the directory if needed.
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&Self::config_path())
    }

    /// Persist config to a specific path, creating parent directories.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, text)
    }
}

/// Resolve the directory that holds the config file.
fn config_dir() -> PathBuf {
    // Prefer %APPDATA% (set on Windows; harmless elsewhere if present).
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("brightvol");
    }
    // Fallbacks for non-Windows / unusual environments.
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("brightvol");
    }
    std::env::temp_dir().join("brightvol")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_expected_values() {
        let c = Config::default();
        assert!(!c.first_run_complete);
        assert!(c.intercept_enabled);
        assert!(!c.autostart);
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!("brightvol-test-rt-{}", std::process::id()));
        let path = dir.join("config.json");
        let original = Config {
            first_run_complete: true,
            intercept_enabled: false,
            autostart: true,
        };
        original.save_to(&path).unwrap();
        let loaded = Config::load_from(&path);
        assert_eq!(original, loaded);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_yields_defaults() {
        let path = std::env::temp_dir().join("brightvol-test-does-not-exist-xyz/config.json");
        let _ = std::fs::remove_file(&path);
        assert_eq!(Config::load_from(&path), Config::default());
    }

    #[test]
    fn garbage_yields_defaults() {
        let dir = std::env::temp_dir().join(format!("brightvol-test-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, "{ not valid json ]").unwrap();
        assert_eq!(Config::load_from(&path), Config::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn partial_json_fills_missing_with_defaults() {
        // `#[serde(default)]` should let an old/partial file still load.
        let dir = std::env::temp_dir().join(format!("brightvol-test-partial-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"autostart": true}"#).unwrap();
        let loaded = Config::load_from(&path);
        assert!(loaded.autostart);
        assert!(loaded.intercept_enabled); // default
        assert!(!loaded.first_run_complete); // default
        let _ = std::fs::remove_dir_all(&dir);
    }
}
