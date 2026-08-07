use std::path::{Path, PathBuf};

use crate::errors::{MolviError, Result};

const IDENTIFIER: &str = "com.molvi.app";

/// Per-OS app-data root, then `IDENTIFIER` (`com.molvi.app`), created if missing.
/// Windows = `%APPDATA%`; macOS = `~/Library/Application Support`; Linux =
/// `$XDG_CONFIG_HOME` (default `~/.config`). std-only (no `dirs` dep — matches
/// the original ponytail call).
pub fn app_data_dir() -> Result<PathBuf> {
    let base: String = {
        #[cfg(target_os = "windows")]
        {
            // ponytail: %APPDATA% = C:\Users\<name>\AppData\Roaming (std only).
            std::env::var("APPDATA")
                .map_err(|_| MolviError::Paths("APPDATA env var not set".into()))?
        }
        #[cfg(target_os = "macos")]
        {
            // ~/Library/Application Support (osx-conventional; std only).
            let home = std::env::var("HOME")
                .map_err(|_| MolviError::Paths("HOME env var not set".into()))?;
            format!("{home}/Library/Application Support")
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            // $XDG_CONFIG_HOME (default ~/.config) — XDG Base Dir spec.
            std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_default();
                format!("{home}/.config")
            })
        }
    };
    let dir = PathBuf::from(base).join(IDENTIFIER);
    std::fs::create_dir_all(&dir)
        .map_err(|e| MolviError::Paths(format!("create app data dir: {e}")))?;
    Ok(dir)
}

pub fn models_dir() -> Result<PathBuf> {
    let dir = app_data_dir()?.join("models");
    std::fs::create_dir_all(&dir)
        .map_err(|e| MolviError::Paths(format!("create models dir: {e}")))?;
    Ok(dir)
}

pub fn settings_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("settings.json"))
}

pub fn history_db_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("molvi.db"))
}

pub fn dictionary_db_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("dictionary.db"))
}

// ponytail: phase3 foundation path. snippets.db consumed by Task 6. No
// caller in the foundation task — alive via this fn + the nested-path test
// below (keeps dead-code lint off until the DB layer lands). Per-app
// profiles live in settings.json (no profiles.db — Task 7 ponytail call).
pub fn snippets_db_path() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("snippets.db"))
}

pub fn log_dir() -> Result<PathBuf> {
    let dir = app_data_dir()?.join("logs");
    std::fs::create_dir_all(&dir).map_err(|e| MolviError::Paths(format!("create log dir: {e}")))?;
    Ok(dir)
}

/// Redact the username-bearing home-ish prefix from a path for privacy-safe
/// logging: `%APPDATA%` on Windows, `$HOME` on Unix. Both expand to a path
/// containing the OS username (PII-adjacent in shared bug-report logs). Falls
/// back to the raw path when the prefix is unset or the path isn't under it
/// (user-picked import/export paths, test fixtures).
pub fn redact_appdata(path: &Path) -> String {
    // Windows: redact %APPDATA% prefix.
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA")
            && let Ok(rel) = path.strip_prefix(&appdata)
        {
            return format!("%APPDATA%\\{}", rel.display());
        }
    }
    // Unix (macOS + Linux): redact $HOME prefix — ~/Library/…, ~/.config/….
    #[cfg(unix)]
    {
        if let Some(home) = std::env::var_os("HOME")
            && !home.is_empty()
            && let Ok(rel) = path.strip_prefix(&home)
        {
            return format!("~{}", rel.display());
        }
    }
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_data_dir_ends_with_identifier() {
        let dir = app_data_dir().unwrap();
        assert!(dir.ends_with("com.molvi.app"));
        assert!(dir.exists(), "dir should be created on call");
    }

    #[test]
    fn subpaths_are_nested() {
        let base = app_data_dir().unwrap();
        assert_eq!(models_dir().unwrap(), base.join("models"));
        assert_eq!(settings_path().unwrap(), base.join("settings.json"));
    }

    #[test]
    fn db_paths_are_nested() {
        let base = app_data_dir().unwrap();
        assert_eq!(history_db_path().unwrap(), base.join("molvi.db"));
        assert_eq!(dictionary_db_path().unwrap(), base.join("dictionary.db"));
        // ponytail: phase3 path asserted now so a rename can't slip past.
        // (profiles.db was dropped — profiles live in settings.json.)
        assert_eq!(snippets_db_path().unwrap(), base.join("snippets.db"));
    }

    #[test]
    fn redact_appdata_strips_prefix_and_falls_back() {
        // Windows branch: %APPDATA% prefix → "%APPDATA%\…".
        #[cfg(target_os = "windows")]
        {
            let appdata = std::env::var_os("APPDATA").expect("APPDATA set");
            let under = PathBuf::from(&appdata)
                .join("com.molvi.app")
                .join("models")
                .join("gigaam-v3-e2e-ctc");
            assert_eq!(
                redact_appdata(&under),
                r"%APPDATA%\com.molvi.app\models\gigaam-v3-e2e-ctc"
            );
        }
        // Unix branch (macOS + Linux): $HOME prefix → "~/…", username gone.
        #[cfg(unix)]
        {
            let home = std::env::var_os("HOME").expect("HOME set");
            let under = PathBuf::from(&home)
                .join(if cfg!(target_os = "macos") {
                    "Library/Application Support"
                } else {
                    ".config"
                })
                .join("com.molvi.app");
            let r = redact_appdata(&under);
            assert!(r.starts_with('~'), "redacted should start with ~: {r}");
            // username must NOT appear in the redacted form.
            let home_str = std::env::var("HOME").unwrap();
            assert!(!r.contains(&home_str), "HOME leaked into redacted: {r}");
        }
        // Foreign path (not under any home prefix): raw passthrough (all OSes).
        let foreign = if cfg!(windows) {
            Path::new(r"C:\foreign\dict.csv")
        } else {
            Path::new("/srv/foreign/dict.csv")
        };
        assert_eq!(redact_appdata(foreign), foreign.display().to_string());
    }
}
