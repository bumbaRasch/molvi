use std::path::{Path, PathBuf};

use crate::errors::{MolviError, Result};

const IDENTIFIER: &str = "com.molvi.app";

/// `%APPDATA%\com.molvi.app\`, created if missing. Ponytail: reading %APPDATA%
/// via the `dirs` crate would add a dep; `std::env::var("APPDATA")` is
/// sufficient on Windows (the only Phase-1 platform).
pub fn app_data_dir() -> Result<PathBuf> {
    let appdata = std::env::var("APPDATA")
        .map_err(|_| MolviError::Paths("APPDATA env var not set".into()))?;
    let dir = PathBuf::from(appdata).join(IDENTIFIER);
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

/// Redact the username-bearing %APPDATA% prefix from a path for privacy-safe
/// logging. `%APPDATA%` = `C:\Users\<name>\AppData\Roaming`; users share
/// molvi.log in bug reports, and <name> is PII-adjacent. Replace the prefix
/// with the literal `%APPDATA%` — mirrors how this file documents every path
/// (e.g. the `app_data_dir` doc comment: "%APPDATA%\com.molvi.app\"), preserves
/// the relative structure (debug value intact), and is instantly recognizable
/// to a Windows reader. If %APPDATA% is unset or the path isn't under it,
/// fall back to the raw path (user-picked import/export paths, test fixtures).
///
/// Ponytail: `std::env::var_os("APPDATA")`, NOT `dirs::home_dir()` — adds no
/// dep, matches the `app_data_dir()` pattern at line 11 (same ponytail call).
/// `strip_prefix` is component-based (doc-verified: "Only considers whole
/// path components to match") so `C:\Users\me\AppData\Roaming` won't partially
/// match `C:\Users\me2\...` — clean prefix-or-fall-through.
pub fn redact_appdata(path: &Path) -> String {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return path.display().to_string();
    };
    match path.strip_prefix(appdata) {
        Ok(rel) => format!("%APPDATA%\\{}", rel.display()),
        Err(_) => path.display().to_string(),
    }
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
        // ponytail: phase3 path asserted now so a rename can't slip past the
        // foundation task. (profiles.db was dropped — profiles live in
        // settings.json; see Task 7.)
        assert_eq!(snippets_db_path().unwrap(), base.join("snippets.db"));
    }

    #[test]
    fn redact_appdata_strips_prefix_and_falls_back() {
        // %APPDATA% is set on every Windows user session (app_data_dir()
        // already hard-depends on it at paths.rs:11). If a hostile env lacks
        // it, the helper returns the raw path — assert both branches.
        let appdata =
            std::env::var_os("APPDATA").expect("APPDATA set (app_data_dir depends on it)");
        // Under-%APPDATA% path: prefix replaced with the literal %APPDATA%.
        let under = PathBuf::from(&appdata)
            .join("com.molvi.app")
            .join("models")
            .join("gigaam-v3-e2e-ctc");
        assert_eq!(
            redact_appdata(&under),
            r"%APPDATA%\com.molvi.app\models\gigaam-v3-e2e-ctc"
        );
        // Foreign path (user-picked import file): not under %APPDATA% → raw.
        let foreign = Path::new(r"C:\foreign\dict.csv");
        assert_eq!(redact_appdata(foreign), foreign.display().to_string());
    }
}
