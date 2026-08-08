# Clips

Short **16 kHz mono** WAV clips, each paired with a reference transcript
`<name>.txt` (same basename). The spike scans this dir for `*.wav` and pairs
each with its `.txt`; the language is inferred from the basename.

The checked-in set was generated on the dev machine (SAPI TTS where no real
audio was handy), so the spike is reproducible end-to-end without a recording
step:

| File     | Source                                   | Reference                          |
|----------|------------------------------------------|------------------------------------|
| `ru.wav` | real speech — copy of `molvi-task0/.../ru/example.wav` (Pushkin excerpt, 11.3 s) | `ru.txt` (golden transcript) |
| `en.wav` | Windows SAPI `Microsoft David Desktop` (TTS) | `en.txt`                      |
| `de.wav` | Windows SAPI `Microsoft Hedda Desktop` (TTS) | `de.txt`                      |

`es`/`fr` are omitted (no SAPI voice for them was installed). The model's RTF
is language-independent (fixed compute per 560 ms chunk), so three clips across
en/de/ru is enough signal; add your own `*.wav`/`*.txt` pairs any time.

Recording tips (if you replace the TTS clips with real audio):

- **16 kHz, mono, 16-bit PCM WAV** (or 32-bit float).
  `ffmpeg -i in.m4a -ar 16000 -ac 1 out.wav`.
- One short phrase per clip (5-15 s). Quiet room, single speaker.
- Edit the matching `.txt` to match what you actually say.

If no `*.wav` files are present when the spike runs, it prints a clear message
and exits 0 (it does not crash) — so you can build/verify before adding clips.
