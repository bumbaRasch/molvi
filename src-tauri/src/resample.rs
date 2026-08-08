use rubato::Resampler as _; // trait in scope for method resolution; no name bind
use rubato::{Fft, FixedSync, Indexing, audioadapter_buffers::direct::InterleavedSlice};

use crate::errors::{MolviError, Result};

/// Band-limited resampler (spec D7). No-op fast path when `in_rate == out_rate`.
/// One block of input may produce zero or multiple blocks of output; sub-block
/// remainders are buffered internally so callers can feed arbitrary lengths.
pub struct Resampler {
    channels: usize,
    inner: Option<Fft<f32>>, // None = no-op passthrough
    leftover: Vec<f32>,      // pending input frames (mono)
    out_buf: Vec<f32>,       // reusable output buffer (output_frames_max * channels)
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32, channels: usize) -> Result<Self> {
        if in_rate == out_rate {
            return Ok(Self {
                channels,
                inner: None,
                leftover: Vec::new(),
                out_buf: Vec::new(),
            });
        }
        // chunk_size = input frames per process_into_buffer call (fixed side).
        // 480 at 48k = 10ms; sub-frames buffer in `leftover`.
        let chunk = 480usize;
        let inner = Fft::<f32>::new(
            in_rate as usize,
            out_rate as usize,
            chunk,
            channels,
            FixedSync::Input,
        )
        .map_err(|e| MolviError::Audio(format!("rubato: {e}")))?;
        let out_buf = vec![0.0; inner.output_frames_max() * channels];
        Ok(Self {
            channels,
            inner: Some(inner),
            leftover: Vec::new(),
            out_buf,
        })
    }

    /// Test-only introspection: did the constructor detect equal rates?
    /// Production `process` reads `self.inner.is_none()` directly.
    #[cfg(test)]
    pub fn is_noop(&self) -> bool {
        self.inner.is_none()
    }

    pub fn process(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        if self.inner.is_none() {
            return Ok(input.to_vec()); // 16k passthrough
        }
        let ch = self.channels;
        self.leftover.extend_from_slice(input);
        let mut out_all: Vec<f32> = Vec::new();
        let indexing = Indexing::new();
        loop {
            let needed = self.inner.as_ref().unwrap().input_frames_next();
            if self.leftover.len() < needed * ch {
                break;
            }
            let input_adapter = InterleavedSlice::new(&self.leftover[..needed * ch], ch, needed)
                .map_err(|e| MolviError::Audio(format!("rubato adapter: {e}")))?;
            let out_cap = self.out_buf.len() / ch;
            let mut output_adapter = InterleavedSlice::new_mut(&mut self.out_buf, ch, out_cap)
                .map_err(|e| MolviError::Audio(format!("rubato adapter: {e}")))?;
            let (_, written) = self
                .inner
                .as_mut()
                .unwrap()
                .process_into_buffer(&input_adapter, &mut output_adapter, Some(&indexing))
                .map_err(|e| MolviError::Audio(format!("rubato process: {e}")))?;
            out_all.extend_from_slice(&self.out_buf[..written * ch]);
            self.leftover.drain(..needed * ch);
        }
        Ok(out_all)
    }

    /// Reset internal state so the next session starts from a clean baseline.
    /// Without this, the previous session's FFT overlap-add tail (~10ms of
    /// attenuated audio @48k→16k) bleeds into the next recording's first output
    /// frames. Called on `EngineCmd::Start`. rubato's `Resampler::reset` zeros
    /// the overlap buffers (synchro.rs:664); `leftover` is cleared too (it is
    /// normally empty after a clean finalize/flush, but a Cancel may leave some).
    /// No-op on the 16k passthrough path (`inner` is None).
    pub fn reset(&mut self) {
        if let Some(r) = self.inner.as_mut() {
            r.reset();
        }
        self.leftover.clear();
    }

    /// Flush any pending `leftover` (< one input chunk) at session end by
    /// zero-padding to a full chunk and processing once. Called by the engine
    /// worker's Finalize path so the trailing sub-chunk isn't dropped.
    ///
    /// ponytail: one zero-padded chunk flushes the pending input (<10ms @48k).
    /// The Fft sync resampler's filter delay tail (~1-2 output ms) stays
    /// un-emitted without further zero input; acceptable vs the 30-60ms tail-loss
    /// this recovers. Loop-feeding zeros until dry would recover it but risks a
    /// live loop for negligible gain.
    pub fn flush(&mut self) -> Result<Vec<f32>> {
        if self.inner.is_none() || self.leftover.is_empty() {
            return Ok(Vec::new());
        }
        let ch = self.channels;
        let needed = self.inner.as_ref().unwrap().input_frames_next();
        self.leftover.resize(needed * ch, 0.0);
        let indexing = Indexing::new();
        let input_adapter = InterleavedSlice::new(&self.leftover, ch, needed)
            .map_err(|e| MolviError::Audio(format!("rubato adapter: {e}")))?;
        let out_cap = self.out_buf.len() / ch;
        let mut output_adapter = InterleavedSlice::new_mut(&mut self.out_buf, ch, out_cap)
            .map_err(|e| MolviError::Audio(format!("rubato adapter: {e}")))?;
        let (_, written) = self
            .inner
            .as_mut()
            .unwrap()
            .process_into_buffer(&input_adapter, &mut output_adapter, Some(&indexing))
            .map_err(|e| MolviError::Audio(format!("rubato flush: {e}")))?;
        self.leftover.clear();
        Ok(self.out_buf[..written * ch].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_rates_equal() {
        let mut r = Resampler::new(16000, 16000, 1).unwrap();
        assert!(r.is_noop());
        let out = r.process(&[0.5; 480]).unwrap();
        assert_eq!(out.len(), 480);
    }

    #[test]
    fn downsamples_48k_to_16k_ratio() {
        // 48000 -> 16000 is a 3:1 downsample. Feed 1 second of 1 kHz sine.
        let mut r = Resampler::new(48000, 16000, 1).unwrap();
        let n = 48000;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48000.0).sin() * 0.5)
            .collect();
        let out = r.process(&input).unwrap();
        // Allow a few samples of rubato latency slop.
        let expected = 16000f64;
        let actual = out.len() as f64;
        assert!(
            (actual - expected).abs() < 64.0,
            "expected ~{expected} got {actual}"
        );

        // Anti-aliasing sanity: output amplitude stays within input envelope.
        let peak = out.iter().cloned().fold(0.0f32, f32::max).abs();
        assert!(peak > 0.3 && peak < 0.7, "unexpected peak {peak}");
    }
}
