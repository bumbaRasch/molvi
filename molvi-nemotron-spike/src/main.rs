//! molvi Nemotron-3.5-ASR viability spike.
//!
//! Standalone measurement tool (NOT a molvi dependency). Loads Nemotron via
//! `parakeet-rs`, transcribes each clip over 560 ms (8960-sample) chunks, and
//! reports cold/warm load ms, per-utterance RTF, peak RSS, CPU%, and WER.
//! Emits `report/<UTC-stamp>.{md,json}` + a one-line GO/Conditional/NO-GO
//! verdict (spec §10.3: GO < 0.5, Conditional 0.5-1.0, NO-GO >= 1.0 median RTF).
//!
//! Privacy: logs and reports carry METADATA ONLY — no transcript text. The
//! hypothesis is reduced to a WER number and dropped.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use parakeet_rs::{Nemotron, NemotronMode};
use serde::Serialize;

mod wer;

const SAMPLE_RATE: usize = 16_000;
const CHUNK: usize = 8_960; // 560 ms at 16 kHz (parakeet-rs Nemotron streaming chunk)

const MODEL_ID: &str = "nemotron-3.5-asr-streaming-0.6b-onnx";

/// Process-affinity policy. `All` (default) lets the OS schedule every logical
/// core — MEASURED fastest for Nemotron on the i5-12450H (median RTF ~0.59 vs
/// ~0.86 pinned). `PCores` pins to performance cores; that HELPS GigaAM (molvi
/// §11/Task 5) but measured ~40% SLOWER for Nemotron, so it's opt-in here purely
/// to reproduce the finding.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Affinity {
    PCores,
    All,
}

/// Detect a P-core affinity mask (`EfficiencyClass == 0`) via
/// `GetLogicalProcessorInformationEx`. Returns `None` on a homogeneous CPU (no
/// E-cores → pointless) or any Win32/shape error (fail-open: inference runs on
/// all cores). Ported from molvi's `src-tauri/src/ort_affinity.rs` (proven on
/// this i5-12450H → mask 0xF00, 4 P-cores).
fn p_core_mask() -> Option<usize> {
    use windows::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, RelationProcessorCore,
        SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };
    let mut len: u32 = 0;
    unsafe {
        let _ = GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len);
    }
    if len == 0 {
        return None;
    }
    let mut buf: Vec<u8> = vec![0u8; len as usize];
    let filled = unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            Some(buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX),
            &mut len,
        )
    };
    if filled.is_err() {
        return None;
    }
    let total = len as usize;
    const HEADER: usize = 8;
    let mut mask: usize = 0;
    let mut is_heterogeneous = false;
    let mut offset = 0usize;
    while offset + HEADER <= total {
        let entry = unsafe {
            &*(buf.as_ptr().add(offset) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX)
        };
        let size = entry.Size as usize;
        let relationship = entry.Relationship;
        if size == 0 || offset + size > total {
            return None;
        }
        if relationship == RelationProcessorCore {
            let proc = unsafe { &entry.Anonymous.Processor };
            if proc.EfficiencyClass == 1 {
                is_heterogeneous = true;
            } else if proc.GroupCount >= 1 {
                mask |= proc.GroupMask[0].Mask as usize;
            }
        }
        offset += size;
    }
    if !is_heterogeneous || mask == 0 {
        return None;
    }
    Some(mask)
}

/// Apply the P-core mask to the current process. Returns `(mask, thread_count)`
/// on success; `None` on any failure / homogeneous CPU (fail-open).
fn apply_p_core_affinity() -> Option<(usize, usize)> {
    use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessAffinityMask};
    let mask = p_core_mask()?;
    let threads = mask.count_ones() as usize;
    let ok = unsafe { SetProcessAffinityMask(GetCurrentProcess(), mask) };
    if ok.is_ok() {
        Some((mask, threads))
    } else {
        None
    }
}

#[derive(Serialize)]
struct ClipRecord {
    clip: String,
    lang: String,
    clip_len_sec: f64,
    rtf: f64,
    wer: f64,
    cold_load_ms: u128,
    warm_load_ms: u128,
    peak_rss_bytes: u64,
    cpu_pct: f64,
    model_id: String,
    nemotron_mode: String,
    error: Option<String>,
}

#[derive(Serialize)]
struct Report {
    generated_at_utc: String,
    model_dir: String,
    model_id: String,
    nemotron_mode: String,
    target_lang: String,
    affinity: String,
    p_core_mask: Option<String>,
    cold_load_ms: u128,
    warm_load_ms: u128,
    peak_rss_bytes: u64,
    median_rtf: f64,
    mean_rtf: f64,
    max_rtf: f64,
    mean_wer: f64,
    verdict: String,
    clips: Vec<ClipRecord>,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("nemotron-spike: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode> {
    let args = parse_args(env::args_os().collect())?;
    let clips_dir = &args.clips;

    // Scan clips FIRST so the no-clips path exits cleanly without needing the model.
    let clips = scan_clips(clips_dir)?;
    if clips.is_empty() {
        println!(
            "No *.wav clips found in {}. Add 16 kHz mono WAV files with matching .txt references \
             (see clips/README.md).",
            clips_dir.display()
        );
        return Ok(ExitCode::SUCCESS);
    }

    let model_dir = args.model.ok_or_else(|| {
        anyhow!("missing --model <dir> (path to the extracted ONNX model directory)")
    })?;

    // --- Process affinity (MEASURED — see report/ + README "Findings"). ---
    // DEFAULT `all` (OS schedules every logical core) is FASTEST for Nemotron on
    // the i5-12450H: parakeet-rs's intra-op pool + aux work use the full 4P+4E
    // set. P-core pinning — which HELPS GigaAM (molvi §11/Task 5) — measured
    // ~40% SLOWER for Nemotron (median RTF ~0.86 vs ~0.59 across runs). So the
    // default is the fast path; `--affinity pcores` stays to reproduce that.
    // CRITICAL FOR TASK 18: Nemotron must NOT inherit molvi's process-wide
    // P-core affinity (it'd cost ~40% throughput vs GigaAM's gain).
    let (affinity_label, p_core_mask_str) = match args.affinity {
        Affinity::PCores => match apply_p_core_affinity() {
            Some((mask, _count)) => ("pcores", Some(format!("0x{mask:X}"))),
            None => {
                eprintln!(
                    "affinity=pcores requested but detection failed (homogeneous CPU?); running unpinned"
                );
                ("pcores (unpinned)", None)
            }
        },
        Affinity::All => ("all", None),
    };

    // parakeet-rs default ExecutionConfig (`None`) is fastest measured. ort
    // GraphOptimizationLevel::Level3 was RTF-neutral but ~doubled cold-load
    // (net-negative) — dropped. See README "Findings".
    let cold_start = Instant::now();
    let cold_model = Nemotron::from_pretrained(&model_dir, None)
        .with_context(|| format!("cold from_pretrained({})", model_dir.display()))?;
    let cold_load_ms = cold_start.elapsed().as_millis();
    let mode = cold_model.mode();
    drop(cold_model);

    // --- Warm load (fresh instance, ort runtime already warm). ---
    let warm_start = Instant::now();
    let mut model = Nemotron::from_pretrained(&model_dir, None)
        .with_context(|| format!("warm from_pretrained({})", model_dir.display()))?;
    let warm_load_ms = warm_start.elapsed().as_millis();

    let nemotron_mode = match model.mode() {
        NemotronMode::Multilingual => "Multilingual",
        NemotronMode::EnglishOnly => "EnglishOnly",
    };
    let target_lang = if mode == NemotronMode::Multilingual {
        model
            .set_target_lang("auto")
            .context("set_target_lang(\"auto\")")?;
        "auto".to_string()
    } else {
        "n/a (English-only)".to_string()
    };

    println!(
        "model={} mode={} affinity={} p_core_mask={} cold_load={}ms warm_load={}ms target_lang={}",
        MODEL_ID,
        nemotron_mode,
        affinity_label,
        p_core_mask_str.as_deref().unwrap_or("n/a"),
        cold_load_ms,
        warm_load_ms,
        target_lang
    );
    println!("measuring {} clip(s)...", clips.len());

    let mut records: Vec<ClipRecord> = Vec::with_capacity(clips.len());
    for (wav, txt) in &clips {
        let clip_name = wav
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>")
            .to_string();
        let lang = lang_from_basename(&clip_name);

        let result = (|| -> Result<(f64, f64, f64, String)> {
            let samples =
                load_wav_mono_f32(wav).with_context(|| format!("load_wav {}", wav.display()))?;
            let audio_dur = samples.len() as f64 / SAMPLE_RATE as f64;

            let reference = std::fs::read_to_string(txt)
                .with_context(|| format!("read ref {}", txt.display()))?;

            // Time ONLY the chunk loop. WAV decode + ref read are outside the timer.
            let cpu_before = process_cpu_secs().unwrap_or(0.0);
            let wall_start = Instant::now();
            let mut text = String::new();
            for chunk in samples.chunks(CHUNK) {
                text.push_str(&model.transcribe_chunk(chunk).context("transcribe_chunk")?);
            }
            let wall = wall_start.elapsed().as_secs_f64();
            let cpu_after = process_cpu_secs().unwrap_or(0.0);

            let rtf = if audio_dur > 0.0 {
                wall / audio_dur
            } else {
                0.0
            };
            let cpu_pct = if wall > 0.0 {
                ((cpu_after - cpu_before) / wall * 100.0).max(0.0)
            } else {
                0.0
            };
            let clip_wer = wer::wer(&reference, &text);
            // ponytail: `text` (hypothesis) is dropped here — never logged (privacy habit).
            Ok((audio_dur, rtf, cpu_pct, format!("{:.4}", clip_wer)))
        })();

        let peak_rss = peak_rss_bytes().unwrap_or(0);
        match result {
            Ok((dur, rtf, cpu_pct, wer_str)) => {
                let wer_val: f64 = wer_str.parse().unwrap_or(0.0);
                println!(
                    "  {} [{}] len={:.2}s rtf={:.3} cpu={:.1}% wer={:.3} peak_rss={}MB",
                    clip_name,
                    lang,
                    dur,
                    rtf,
                    cpu_pct,
                    wer_val,
                    peak_rss / (1024 * 1024)
                );
                records.push(ClipRecord {
                    clip: clip_name,
                    lang: lang.to_string(),
                    clip_len_sec: dur,
                    rtf,
                    wer: wer_val,
                    cold_load_ms,
                    warm_load_ms,
                    peak_rss_bytes: peak_rss,
                    cpu_pct,
                    model_id: MODEL_ID.to_string(),
                    nemotron_mode: nemotron_mode.to_string(),
                    error: None,
                });
            }
            Err(e) => {
                eprintln!("  {} [{}]: FAILED — {e:#}", clip_name, lang);
                records.push(ClipRecord {
                    clip: clip_name,
                    lang: lang.to_string(),
                    clip_len_sec: 0.0,
                    rtf: f64::INFINITY,
                    wer: 1.0,
                    cold_load_ms,
                    warm_load_ms,
                    peak_rss_bytes: peak_rss,
                    cpu_pct: 0.0,
                    model_id: MODEL_ID.to_string(),
                    nemotron_mode: nemotron_mode.to_string(),
                    error: Some(format!("{e:#}")),
                });
            }
        }
    }

    let (median, mean, max, mean_wer) = summarize(&records);
    let verdict = verdict_line(median);

    let peak_rss = peak_rss_bytes().unwrap_or(0);
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let stamp = utc_stamp(now_secs);
    let report = Report {
        generated_at_utc: stamp.clone(),
        model_dir: model_dir.display().to_string(),
        model_id: MODEL_ID.to_string(),
        nemotron_mode: nemotron_mode.to_string(),
        target_lang,
        affinity: affinity_label.to_string(),
        p_core_mask: p_core_mask_str,
        cold_load_ms,
        warm_load_ms,
        peak_rss_bytes: peak_rss,
        median_rtf: median,
        mean_rtf: mean,
        max_rtf: max,
        mean_wer,
        verdict: verdict.clone(),
        clips: records,
    };

    std::fs::create_dir_all(&args.report_dir)
        .with_context(|| format!("create {}", args.report_dir.display()))?;
    let md_path = args.report_dir.join(format!("{stamp}.md"));
    let json_path = args.report_dir.join(format!("{stamp}.json"));
    std::fs::write(&json_path, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("write {}", json_path.display()))?;
    std::fs::write(&md_path, render_md(&report))
        .with_context(|| format!("write {}", md_path.display()))?;

    println!("{verdict}");
    println!("report: {}", md_path.display());
    println!("        {}", json_path.display());
    Ok(ExitCode::SUCCESS)
}

struct Args {
    model: Option<PathBuf>,
    clips: PathBuf,
    report_dir: PathBuf,
    affinity: Affinity,
}

fn parse_args(argv: Vec<OsString>) -> Result<Args> {
    let mut model: Option<PathBuf> = None;
    let mut clips: Option<PathBuf> = None;
    let mut report_dir: Option<PathBuf> = None;
    let mut affinity: Affinity = Affinity::All;

    let mut i = 1;
    while i < argv.len() {
        let raw = &argv[i];
        let s = raw.to_string_lossy();
        let take_val = |idx: &mut usize| -> Result<PathBuf> {
            *idx += 1;
            if *idx >= argv.len() {
                return Err(anyhow!("missing value for {}", s));
            }
            Ok(PathBuf::from(argv[*idx].clone()))
        };
        match s.as_ref() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "--model" => model = Some(take_val(&mut i)?),
            "--clips" => clips = Some(take_val(&mut i)?),
            "--report-dir" => report_dir = Some(take_val(&mut i)?),
            "--affinity" => {
                // pcores = pin to performance cores (default, matches molvi);
                // all = let the OS schedule every logical core (baseline).
                i += 1;
                if i >= argv.len() {
                    return Err(anyhow!("missing value for --affinity (pcores|all)"));
                }
                affinity = match argv[i].to_string_lossy().as_ref() {
                    "pcores" | "p" => Affinity::PCores,
                    "all" | "a" => Affinity::All,
                    other => return Err(anyhow!("--affinity expects pcores|all, got `{other}`")),
                };
            }
            "--model-dir" => {
                // ponytail: common alias, same as --model.
                model = Some(take_val(&mut i)?)
            }
            other => return Err(anyhow!("unknown argument `{other}` (see --help)")),
        }
        i += 1;
    }

    Ok(Args {
        model,
        clips: clips.unwrap_or_else(|| PathBuf::from("clips")),
        report_dir: report_dir.unwrap_or_else(|| PathBuf::from("report")),
        affinity,
    })
}

fn print_usage() {
    eprintln!(
        "molvi-nemotron-spike — Nemotron-3.5-ASR viability measurement\n\n\
         USAGE:\n    \
         molvi-nemotron-spike --model <dir> [--clips <dir>] [--report-dir <dir>] [--affinity pcores|all]\n\n\
         ARGS:\n    \
         --model <dir>        Directory with the extracted ONNX model\n                         \
         (encoder.onnx, encoder.onnx.data, decoder_joint.onnx, tokenizer.model).\n    \
         --clips <dir>        Dir with *.wav + matching *.txt references (default: ./clips).\n    \
         --report-dir <dir>   Where to write <UTC-stamp>.{{md,json}} (default: ./report).\n    \
         --affinity <mode>    all = OS-schedule every logical core (DEFAULT, fastest for Nemotron on\n                         \
         this CPU). pcores = pin to P-cores (helps GigaAM but measured ~40% slower for\n                         \
         Nemotron — kept to reproduce the finding).\n\n\
         If no *.wav clips are found, prints a notice and exits 0."
    );
}

/// Collect (wav, txt) pairs from the clips dir, sorted by wav path.
fn scan_clips(dir: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut by_stem: BTreeMap<OsString, (Option<PathBuf>, Option<PathBuf>)> = BTreeMap::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        let stem = match path.file_stem() {
            Some(s) => s.to_os_string(),
            None => continue,
        };
        let slot = by_stem.entry(stem).or_insert((None, None));
        match ext.as_deref() {
            Some("wav") => slot.0 = Some(path),
            Some("txt") => slot.1 = Some(path),
            _ => {}
        }
    }
    let pairs = by_stem
        .into_values()
        .filter_map(|(w, t)| w.zip(t))
        .collect();
    Ok(pairs)
}

/// Decode a WAV (any sample rate/channels hound can read) to mono f32 @ the source rate.
/// The spike assumes 16 kHz mono already (see clips/README.md); if the file is stereo,
/// the first channel is taken. No resampling — a wrong sample rate will simply yield a
/// nonsense RTF/WER, which is the human's signal to fix the file.
fn load_wav_mono_f32(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if channels == 0 {
        return Err(anyhow!("0-channel wav"));
    }
    match spec.sample_format {
        hound::SampleFormat::Float => {
            let it = reader.samples::<f32>().filter_map(|s| s.ok());
            take_first_channel_f32(it, channels)
        }
        hound::SampleFormat::Int => {
            // ponytail: normalize by the integer range for the bit depth.
            let max = (1u64 << (spec.bits_per_sample - 1)) as f32;
            let it = reader
                .samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max);
            take_first_channel_f32(it, channels)
        }
    }
}

fn take_first_channel_f32(it: impl Iterator<Item = f32>, channels: usize) -> Result<Vec<f32>> {
    // ponytail: every Nth sample starting at 0 — exactly "first channel of an interleave".
    Ok(it.step_by(channels).collect())
}

fn lang_from_basename(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    let head = lower
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("");
    match head {
        "en" | "eng" | "english" => "en",
        "es" | "esp" | "spanish" => "es",
        "de" | "ger" | "german" => "de",
        "fr" | "fra" | "french" => "fr",
        "ru" | "rus" | "russian" => "ru",
        _ => "auto",
    }
}

fn summarize(records: &[ClipRecord]) -> (f64, f64, f64, f64) {
    let rtfs: Vec<f64> = records
        .iter()
        .filter(|r| r.error.is_none())
        .map(|r| r.rtf)
        .collect();
    let wers: Vec<f64> = records
        .iter()
        .filter(|r| r.error.is_none())
        .map(|r| r.wer)
        .collect();
    let median = {
        let mut v = rtfs.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if v.is_empty() {
            f64::INFINITY
        } else if v.len() % 2 == 1 {
            v[v.len() / 2]
        } else {
            (v[v.len() / 2 - 1] + v[v.len() / 2]) / 2.0
        }
    };
    let mean = |xs: &[f64]| {
        if xs.is_empty() {
            f64::NAN
        } else {
            xs.iter().sum::<f64>() / xs.len() as f64
        }
    };
    let max = rtfs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    (median, mean(&rtfs), max, mean(&wers))
}

/// spec §10.3: GO < 0.5, Conditional 0.5-1.0, NO-GO >= 1.0 (median RTF).
fn verdict_line(median_rtf: f64) -> String {
    let band = if median_rtf < 0.5 {
        "GO"
    } else if median_rtf < 1.0 {
        "Conditional"
    } else {
        "NO-GO"
    };
    let suffix = match band {
        "GO" => "real-time headroom; viable for default-on CPU path",
        "Conditional" => "real-time but tight; wire behind manual picker + latency warning",
        _ => "not real-time on this CPU; keep Nemotron off the default path",
    };
    format!(
        "VERDICT: {band} (median RTF {median_rtf:.3}) — Nemotron-3.5-ASR-0.6B {suffix}. [spec §10.3]"
    )
}

fn render_md(r: &Report) -> String {
    let mut md = String::new();
    md.push_str("# Nemotron-3.5-ASR viability spike — report\n\n");
    md.push_str(&format!("- generated (UTC): {}\n", r.generated_at_utc));
    md.push_str(&format!(
        "- model: `{}` ({})\n",
        r.model_id, r.nemotron_mode
    ));
    md.push_str(&format!("- model dir: `{}`\n", r.model_dir));
    md.push_str(&format!("- target lang: {}\n", r.target_lang));
    md.push_str(&format!(
        "- affinity: {} (mask {})\n",
        r.affinity,
        r.p_core_mask.as_deref().unwrap_or("n/a")
    ));
    md.push_str(&format!(
        "- cold load: {} ms / warm load: {} ms\n",
        r.cold_load_ms, r.warm_load_ms
    ));
    md.push_str(&format!(
        "- peak RSS: {:.1} MB ({} bytes)\n",
        r.peak_rss_bytes as f64 / (1024.0 * 1024.0),
        r.peak_rss_bytes
    ));
    md.push_str(&format!(
        "- median RTF: {:.3} / mean: {:.3} / max: {:.3} | mean WER: {:.3}\n\n",
        r.median_rtf, r.mean_rtf, r.max_rtf, r.mean_wer
    ));
    md.push_str(&format!("**{}**\n\n", r.verdict));
    md.push_str("| clip | lang | len (s) | RTF | CPU% | WER | status |\n");
    md.push_str("|------|------|--------:|----:|-----:|----:|--------|\n");
    for c in &r.clips {
        let status = c.error.as_deref().unwrap_or("ok");
        let rtf = if c.error.is_some() {
            "—".to_string()
        } else {
            format!("{:.3}", c.rtf)
        };
        let wer = if c.error.is_some() {
            "—".to_string()
        } else {
            format!("{:.3}", c.wer)
        };
        md.push_str(&format!(
            "| {} | {} | {:.2} | {} | {:.1} | {} | {} |\n",
            c.clip, c.lang, c.clip_len_sec, rtf, c.cpu_pct, wer, status
        ));
    }
    md.push_str(
        "\n Verdict bands (spec §10.3): GO < 0.5, Conditional 0.5–1.0, NO-GO ≥ 1.0 (median RTF).\n",
    );
    md
}

// ----------------------------- Windows metrics -------------------------------

fn peak_rss_bytes() -> Option<u64> {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;
    unsafe {
        let h = GetCurrentProcess();
        let mut counters = PROCESS_MEMORY_COUNTERS::default();
        GetProcessMemoryInfo(
            h,
            &mut counters as *mut _,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
        .ok()?;
        Some(counters.PeakWorkingSetSize as u64)
    }
}

fn process_cpu_secs() -> Option<f64> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    unsafe {
        let h = GetCurrentProcess();
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(
            h,
            &mut creation as *mut _,
            &mut exit as *mut _,
            &mut kernel as *mut _,
            &mut user as *mut _,
        )
        .ok()?;
        Some((filetime_to_secs(&kernel) + filetime_to_secs(&user)).max(0.0))
    }
}

fn filetime_to_secs(ft: &windows::Win32::Foundation::FILETIME) -> f64 {
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    // FILETIME is 100-ns intervals.
    ticks as f64 / 10_000_000.0
}

// ------------------------------- UTC stamp -----------------------------------

/// Format a Unix-seconds timestamp as `YYYYmmdd-HHMMSSZ` (UTC), std-only.
/// Civil-from-days via the well-known Hinnant algorithm. ~15 lines, no chrono.
fn utc_stamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let sod = unix_secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    let hh = sod / 3_600;
    let mm = (sod % 3_600) / 60;
    let ss = sod % 60;
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}Z", year, m, d, hh, mm, ss)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_stamp_known_epoch() {
        // Verified anchors (derived by hand, not a date lib):
        //   1970-01-01T00:00:00Z = unix 0
        //   1970-01-02T00:00:00Z = unix 86400  (exactly one day)
        //   1970-01-01T23:59:59Z = unix 86399  (last second of epoch day)
        //   2021-01-01T00:00:00Z = unix 1609459200
        //     (51*365 + 13 leap days in [1972,2020]) * 86400 = 18628 * 86400 = 1609459200
        assert_eq!(utc_stamp(0), "19700101-000000Z");
        assert_eq!(utc_stamp(86399), "19700101-235959Z");
        assert_eq!(utc_stamp(86400), "19700102-000000Z");
        assert_eq!(utc_stamp(1_609_459_200), "20210101-000000Z");
    }

    #[test]
    fn lang_from_basename_cases() {
        assert_eq!(lang_from_basename("en"), "en");
        assert_eq!(lang_from_basename("EN_clip2"), "en");
        assert_eq!(lang_from_basename("ru"), "ru");
        assert_eq!(lang_from_basename("foo"), "auto");
    }
}
