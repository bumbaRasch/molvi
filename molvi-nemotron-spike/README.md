# molvi-nemotron-spike

Standalone measurement crate that answers: **can NVIDIA Nemotron-3.5-ASR
(streaming, 0.6 B, multilingual) run real-time on the molvi dev machine
(i5-12450H, CPU-only)?** It is **not** a molvi dependency — nothing under
molvi's `src-tauri/`, `src/`, or its `Cargo.toml` depends on this crate. It
exists only to produce the measurement that drives the Task 17 GO/Conditional/
NO-GO decision (spec §10.3).

## What it measures

For a single model load and each `*.wav` clip in `clips/`:

| metric           | how                                                                   |
|------------------|-----------------------------------------------------------------------|
| cold-load ms     | wall time of the first `Nemotron::from_pretrained` (ort session init) |
| warm-load ms     | wall time of a second `from_pretrained` (ort runtime already warm)    |
| per-utterance RTF| wall(audio) of the `transcribe_chunk` loop over 560 ms (8960-sample) chunks; WAV decode is outside the timer |
| peak RSS         | `GetProcessMemoryInfo` → `PeakWorkingSetSize` (bytes, read at end)    |
| CPU%             | process (kernel+user) / wall around the transcription loop (`GetProcessTimes`) |
| WER              | word-level Levenshtein (normalize → lowercase, strip punct, collapse ws) vs the matching `.txt` |

Verdict (one-line print + into the report), spec §10.3:

- **GO** — median RTF < 0.5
- **Conditional** — median RTF 0.5–1.0
- **NO-GO** — median RTF ≥ 1.0

## Findings (measured on the i5-12450H, 4P+4E, release build)

These are why the spike ships the config it ships — every knob below was tried
with find-docs verification (ctx7 `/altunenes/parakeet-rs` + `/pykeio/ort`) and
**measured**, not assumed.

| config | median RTF | verdict |
|---|---:|---|
| **parakeet defaults, all-cores** (shipped default) | **0.48–0.62** (best 0.482) | **GO / Conditional boundary** |
| P-core process affinity (`--affinity pcores`, mask 0xF00) | 0.79–0.93 | Conditional (worse) |
| + ort `GraphOptimizationLevel::Level3` | ≈ default RTF, cold-load ~doubled | dropped (net-negative) |

1. **P-core pinning HURTS Nemotron (~40% slower), the opposite of GigaAM.**
   molvi pins its whole process to P-cores (spec §11 / Task 5) because that helps
   transcribe-rs/GigaAM. Re-applying the same pin to Nemotron raised median RTF
   from ~0.55 to ~0.86 — parakeet-rs's intra-op pool + tokenization benefit from
   the full 4P+4E set, and restricting to 4 P-cores oversubscribes them (4 intra
   threads + inter + main on 4 cores). **Implication for Task 18: Nemotron must
   NOT inherit molvi's process-wide P-core affinity** — it needs all-cores, so
   the adapter must either skip affinity for Nemotron or make it engine-specific.
2. **ort Level3 graph optimization is RTF-neutral but ~doubles cold-load**
   (3.8 s → 5.8 s): the extra graph transforms cost one-time init for zero
   per-chunk gain. Dropped — parakeet's default optimization level stands.
3. **Verdict for the plan decision: GO/Conditional boundary.** Across all-cores
   runs the median RTF straddles 0.5 (0.48 best, 0.62 worst) → spec §10.3 reads
   this as **Conditional (borderline GO)**. Per spec D17, GO/Conditional both
   → proceed to Task 18; treat as Conditional (manual picker + "slower than
   GigaAM" warning). WER: en TTS 0.000, de TTS 0.083, ru real-speech 0.192.
4. Run-to-run variance is ±0.1 RTF on this shared dev box (background load); a
   quiet machine would tighten the band. The all-cores > pcores ordering was
   100% consistent across all runs, so the qualitative finding is solid.

## License

Nemotron-3.5-ASR ships under the **OpenMDW-1.1** license (the upstream model
card declares `license_name: openmdw-1.1`; link `https://openmdw.ai/license/1-1/`).
Full text in [`LICENSE.nemotron`](./LICENSE.nemotron). Terms that matter for molvi:

- **Commercial use: allowed** — "permission is hereby granted, free of charge,
  to deal in the Model Materials without restriction".
- **Redistribution of the model: allowed** provided you retain a copy of the
  agreement + all copyright/origin notices.
- **Outputs: unrestricted** — the agreement imposes nothing on model outputs.
- **No warranty** (standard AS-IS clause).
- **Attribution:** not explicitly required by the text, but the "retain …
  notices of origin" clause means redistributions must keep whatever notices
  NVIDIA ships with the model. Crediting NVIDIA in molvi's docs is the safe read.

Note: the brief's assumption of "NVIDIA Open Model License (NOML)" was wrong;
the actual license is OpenMDW-1.1. The model repos also have no root `LICENSE`
file — the text is published at openmdw.ai and copied verbatim into this repo.

## 1. Get the model

The model path is a **CLI arg — never a hardcoded URL**, and the model is
**never committed** (see `.gitignore`). Download once:

```powershell
# Primary export (parakeet-rs-native ONNX layout):
huggingface-cli download pantinor/nemotron-3.5-asr-streaming-0.6b-onnx `
    --local-dir ./nemotron
# or the parakeet-rs mirror (same files):
# huggingface-cli download altunenes/parakeet-rs --repo-type model `
#     --include "nemotron-3.5-asr-streaming-0.6b-onnx/*" --local-dir ./parakeet-mirror
```

The dir must contain: `encoder.onnx`, `encoder.onnx.data`, `decoder_joint.onnx`,
`tokenizer.model` (the multilingual 3.5 model layout that parakeet-rs loads).

Alternative (smaller/slower): `onnx-community/nemotron-3.5-asr-streaming-0.6b-onnx-int4`.

## 2. Add clips

See [`clips/README.md`](./clips/README.md). Drop 5–15 s, 16 kHz, mono WAVs
(`en.wav`, `es.wav`, `de.wav`, `fr.wav`, `ru.wav`) into `clips/` and edit the
matching `.txt` references to match what you actually say. If no `*.wav` is
present the spike prints a notice and exits 0 — it never crashes.

```powershell
# example: convert any source to 16 kHz mono WAV
ffmpeg -i myphrase.m4a -ar 16000 -ac 1 clips/en.wav
```

## 3. Run

```powershell
cd molvi-nemotron-spike
cargo run --release -- --model ./nemotron --clips clips
```

This prints per-clip metrics to stdout and writes:

- `report/<UTC-stamp>.md` — human-readable summary table + verdict
- `report/<UTC-stamp>.json` — structured per-clip metrics

The GO / Conditional / NO-GO verdict (Step 4 of the plan) is the **human's**
call, based on the report. This crate only generates the measurement.

## CLI

```
molvi-nemotron-spike --model <dir> [--clips <dir>] [--report-dir <dir>] [--affinity all|pcores]
```

| flag            | default   | purpose                                            |
|-----------------|-----------|----------------------------------------------------|
| `--model`       | required  | dir with the extracted ONNX model                  |
| `--clips`       | `./clips` | dir of `*.wav` + matching `.txt`                  |
| `--report-dir`  | `./report`| where `<UTC-stamp>.{md,json}` land                 |
| `--affinity`    | `all`     | `all` = OS-schedule every core (fastest, default). `pcores` = pin to P-cores (helps GigaAM, ~40% slower for Nemotron — see Findings) |
| `-h` / `--help` |           | usage                                              |

## Privacy

Per molvi §10.1 habit (the spike itself never touches molvi's
transcript/history paths): logs and reports carry **metadata only** — model
id, durations, RTF, RSS, CPU%, WER. The hypothesis text is reduced to a WER
number and dropped; it is never printed or persisted.

## Building / testing

```powershell
cargo build --manifest-path molvi-nemotron-spike/Cargo.toml
cargo clippy --manifest-path molvi-nemotron-spike/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path molvi-nemotron-spike/Cargo.toml
cargo test --manifest-path molvi-nemotron-spike/Cargo.toml   # runs the WER + UTC-stamp unit tests
```

`cargo run` needs the model + clips; the four commands above do not (the unit
tests are model-free).
