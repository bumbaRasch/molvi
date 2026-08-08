# molvi Phase-3 UX Research

Scope: accent motions that move molvi from functional to delightful. Every
proposal is implementable in vanilla TS + Tauri 2, and respects the privacy
HARD RULE (no transcript text in logs; local DOM is fine). DB citations refer
to the `ui-ux-pro-max` database (styles / typography / ux / gsap / icons
domains).

---

## 1. Three visual directions

**A. "Studio Console" (recommended) — DB style: Modern Dark Cinema**
- Font: Inter, single family, weight carries hierarchy (DB pairing "Modern
  Dark Cinema (Inter System)" / "Minimal Swiss"); tabular-nums for timers.
- Palette: keep teal `#0E7C86` as the single accent; adopt the DB elevated
  surface ramp (`#020203` / `#050506` / `#0a0a0c`) for the settings shell;
  hairline borders `rgba(255,255,255,0.08)`.
- Rationale: continuity with the already-shipped dark overlay bubble
  (tray -> overlay -> settings one language), and it is the lineage of the
  tools molvi envies (Linear, Raycast). Voice work is craft; dark reads as
  precision.

**B. "Fluent Paper" — DB style: E-Ink/Paper + Bento Box Grid (light, native)**
- Font: Inter (DB "Minimal Swiss").
- Palette: off-white `#FDFBF7` surface, ink `#1A1A1A` text, teal reserved for
  active states only; 12px rounded cards (matches current overlay radius).
- Rationale: Windows 11 Mica/acrylic feel; lowest cognitive distance for the
  non-technical professional; reads as "calm local tool, not cloud SaaS."

**C. "Calm Instrument" — DB style: Micro-interactions pushed toward E-Ink calm**
- Font: Inter for UI; optional serif (Georgia/system) for the transcript
  preview only, to separate "your words" from "the chrome."
- Palette: desaturate teal toward `#4A8A90`, near-zero chroma elsewhere.
- Rationale: molvi is always-on and ambient; an instrument you forget is
  there. Minimal motion, maximum quiet = strongest privacy signal.

**Pick A.** It is the only direction continuous with the shipped dark overlay,
and it is where pro voice tools are going. B and C are fallbacks if user
testing rejects the dark shell.

---

## 2. Five UX guidelines (from the DB)

1. **Loading States (Animation cat., severity High)** — "skeleton/spinner for
   ops >300ms; never freeze." molvi: finalize -> post-proc -> paste regularly
   clears 300ms. Replace the static "Polishing..." text with motion (breathing
   ring + shimmer) so the wait reads as active, not frozen.
2. **Error Recovery (Feedback cat., Medium)** — "clear next steps; never error
   without a recovery path." molvi: paste-failed dead-ends on "text is in the
   clipboard." Add two affordances: "Paste anyway" (re-attempt to current
   focus) and "Open history" (the row is already saved).
3. **Error Messages announced (Accessibility cat., High)** — "aria-live /
   role=alert for errors; never visual-only." molvi: the overlay caption is a
   plain div; promote paste-failed to `role="alert"` (the settings toaster
   already is; the overlay is not).
4. **User Freedom / Onboarding (Onboarding cat., Medium)** — "Skip + Back
   always; never force a linear unskippable tour." molvi: the 3-step first-run
   must be skippable from step 1, settings reachable throughout.
5. **Keyboard Navigation + Focus States (Accessibility + Interaction, both
   High)** — "tab order matches visual order; visible focus rings; never
   outline-none without replacement." molvi: the overlay's cancel button and
   every settings control need visible focus rings; the future command palette
   must be fully keyboard-driven.

---

## 3. Overlay redesign

Today the overlay has two visible states that both feel broken: RECORDING
(caption empty -- no partials yet -- so the user stares at a blank line and a
red dot), and POLISHING (a blue dot spins, caption says "Polishing..."). The
red->blue jump reads as error->loading, not a calm flow. Phase-3 should make
the overlay a continuous confidence-building motion: a single teal element
that *breathes* while listening, becomes a soft *ring* while polishing, and
flashes a *check* on success -- then make the short finalize window useful by
offering an inline edit affordance before the paste fires.

Key moves:
- **One accent, three phases.** Teal dot breathes (listening) -> teal ring
  sweeps (polishing, shimmer on the caption line per DB Loading/Skeleton) ->
  teal check, 400ms, hide. Drop the red/blue swap entirely.
- **Latency masking.** Breathing dot (scale 1->1.15, sine.inOut 1.6s) + live
  waveform make the 300-800ms finalize feel active. DB rule: transform/opacity
  only -> stays on the compositor.
- **Inline edit-before-paste (SuperWhisper review parity).** During polishing,
  a small "edit" affordance appears; clicking pauses auto-paste, the caption
  becomes `contenteditable`, Enter pastes, Esc cancels. Privacy-safe: local
  DOM only, cleared on hide (overlay already clears caption on `hide-overlay`
  per spec 10.1).
- **Streaming-ready.** When Phase-3 adds partials, the caption grows with the
  caret (the `.caption::after` caret already exists in `overlay.css:57`) -- no
  overlay rewrite needed.
- **Confidence tint (gated).** Only if the engine exposes per-token
  confidence: underline low-confidence words in amber. Do not fake it until
  then.
- **Paste-failed = recovery, not dead-end.** "Text saved -- Paste anyway |
  Open history" (guideline #2).

ASCII (polishing + edit state, ~700px, bottom-center):

```
+---------------------------------------------------------------+
|                                                               |
|   "let's ship the onboarding change on Thursday"|              |
|    ------ shimmer sweep (sine.inOut 1.4s) ------              |
|                                                               |
|   (o)   [ edit ]   paste in 0.4s   0:03               [x]     |
|   '''                                                          |
+---------------------------------------------------------------+
        ^teal ring    ^tabular-nums timer        ^cancel
```

---

## 4. Onboarding (3 steps, skippable)

Benchmark lesson: Raycast and Arc teach by *doing* (you press the key, you
don't read about it); Notion uses one CTA + benefit bullets; SuperWhisper
gates on the model download and teaches review-mode in the first session.
molvi's differentiator is privacy -- lead with it, prove it in step 3.

**Step 1 -- Welcome + model fetch.**
- Headline: "Welcome to molvi."
- Sub: "Dictation that never leaves this machine. Everything runs on your CPU
  -- no cloud, no account."
- Body: model download progress (~2.6 GB Nemotron / GigaAM weight) with real
  byte bar + ETA, and one line: "What's downloading? The speech model. It's
  why nothing you say is sent anywhere."
- Controls: [ Continue ] [ Skip -- I'll set up later ]. Skip always visible
  (guideline #4). Auto-advance if the model is already present.

**Step 2 -- Hotkey + mic test (do, don't read).**
- Headline: "Press your key. Then say a word."
- Body: hotkey capture (default `Alt+` `, reassignable inline), then a
  2-second mic meter -- the breathing teal dot reacts to the user's voice.
  When the meter moves: "Hearing you."
- This is the Raycast move: the user performs the real action in onboarding,
  so it sticks.

**Step 3 -- First word.**
- Headline: "Say anything."
- Body: user holds the hotkey, speaks one phrase; molvi transcribes and shows
  the result in the inline-edit field from section 3 (so they learn review-mode
  for free). On confirm: "You're all set -- press Alt+` anywhere." A soft teal
  check (no confetti -- calm ethos, and it is the same check the overlay uses
  on every successful paste, reinforcing the language).
- [ Open settings ] [ Done ].

---

## 5. Settings IA upgrade

The 9-section sidebar is a table of contents; what it lacks is a way to
*jump*. Three moves, ranked:

- **Federated search box at the top of the sidebar (must-have).** One input
  that, as you type, filters the 9 sections by title/keywords AND surfaces
  matching history entries + dictionary pairs inline below (DB: Search /
  Autocomplete + Search / No-results -- show "No matches -- try 'hotkey',
  'language', or a word from your history"). This is also the keyboard user's
  skip-link (DB: Accessibility / Skip-links). Effort: one filter function over
  the existing section registry + the two IPC calls you already have
  (`history_query`, `dictionary_list`).
- **Inline `?` contextual help (cheap win).** Each `SettingsGroup` gains an
  optional `?` that toggles a one-line hint using the `Alert` component you
  already ship. Kills "buried help" without a docs site.
- **Command palette (stretch, Phase-3.1).** A `Ctrl+K` overlay (separate tiny
  webview, like the overlay window) running actions: "Start listening", "Open
  history", "Add dictionary entry: foo -> bar", "Switch recognition language:
  en-US". Full keyboard nav (guideline #5). Reuses the federated search index.
  This is the Linear/Raycast parity move -- defer until federated search proves
  the indexing model.

**Recommendation:** ship federated search + inline help in Phase-3; palette as
Phase-3.1.

---

## 6. History + Dictionary as first-class

**History.** Today: opt-in, debounced search, 50/page, repaste/delete, 80-char
cap, clear/erase. Three must-haves:
- **Full-text row expansion + filters.** Click a row -> expand inline to full
  text (lazy, per-row -- the 80-char cap at `history.ts:205` is a DOM heuristic,
  not a real limit). Add filter chips: by detected language (the `lang` field
  already flows engine -> pipeline -> history) and by date range. Also: fix the
  hardcoded `toLocaleString("ru-RU")` at `history.ts:201` -> use `ui_lang`; a
  real i18n bug hiding in the viewer.
- **Keyboard navigation.** j/k or arrows through rows, Enter to repaste, Del to
  delete -- matches the "pro tool" feel and satisfies guideline #5.
- **Bulk select + bulk delete.** Checkbox per row, shift-click range, one
  bulk-destroy confirm (reuse the existing `twoStepConfirm`).

**Dictionary.** Today: add/edit form, import/export, delete. Three must-haves:
- **Live filter.** Search box matching entry OR replacement (mirrors history
  search). One input, one filter.
- **Undo-delete toast (5s).** Deletes are instant and irreversible today; a
  5-second "Entry removed -- Undo" toast (the toaster supports action-less
  toasts today; add an optional action button) prevents the most common
  dictionary mistake.
- **Import preview.** Before applying a CSV import, show "N new, M conflicts
  (will overwrite)" -- one confirm screen, saves a round-trip and a surprise.

---

## 7. Brand direction

Keep teal `#0E7C86` -- it is distinctive in a category dominated by
blue/purple (Otter, Whisper apps) and red (Handy). The icon is the weak spot.

Proposal: a **waveform-m monogram** -- the lowercase "m" drawn so its three
vertical strokes are unequal-height equalizer bars (the classic 3-bar audio
meter), set in a rounded-square tile (radius matches the overlay bubble's
12px) filled teal, bars in white. It reads as "m for molvi" and "voice"
simultaneously, at 16px (tray) through 1024 (installer). This is the
Riverside/Descript lineage (audio brands own a wave motif) but differentiated
by being a letterform at the same time -- no major dictation app does both.

Secondary: drop any spinning-disc metaphor; the brand motion is the breathing
dot from section 3 -- one motion language across tray, overlay, and
onboarding-success.

Implementable now: one SVG, four rectangles (tile + 3 bars) inside a rounded
rect, shipped as `icon.png` / tray icon + 16px favicon. No font dependency.

```
   .----.
   |    |     <- tile, teal #0E7C86, radius 12
   | |)|     <- 3 unequal white bars = "m" + equalizer
   | || |       (middle bar tallest, reads as the 'm' apex)
   '----'
```

---

## 8. Top 10 Phase-3 UX wins, ranked by delight-per-effort

| # | Win | Effort | Delight | Notes |
|---|-----|--------|---------|-------|
| 1 | Overlay: breathing teal dot + shimmer polish phase (replace red pulse / blue spin) | S | 5 | Two CSS keyframe swaps; biggest perceived-quality jump per line of code. |
| 2 | Overlay: inline edit-before-paste (POLISHING state) | M | 5 | SuperWhisper review-mode parity; reuses contenteditable + existing clear-on-hide. |
| 3 | 3-step skippable onboarding (model fetch -> hotkey+mic -> first word) | M | 5 | Zero to one onboarding; privacy lead is the differentiator. |
| 4 | Overlay: paste-failed recovery ("Paste anyway" / "Open history") | S | 4 | Fixes a dead-end (guideline #2); one listener change in `overlay.ts`. |
| 5 | Settings: federated search box (sections + history + dictionary) | M | 4 | Also the keyboard skip-link (guideline #5); reuses existing IPC. |
| 6 | Dictionary: live filter + 5s undo-delete toast | S | 3 | Two small adds on existing list/toaster. |
| 7 | Brand: waveform-m monogram (one SVG) | S | 3 | Tray + installer + favicon; replaces generic icon. |
| 8 | History: full-text row expansion + lang/date filters + locale fix | M | 3 | `history.ts:201` locale bug fixed en route. |
| 9 | Settings: command palette (Ctrl+K) | L | 4 | Stretch / Phase-3.1; defer until federated search proves the index. |
| 10 | Overlay: confidence-based caption tint | L | 4 | Blocked on engine exposing per-token confidence; do not fake. |

**Phase-3 shipping order:** 1 -> 4 -> 7 -> 3 -> 2 -> 5 -> 6 -> 8 -> (9, 10 gated).
