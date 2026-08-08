//! Spec §13: golden WER against the RU fixture, model-gated.
//!
//! **WER reference decision (Task 13):** there is no committed ground-truth
//! transcript for `example.wav` (Task-0 recorded "visibly correct Cyrillic +
//! punctuation" but not the exact text; the clip is a Pushkin passage but the
//! precise source could not be verified against the audio). Per the controller
//! guidance this test takes **option (b) — snapshot-golden**: the engine's
//! current output IS the golden reference, and the test asserts WER ~0 against
//! it. This is a **regression gate** (catches model/output drift), not an
//! accuracy measurement. True accuracy-WER needs a human-verified reference
//! transcript (deferred to a Phase-2 data task).
//!
//! Run: `cargo test --manifest-path src-tauri/Cargo.toml --features engine-model-test --test golden_wer`

#![cfg(feature = "engine-model-test")]

use std::path::PathBuf;

use molvi::engine::Engine;
use molvi::settings::Settings;

mod common;
use common::{fixture_model_dir, fixture_wav};

/// Snapshot of `Engine::transcribe_offline(example.wav)` captured on
/// 2026-08-03 (transcribe-rs 0.3.11 / ort 2.0.0-rc.12, int8). Treated as the
/// golden reference until a human-verified transcript replaces it.
const GOLDEN_EXAMPLE_WAV: &str = include_str!("../GOLDEN_EXAMPLE_WAV.txt");

#[test]
fn golden_wer_under_threshold() {
    let model_dir = fixture_model_dir();
    if !model_dir.exists() {
        eprintln!("skipping: model not present at {}", model_dir.display());
        return;
    }
    let wav = fixture_wav();

    let mut engine = Engine::load(&model_dir, &Settings::default()).expect("engine load");
    let hyp = engine
        .transcribe_offline(&wav)
        .expect("transcribe_offline")
        .text;

    let e = wer(&normalize(&hyp), &normalize(GOLDEN_EXAMPLE_WAV));
    let r = normalize(GOLDEN_EXAMPLE_WAV).split_whitespace().count();
    let wer_ratio = e as f64 / r.max(1) as f64;
    // Regression gate: int8 CPU inference is deterministic, so WER must be ~0.
    // 0.05 tolerates a single-word edge effect across a ~30-word clip without
    // masking a real model/output drift (which jumps well above this).
    assert!(
        wer_ratio < 0.05,
        "WER {wer_ratio:.3} >= 0.05 (regression vs golden)\n\
         golden: {GOLDEN_EXAMPLE_WAV}\n\
         hyp:    {hyp}"
    );
}

/// Lowercase, strip non-alphanumeric (keeps Cyrillic letters + digits), collapse
/// whitespace. Matches the brief's normalization.
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Word-level Levenshtein (sub+ins+del) edit distance. ponytail: O(n*m) DP is
/// fine for short clips (~30 words); standard Wagner-Fischer.
fn wer(hyp: &str, reference: &str) -> usize {
    let h: Vec<&str> = hyp.split_whitespace().collect();
    let r: Vec<&str> = reference.split_whitespace().collect();
    let (n, m) = (h.len(), r.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev = (0..=m).collect::<Vec<usize>>();
    let mut curr = vec![0usize; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if h[i - 1] == r[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// One-shot golden capture: transcribes the fixture and WRITES the result to
/// GOLDEN_EXAMPLE_WAV.txt (overwriting the placeholder). Run with `--ignored`
/// when regenerating the snapshot, then commit the file. Not part of the gate.
#[test]
#[ignore = "golden snapshot generator; run with --ignored to regenerate"]
fn capture_golden() {
    let model_dir = fixture_model_dir();
    if !model_dir.exists() {
        eprintln!("skipping: model not present at {}", model_dir.display());
        return;
    }
    let mut engine = Engine::load(&model_dir, &Settings::default()).expect("engine load");
    let hyp = engine
        .transcribe_offline(&fixture_wav())
        .expect("transcribe_offline")
        .text;
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("GOLDEN_EXAMPLE_WAV.txt");
    std::fs::write(&out, &hyp).expect("write golden");
    eprintln!("GOLDEN written to {}: {hyp}", out.display());
}

#[cfg(test)]
mod arity_checks {
    use super::*;

    #[test]
    fn wer_handles_empty_and_equal() {
        assert_eq!(wer("", "a b c"), 3);
        assert_eq!(wer("a b c", ""), 3);
        assert_eq!(wer("a b c", "a b c"), 0);
        assert_eq!(wer("a x c", "a b c"), 1);
    }

    #[test]
    fn normalize_strips_punctuation_and_case() {
        assert_eq!(normalize("Привет, Мир!"), "привет мир");
        assert_eq!(normalize("  A\nB  "), "a b");
    }
}
