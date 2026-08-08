// molvi Task 0 — verification gate.
// Loads v3_e2e_ctc natively via transcribe-rs, transcribes example.wav, reports RTF.
// WER is covered by the official GigaAM evaluation.md (e2e_ctc avg 12%, clean 3-10%);
// this binary confirms load + transcription + RTF on the dev CPU.

use std::path::PathBuf;
use std::time::Instant;
use transcribe_rs::audio;
use transcribe_rs::onnx::gigaam::GigaAMModel;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::{SpeechModel, TranscribeOptions};

const MODEL_DIR: &str = "models/gigaam-v3-e2e-ctc";
const CLIP: &str = "tests/fixtures/ru/example.wav";
const SAMPLE_RATE: usize = 16000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let load_start = Instant::now();
    let mut model = GigaAMModel::load(&PathBuf::from(MODEL_DIR), &Quantization::Int8)?;
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.0;

    let samples = audio::read_wav_samples(&PathBuf::from(CLIP))?;
    let dur_secs = samples.len() as f64 / SAMPLE_RATE as f64;

    let opts = TranscribeOptions {
        language: Some("ru".into()),
        ..Default::default()
    };

    let infer_start = Instant::now();
    let result = model.transcribe_raw(&samples, &opts)?;
    let infer_secs = infer_start.elapsed().as_secs_f64();

    let rtf = if dur_secs > 0.0 {
        infer_secs / dur_secs
    } else {
        f64::INFINITY
    };
    let has_punct = result.text.chars().any(|c| ".,!?;:—".contains(c));

    println!("=== molvi Task 0 ===");
    println!("model:      v3_e2e_ctc int8 (native transcribe-rs)");
    println!("clip:       {CLIP}  ({dur_secs:.2} s)");
    println!("load_ms:    {load_ms:.0}");
    println!("infer_s:    {infer_secs:.3}");
    println!(
        "rtf:        {rtf:.3}   (<0.7 = streaming OK, <1.0 = faster-than-realtime)"
    );
    println!("has_punct:  {has_punct}");
    println!("transcript: {}", result.text);
    Ok(())
}
