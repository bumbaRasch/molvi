use thiserror::Error;

#[derive(Debug, Error)]
pub enum MolviError {
    #[error("paths: {0}")]
    Paths(String),

    #[error("settings: {0}")]
    Settings(String),

    #[error("model store: {0}")]
    ModelStore(String),

    #[error("audio: {0}")]
    Audio(String),

    #[error("engine: {0}")]
    Engine(String),

    #[error("inference: {0}")]
    Inference(String),

    #[error("hotkey: {0}")]
    Hotkey(String),

    #[error("paste: {0}")]
    Paste(String),

    #[error("overlay: {0}")]
    Overlay(String),

    #[error("db: {0}")]
    Db(String),

    #[error("dictionary: {0}")]
    Dictionary(String),

    #[error("profile: {0}")]
    Profile(String),

    #[error("snippet: {0}")]
    Snippet(String),

    #[error("updater: {0}")]
    Updater(String),
}

pub type Result<T> = std::result::Result<T, MolviError>;

// R1: Tauri commands return Result<T, MolviError>; the frontend receives the
// Display string. `e` is always metadata (io/serde/sqlite error strings).
impl serde::Serialize for MolviError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_displays_with_category() {
        let e = MolviError::Audio("mic denied".into());
        assert_eq!(e.to_string(), "audio: mic denied");
    }
}
