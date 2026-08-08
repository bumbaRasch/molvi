//! Shared fixture paths for model-gated integration tests. Both
//! `log_privacy.rs::engine_privacy` and `golden_wer.rs` resolve the GigaAM
//! model dir + RU example wav here — the single source. The model dir comes
//! from `MOLVI_GIGAAM_MODEL_DIR` (fallback `../models/gigaam-v3-e2e-ctc`); the
//! wav lives in-repo at `tests/fixtures/ru/example.wav`.
#![cfg(feature = "engine-model-test")]

use std::path::PathBuf;

pub fn fixture_model_dir() -> PathBuf {
    std::env::var_os("MOLVI_GIGAAM_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("models")
                .join("gigaam-v3-e2e-ctc")
        })
}

pub fn fixture_wav() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("ru")
        .join("example.wav")
}
