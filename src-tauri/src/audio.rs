use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat};
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME};
use windows::core::PCWSTR;

use crate::errors::{MolviError, Result};

// ponytail: fixed 1600-sample RMS window. At the common 48k native rate that's
// ~33ms -> ~30 updates/s (matches the overlay's ~30fps); at 16k ~100ms (10/s).
// A native rate != 16k is handled by Task 8's resampler, not by resizing this.
const MIC_LEVEL_WINDOW: usize = 1_600;

/// Mic capture handle.
///
/// Owns the cpal input `stream` (which itself owns the ring's `Producer` half,
/// moved into the realtime data callback at build time) plus the ring's
/// `Consumer` half (`consumer()` hands it to the Task-8 worker). `mic_level()`
/// exposes an RMSx1000 meter the overlay polls.
///
/// rtrb note (verified docs.rs/rtrb/0.3.4): `Producer`/`Consumer` are `Send` but
/// neither `Clone` nor `Sync` — strict single-producer/single-consumer. So the
/// brief's "clones cheaply" accessor shape is impossible; the consumer is taken
/// once via `consumer(&mut self)`. Both ring halves stay alive for the struct's
/// lifetime: the producer lives inside `stream`'s callback, the consumer here.
///
/// `Send` but not `Sync` (cpal `Stream` is `!Sync`). Task 13's `Pipeline`
/// decides ownership; nothing constructs this until then.
pub struct AudioCapture {
    stream: cpal::Stream,
    ring_rx: Option<rtrb::Consumer<f32>>,
    mic_level: Arc<AtomicU32>,
    native_rate: u32,
}

impl AudioCapture {
    /// Open the default (or named) input device and start capturing. The stream
    /// is live on return; recording sessions `pause()`/`resume()` around it.
    pub fn start(input_device: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let device = match input_device {
            // DeviceTrait: Display in 0.18 (no name()) -> match via to_string().
            Some(name) => host
                .input_devices()
                .map_err(|e| MolviError::Audio(format!("enum devices: {e}")))?
                .find(|d| d.to_string() == *name)
                .ok_or_else(|| MolviError::Audio(format!("device not found: {name}")))?,
            None => host
                .default_input_device()
                .ok_or_else(|| MolviError::Audio("no default input device".into()))?,
        };

        let supported = device
            .default_input_config()
            .map_err(|e| MolviError::Audio(format!("default config: {e}")))?;
        let native_rate = supported.sample_rate();
        let channels = supported.channels() as usize;
        let fmt = supported.sample_format();
        // ponytail: capture at the device's native rate; Task 8 resamples to 16k
        // (no-op fast path when native is already 16k). Probing supported_input
        // _configs() for a 16k-native stream adds code for a case the resampler
        // already covers, so the brief's "desired 16k" hint is intentionally not
        // implemented here.
        let cfg = supported.config();

        // SPSC ring sized for ~2s at the native fill rate (48k -> 96k samples).
        // ponytail: brief used TARGET_RATE*2 (32k) which is only ~0.67s at 48k;
        // sizing at native_rate holds the spec's "~2s" intent at the real rate.
        let (ring_tx, ring_rx) = rtrb::RingBuffer::<f32>::new(native_rate as usize * 2);
        let mic_level = Arc::new(AtomicU32::new(0));

        let err_cb = |e: cpal::Error| tracing::error!("cpal stream error: {e}");
        let stream = match fmt {
            SampleFormat::F32 => device.build_input_stream(
                cfg,
                make_callback::<f32>(channels, ring_tx, mic_level.clone()),
                err_cb,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                cfg,
                make_callback::<i16>(channels, ring_tx, mic_level.clone()),
                err_cb,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                cfg,
                make_callback::<u16>(channels, ring_tx, mic_level.clone()),
                err_cb,
                None,
            ),
            other => {
                return Err(MolviError::Audio(format!(
                    "unsupported sample format: {other:?}"
                )));
            }
        }
        .map_err(|e| MolviError::Audio(format!("build input stream: {e}")))?;

        stream
            .play()
            .map_err(|e| MolviError::Audio(format!("stream play: {e}")))?;

        Ok(Self {
            stream,
            ring_rx: Some(ring_rx),
            mic_level,
            native_rate,
        })
    }

    /// Take the SPSC consumer for the Task-8 worker thread. One-shot (returns
    /// `None` after the first call): rtrb enforces a single consumer, so the
    /// brief's clone-accessor can't exist — the worker gets the only handle.
    pub fn consumer(&mut self) -> Option<rtrb::Consumer<f32>> {
        self.ring_rx.take()
    }

    /// RMSx1000 mic level, updated ~30fps. The overlay polls this on a timer.
    pub fn mic_level(&self) -> Arc<AtomicU32> {
        self.mic_level.clone()
    }

    /// Device-native sample rate; the worker builds its resampler against this.
    pub fn native_rate(&self) -> u32 {
        self.native_rate
    }

    pub fn pause(&self) {
        let _ = self.stream.pause();
    }

    pub fn resume(&self) {
        let _ = self.stream.play();
    }
}

/// Build the realtime data callback for sample type `T`. Allocation-free: the
/// closure owns the `Producer`, downmixes each frame to mono f32 in place,
/// pushes to the ring (drop on overflow - never block), and folds sum-of-squares
/// into stack locals. `+ Send + 'static` satisfies cpal's callback bounds; the
/// capture set (Producer, Arc, two scalars, a usize) is Send regardless of `T`.
fn make_callback<T>(
    channels: usize,
    ring_tx: rtrb::Producer<f32>,
    mic_level: Arc<AtomicU32>,
) -> impl FnMut(&[T], &cpal::InputCallbackInfo) + Send + 'static
where
    T: Sample,
    f32: FromSample<T>,
{
    let mut ring_tx = ring_tx;
    let mut sum_sq = 0.0_f64;
    let mut count = 0_usize;
    move |samples, _info| {
        for frame in samples.chunks(channels) {
            let mono: f32 = if channels > 1 {
                frame.iter().map(|s| f32::from_sample(*s)).sum::<f32>() / channels as f32
            } else {
                f32::from_sample(frame[0])
            };
            // Drop on overflow: losing samples beats blocking the audio thread.
            let _ = ring_tx.push(mono);
            sum_sq += f64::from(mono * mono);
            count += 1;
            if count >= MIC_LEVEL_WINDOW {
                let rms = (sum_sq / count as f64).sqrt() as f32;
                mic_level.store((rms * 1000.0) as u32, Ordering::Relaxed);
                sum_sq = 0.0;
                count = 0;
            }
        }
    }
}

/// Which feedback tone to play.
#[derive(Copy, Clone, Debug)]
pub enum Tone {
    Start,
    Stop,
}

/// Synthesize a short sine tone with linear fade in/out (avoids click
/// discontinuities at the buffer edges). Pure & unit-testable.
pub fn tone_samples(freq_hz: f32, dur_ms: u32, sample_rate: u32) -> Vec<f32> {
    let n = ((dur_ms as f64 / 1000.0) * sample_rate as f64).round() as usize;
    let fade = ((sample_rate as f32 / 200.0) as usize).min(n / 2); // ~5ms, clamped
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sample_rate as f32;
        let mut amp = (2.0 * std::f32::consts::PI * freq_hz * t).sin() * 0.3;
        if i < fade {
            amp *= i as f32 / fade as f32;
        } else if i >= n.saturating_sub(fade) {
            amp *= (n - i) as f32 / fade as f32;
        }
        out.push(amp);
    }
    out
}

/// Play a feedback tone. Fire-and-forget: spawns a short-lived thread so the
/// caller (the coordinator thread) never blocks. Best-effort — cpal/device
/// errors are logged (metadata-only) and swallowed; a failed beep never crashes
/// the app. Caller gates on `settings.overlay.sounds.enabled`.
pub fn play_tone(kind: Tone) {
    if let Err(e) = std::thread::Builder::new()
        .name("molvi-tone".into())
        .spawn(move || {
            if let Err(e) = play_tone_blocking(kind) {
                tracing::warn!("tone playback skipped: {e}");
            }
        })
    {
        tracing::warn!("tone thread spawn failed: {e}");
    }
}

fn play_tone_blocking(kind: Tone) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| MolviError::Audio("no default output device".into()))?;
    // Request F32 at a standard rate so the fill callback is `&mut [f32]`.
    // cpal 0.18: StreamConfig passed by value, SampleRate is a u32 alias.
    let supported = device
        .supported_output_configs()
        .map_err(|e| MolviError::Audio(format!("enum output configs: {e}")))?
        .find_map(|r| {
            if r.sample_format() == SampleFormat::F32 {
                r.try_with_standard_sample_rate()
            } else {
                None
            }
        })
        .ok_or_else(|| MolviError::Audio("no F32 output config at standard rate".into()))?;
    let sample_rate = supported.sample_rate();
    // ponytail: BufferSize::Default (host/device default) is intentional for this
    // one-shot ~120ms beep. cpal 0.18 docs warn Default "may lead to varying
    // latency and potential underruns" — that caveat targets latency-sensitive
    // CONTINUOUS playback (music/comms), not a fire-and-forget feedback blip.
    // Playback is real-time-consumed, so the tone completes fully regardless of
    // buffer size; only first-sample startup latency varies (imperceptible).
    // Fixed(size) would need a rebuild-fallback (the FnMut callback isn't
    // reusable across two build_output_stream calls) — not worth the code for a
    // default-off feature. Verified via ctx7 /rustaudio/cpal.
    let cfg = supported.config();
    let (freq, dur_ms) = match kind {
        Tone::Start => (880.0, 120),
        Tone::Stop => (660.0, 120),
    };
    let samples = Arc::new(tone_samples(freq, dur_ms, sample_rate));
    let cursor = Arc::new(AtomicUsize::new(0));
    let err_cb = |e: cpal::Error| tracing::error!("cpal output stream error: {e}");
    let stream = device
        .build_output_stream(
            cfg,
            move |buf: &mut [f32], _| {
                let mut c = cursor.load(Ordering::Relaxed);
                for slot in buf.iter_mut() {
                    *slot = if c < samples.len() {
                        let s = samples[c];
                        c += 1;
                        s
                    } else {
                        0.0
                    };
                }
                cursor.store(c, Ordering::Relaxed);
            },
            err_cb,
            None,
        )
        .map_err(|e| MolviError::Audio(format!("build output stream: {e}")))?;
    stream
        .play()
        .map_err(|e| MolviError::Audio(format!("output play: {e}")))?;
    // Hold the stream alive so the callback drains the tone, then it drops.
    std::thread::sleep(std::time::Duration::from_millis(dur_ms as u64 + 80));
    Ok(())
}

/// Play a user-chosen .wav via Win32 PlaySoundW. Fire-and-forget (SND_ASYNC),
/// returns immediately. Best-effort — a fixed failure message is logged
/// (metadata-only; no path/content interpolated) and swallowed.
pub fn play_sound_file(path: &str) {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0u16)).collect();
    let flags = SND_FILENAME | SND_ASYNC;
    // SAFETY: pszsound is a valid PCWSTR to our owned null-terminated wide
    // string that outlives the call; hmod=None (no resource module); flags
    // request filename + async. SND_ASYNC returns immediately (the OS opens
    // the file by name during the call), so the buffer may drop after.
    let ok = unsafe { PlaySoundW(PCWSTR::from_raw(wide.as_ptr()), None, flags) };
    if !ok.as_bool() {
        tracing::warn!("sound file playback failed");
    }
}

/// Play the configured feedback: custom .wav if `custom_path` is set, else the
/// synthesized default tone. Fire-and-forget, best-effort. Caller gates on
/// `settings.overlay.sounds.enabled`.
pub fn play_configured(custom_path: &str, default: Tone) {
    if custom_path.is_empty() {
        play_tone(default);
    } else {
        play_sound_file(custom_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_samples_length_matches_duration() {
        let s = tone_samples(880.0, 120, 48000);
        assert_eq!(s.len(), 5760); // round(120/1000 * 48000)
    }

    #[test]
    fn tone_samples_starts_near_zero_and_fades() {
        let s = tone_samples(880.0, 120, 48000);
        let first = s[0].abs();
        let last = s[s.len() - 1].abs();
        let max = s.iter().cloned().fold(0.0f32, f32::max);
        assert!(
            first < max * 0.1,
            "first sample should be near zero: {first}"
        );
        assert!(last < max * 0.1, "last sample should be near zero: {last}");
        assert!(max > 0.2, "peak amplitude should be substantial: {max}");
    }

    #[test]
    fn tone_samples_never_clips() {
        let s = tone_samples(880.0, 120, 48000);
        for (i, &v) in s.iter().enumerate() {
            assert!((-0.35..=0.35).contains(&v), "sample {i} clips: {v}");
        }
    }

    #[test]
    fn tone_samples_frequency_roughly_correct() {
        let sr = 48000u32;
        let s = tone_samples(880.0, 120, sr);
        let fade = (sr as f32 / 200.0) as usize;
        let mid = &s[fade..s.len() - fade];
        let crossings = mid
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count();
        // 880Hz * 2 crossings/period * ~0.11s mid section ≈ 193.
        assert!(
            (170..=230).contains(&crossings),
            "zero-crossings out of range: {crossings}"
        );
    }

    #[test]
    fn tone_samples_handles_very_short_duration() {
        let s = tone_samples(880.0, 5, 48000); // n=240 < 2*fade=480 → clamp fires
        assert!(!s.is_empty());
        for (i, &v) in s.iter().enumerate() {
            assert!(v.is_finite(), "sample {i} is NaN/Inf");
            assert!((-0.35..=0.35).contains(&v), "sample {i} clips: {v}");
        }
    }
}
