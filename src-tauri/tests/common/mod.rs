//! Shared fixture paths for model-gated integration tests. Both
//! `log_privacy.rs::engine_privacy` and `golden_wer.rs` resolve the same
//! `molvi-task0` model dir + RU example wav — this is the single source.
#![cfg(feature = "engine-model-test")]

use std::path::PathBuf;

pub fn fixture_model_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("molvi-task0")
        .join("models")
        .join("gigaam-v3-e2e-ctc")
}

pub fn fixture_wav() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("molvi-task0")
        .join("tests")
        .join("fixtures")
        .join("ru")
        .join("example.wav")
}
