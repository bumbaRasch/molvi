//! Deterministic command-mode grammar → enigo `KeyChord` (spec §6.2/§6.5.4).
//!
//! When `settings.recognition_mode == Command`, the finalized transcript is
//! matched against a flat, data-driven, locale-agnostic phrase table; on an
//! exact whole-text match the mapped key-chord is emitted and paste/post-proc/
//! history are skipped. A no-match falls through to the normal paste path.
//!
//! Privacy §10.1: `parse` is pure and emits NO logs (it never sees a target /
//! foreground; the chord is delivered by `paste::run_command_chord`, whose log
//! is a fixed content-free string).

use enigo::Key;

/// A simulated key combination. `keys` are clicked in order; when `hold_ctrl`
/// is true the platform command modifier is held (Press) before and released
/// after the clicks — Ctrl on Windows/Linux, ⌘ on macOS (routed via
/// `paste::paste_modifier()` in `run_command_chord`). The field name `hold_ctrl`
/// is kept for stability; semantically it means "hold the platform command
/// modifier". `Clone, Debug` only — `Vec` is not `Copy` (DECISION 1).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyChord {
    pub keys: Vec<Key>,
    pub hold_ctrl: bool,
}

/// Resolve a command chord's letter key per platform. Windows + macOS use the
/// platform VIRTUAL key (layout-robust: Ctrl/⌘-shortcuts ignore the active
/// layout); Linux/X11 uses `Key::Unicode` (XKB keysym via the layout). macOS
/// uses the QWERTY `kVK_ANSI_*` codes — NOT `Key::Unicode`, which under a
/// non-QWERTY layout would map to the wrong physical key (AZERTY 'z'→keycode
/// 0x10 → ⌘Y redo instead of ⌘Z undo). `const` so the static PHRASES table can
/// call it.
pub(crate) const fn letter_key(c: char) -> Key {
    #[cfg(target_os = "windows")]
    {
        // Win32 VK: A=0x41..Z=0x5A = (lowercase ascii) - 0x20.
        let vk = if c.is_ascii_lowercase() {
            (c as u32) - 0x20
        } else {
            c as u32
        };
        Key::Other(vk)
    }
    #[cfg(target_os = "macos")]
    {
        // macOS QWERTY virtual key codes (HIToolbox kVK_ANSI_*).
        let vk = match c {
            'a' | 'A' => 0x00,
            'c' | 'C' => 0x08,
            'v' | 'V' => 0x09,
            'x' | 'X' => 0x07,
            'y' | 'Y' => 0x10,
            'z' | 'Z' => 0x06,
            // Only a/c/v/x/y/z command letters are used (PHRASES + paste
            // select-all). A non-letter here is a programmer error → fail
            // loudly (compile-time in the static table, runtime otherwise).
            // panic! not unreachable!: unreachable!(msg) expands to a formatted
            // panic, which is non-const (E0015) in this const fn.
            _ => panic!("letter_key: only a/c/v/x/y/z command letters are supported"),
        };
        Key::Other(vk)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Key::Unicode(c)
    }
}

/// Normalize `input` for table lookup (DECISION 4):
/// 1. `to_lowercase` (Unicode-aware: Ё→ё, Ä→ä, É→é),
/// 2. fold Russian `ё`→`е` (the one well-known ASR instability fold),
/// 3. trim + collapse internal whitespace runs to a single space,
/// 4. strip ONE trailing sentence-final char (`.`, `!`, `?`) — ASR often
///    appends a period.
// ponytail: no regex — stdlib `split_whitespace().join(' ')` does trim+collapse
// in one pass over ~50 entries at finalize time (human-paced, negligible).
// Diacritics (é/ü/ñ) are NOT folded: Nemotron emits them correctly for FR/DE/ES.
// If smoke shows poor FR/DE/ES match rates, folding via char::decompose / a
// small accent-fold map is the recall knob (add it here, table stays as-is).
fn normalize(input: &str) -> String {
    let lowered = input.to_lowercase();
    let folded = lowered.replace('ё', "е");
    // ponytail: split_whitespace().join(' ') trims AND collapses runs in one
    // pass — same result as the 2-step trim+collapse, no regex.
    let collapsed = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = collapsed;
    // Strip ONE trailing sentence-final char (ASR often appends a period).
    if matches!(out.chars().last(), Some('.' | '!' | '?')) {
        out.pop();
    }
    out
}

/// Parse a finalized transcript against the phrase table; return the first
/// whole-text match's chord, or `None`. There can be only one match (exact
/// match over a flat table). Logs nothing (privacy §10.1).
// ponytail: linear scan over a ~50-entry static slice — exact match needs no
// HashMap/regex. The chord `Vec` is allocated only on match (rare, human-paced).
pub fn parse(text: &str) -> Option<KeyChord> {
    let norm = normalize(text);
    for (phrase, hold_ctrl, keys) in PHRASES {
        if *phrase == norm {
            return Some(KeyChord {
                keys: keys.to_vec(),
                hold_ctrl: *hold_ctrl,
            });
        }
    }
    None
}

// Static grammar: (phrase, hold_ctrl, static key slice). Phrases are stored
// PRE-normalized (lowercase, single internal spaces, ё already folded, no
// trailing punctuation). Adding a language or phrase = adding rows. Bare keys
// (Enter/Tab/Backspace) use the named `Key` variants; Ctrl/⌘+letter uses
// `letter_key(c)` — `Key::Unicode('z')` would type the literal char and NOT
// combine with the held modifier on Windows (see paste.rs Ctrl+V note), and on
// macOS would mis-key under non-QWERTY layouts (see `letter_key` doc).
//
// ponytail: `capital`/`lowercase` (spec §6.2) are DEFERRED — there is no
// universal directional case-change chord on Windows (Shift+F3 is toggle-only
// + Word-specific). A clipboard-transform action variant is the upgrade path;
// the data-driven table means adding them later = new rows + one action kind,
// not a rewrite.
static PHRASES: &[(&str, bool, &[Key])] = &[
    // ── new line / Enter (Key::Return, no modifier) ──
    ("new line", false, &[Key::Return]),
    ("newline", false, &[Key::Return]),
    ("neue zeile", false, &[Key::Return]),
    ("nouvelle ligne", false, &[Key::Return]),
    ("новая строка", false, &[Key::Return]),
    ("nueva línea", false, &[Key::Return]),
    // ── undo: Ctrl/⌘+Z ──
    ("undo", true, &[letter_key('z')]),
    ("rückgängig", true, &[letter_key('z')]),
    ("annuler", true, &[letter_key('z')]),
    ("отмена", true, &[letter_key('z')]),
    ("deshacer", true, &[letter_key('z')]),
    // ── redo: Ctrl/⌘+Y ──
    ("redo", true, &[letter_key('y')]),
    ("wiederherstellen", true, &[letter_key('y')]),
    ("rétablir", true, &[letter_key('y')]),
    ("повтор", true, &[letter_key('y')]),
    ("rehacer", true, &[letter_key('y')]),
    // ── select all: Ctrl/⌘+A ──
    ("select all", true, &[letter_key('a')]),
    ("alles markieren", true, &[letter_key('a')]),
    ("tout sélectionner", true, &[letter_key('a')]),
    ("выделить все", true, &[letter_key('a')]),
    ("seleccionar todo", true, &[letter_key('a')]),
    // ── delete last word: Ctrl/⌘+Backspace ──
    ("delete last word", true, &[Key::Backspace]),
    ("letztes wort löschen", true, &[Key::Backspace]),
    ("supprimer le dernier mot", true, &[Key::Backspace]),
    ("удалить последнее слово", true, &[Key::Backspace]),
    ("borrar última palabra", true, &[Key::Backspace]),
    // ── copy: Ctrl/⌘+C ──
    ("copy", true, &[letter_key('c')]),
    ("kopieren", true, &[letter_key('c')]),
    ("copier", true, &[letter_key('c')]),
    ("копировать", true, &[letter_key('c')]),
    ("copiar", true, &[letter_key('c')]),
    // ── paste: Ctrl/⌘+V ──
    ("paste", true, &[letter_key('v')]),
    ("einfügen", true, &[letter_key('v')]),
    ("coller", true, &[letter_key('v')]),
    ("вставить", true, &[letter_key('v')]),
    ("pegar", true, &[letter_key('v')]),
    // ── cut: Ctrl/⌘+X ──
    ("cut", true, &[letter_key('x')]),
    ("ausschneiden", true, &[letter_key('x')]),
    ("couper", true, &[letter_key('x')]),
    ("вырезать", true, &[letter_key('x')]),
    ("cortar", true, &[letter_key('x')]),
    // ── tab: Tab (no modifier) ──
    ("tab", false, &[Key::Tab]),
    ("tabulator", false, &[Key::Tab]),
    ("tabulation", false, &[Key::Tab]),
    ("таб", false, &[Key::Tab]),
    ("tabulador", false, &[Key::Tab]),
    // ── enter: Enter (Key::Return, no modifier) ──
    ("enter", false, &[Key::Return]),
    ("eingabe", false, &[Key::Return]),
    ("entrée", false, &[Key::Return]),
    ("ввод", false, &[Key::Return]),
    ("intro", false, &[Key::Return]),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(keys: &[Key], hold_ctrl: bool) -> KeyChord {
        KeyChord {
            keys: keys.to_vec(),
            hold_ctrl,
        }
    }

    fn enter_chord() -> KeyChord {
        ch(&[Key::Return], false)
    }
    fn undo_chord() -> KeyChord {
        ch(&[letter_key('z')], true)
    }
    fn redo_chord() -> KeyChord {
        ch(&[letter_key('y')], true)
    }
    fn selall_chord() -> KeyChord {
        ch(&[letter_key('a')], true)
    }
    fn delword_chord() -> KeyChord {
        ch(&[Key::Backspace], true)
    }
    fn copy_chord() -> KeyChord {
        ch(&[letter_key('c')], true)
    }
    fn paste_chord() -> KeyChord {
        ch(&[letter_key('v')], true)
    }
    fn cut_chord() -> KeyChord {
        ch(&[letter_key('x')], true)
    }
    fn tab_chord() -> KeyChord {
        ch(&[Key::Tab], false)
    }

    // ── one assertion per command family, all 5 languages ──

    #[test]
    fn new_line_all_languages() {
        let exp = enter_chord();
        for p in [
            "new line",
            "neue zeile",
            "nouvelle ligne",
            "новая строка",
            "nueva línea",
        ] {
            assert_eq!(parse(p), Some(exp.clone()), "phrase: {p}");
        }
    }

    #[test]
    fn newline_alias_maps_to_enter() {
        assert_eq!(parse("newline"), Some(enter_chord()));
    }

    #[test]
    fn undo_all_languages() {
        let exp = undo_chord();
        for p in ["undo", "rückgängig", "annuler", "отмена", "deshacer"] {
            assert_eq!(parse(p), Some(exp.clone()), "phrase: {p}");
        }
    }

    #[test]
    fn redo_all_languages() {
        let exp = redo_chord();
        for p in ["redo", "wiederherstellen", "rétablir", "повтор", "rehacer"] {
            assert_eq!(parse(p), Some(exp.clone()), "phrase: {p}");
        }
    }

    #[test]
    fn select_all_all_languages() {
        let exp = selall_chord();
        for p in [
            "select all",
            "alles markieren",
            "tout sélectionner",
            "выделить все",
            "seleccionar todo",
        ] {
            assert_eq!(parse(p), Some(exp.clone()), "phrase: {p}");
        }
    }

    #[test]
    fn delete_last_word_all_languages() {
        let exp = delword_chord();
        for p in [
            "delete last word",
            "letztes wort löschen",
            "supprimer le dernier mot",
            "удалить последнее слово",
            "borrar última palabra",
        ] {
            assert_eq!(parse(p), Some(exp.clone()), "phrase: {p}");
        }
    }

    #[test]
    fn copy_all_languages() {
        let exp = copy_chord();
        for p in ["copy", "kopieren", "copier", "копировать", "copiar"] {
            assert_eq!(parse(p), Some(exp.clone()), "phrase: {p}");
        }
    }

    #[test]
    fn paste_all_languages() {
        let exp = paste_chord();
        for p in ["paste", "einfügen", "coller", "вставить", "pegar"] {
            assert_eq!(parse(p), Some(exp.clone()), "phrase: {p}");
        }
    }

    #[test]
    fn cut_all_languages() {
        let exp = cut_chord();
        for p in ["cut", "ausschneiden", "couper", "вырезать", "cortar"] {
            assert_eq!(parse(p), Some(exp.clone()), "phrase: {p}");
        }
    }

    #[test]
    fn tab_all_languages() {
        let exp = tab_chord();
        for p in ["tab", "tabulator", "tabulation", "таб", "tabulador"] {
            assert_eq!(parse(p), Some(exp.clone()), "phrase: {p}");
        }
    }

    #[test]
    fn enter_all_languages() {
        let exp = enter_chord();
        for p in ["enter", "eingabe", "entrée", "ввод", "intro"] {
            assert_eq!(parse(p), Some(exp.clone()), "phrase: {p}");
        }
    }

    // ── normalization behavior ──

    #[test]
    fn trailing_sentence_punct_stripped() {
        assert_eq!(parse("undo."), Some(undo_chord()));
        assert_eq!(parse("copy!"), Some(copy_chord()));
        assert_eq!(parse("paste?"), Some(paste_chord()));
        assert_eq!(parse("отмена."), Some(undo_chord()));
    }

    #[test]
    fn only_one_trailing_punct_stripped() {
        // "undo.." → "undo." after one strip → no match. ASR rarely emits two.
        assert_eq!(parse("undo.."), None);
    }

    #[test]
    fn yo_fold_select_all_ru() {
        // Table stores ё-folded "выделить все"; both ё and е inputs match.
        assert_eq!(parse("выделить всё"), Some(selall_chord()));
        assert_eq!(parse("выделить все"), Some(selall_chord()));
        // Uppercase Ё → lowercase ё → fold to е.
        assert_eq!(parse("ВЫДЕЛИТЬ ВСЁ"), Some(selall_chord()));
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(parse("UNDO"), Some(undo_chord()));
        assert_eq!(parse("Copy"), Some(copy_chord()));
        assert_eq!(parse("SELECT ALL"), Some(selall_chord()));
        assert_eq!(parse("RÜCKGÄNGIG"), Some(undo_chord())); // Ä → ä
        assert_eq!(parse("RÉTABLIR"), Some(redo_chord())); // É → é
        assert_eq!(parse("КОПИРОВАТЬ"), Some(copy_chord()));
    }

    #[test]
    fn whitespace_collapsed() {
        assert_eq!(parse("select  all"), Some(selall_chord())); // double internal
        assert_eq!(parse("  undo  "), Some(undo_chord())); // leading/trailing
        assert_eq!(parse("delete\tlast\tword"), Some(delword_chord())); // tabs
        assert_eq!(parse("новая   строка"), Some(enter_chord())); // multi-space RU
    }

    #[test]
    fn no_match_returns_none() {
        assert_eq!(parse("hello world"), None);
        assert_eq!(parse("удалить"), None); // partial — delete-word is 3 words
        assert_eq!(parse(""), None);
        assert_eq!(parse("   "), None);
        assert_eq!(parse("copy paste"), None); // two actions concatenated
        assert_eq!(parse("undone"), None); // near but not exact
    }

    // ── confusable pairs (DATA-SAFETY): copy≠cut, undo≠redo in every lang ──

    #[test]
    fn copy_and_cut_distinct_all_languages() {
        let pairs = [
            ("copy", "cut"),
            ("копировать", "вырезать"),
            ("kopieren", "ausschneiden"),
            ("copier", "couper"),
            ("copiar", "cortar"),
        ];
        for (c, x) in pairs {
            let cc = parse(c).unwrap_or_else(|| panic!("copy phrase {c} unmatched"));
            let xc = parse(x).unwrap_or_else(|| panic!("cut phrase {x} unmatched"));
            assert_ne!(cc, xc, "{c} must map to a different chord than {x}");
            assert_eq!(cc, copy_chord(), "copy phrase {c}");
            assert_eq!(xc, cut_chord(), "cut phrase {x}");
        }
    }

    #[test]
    fn undo_and_redo_distinct_all_languages() {
        let pairs = [
            ("undo", "redo"),
            ("отмена", "повтор"),
            ("rückgängig", "wiederherstellen"),
            ("annuler", "rétablir"),
            ("deshacer", "rehacer"),
        ];
        for (u, r) in pairs {
            let uc = parse(u).unwrap_or_else(|| panic!("undo phrase {u} unmatched"));
            let rc = parse(r).unwrap_or_else(|| panic!("redo phrase {r} unmatched"));
            assert_ne!(uc, rc, "{u} must map to a different chord than {r}");
            assert_eq!(uc, undo_chord(), "undo phrase {u}");
            assert_eq!(rc, redo_chord(), "redo phrase {r}");
        }
    }

    #[test]
    fn normalize_unit_examples() {
        assert_eq!(normalize("UNDO"), "undo");
        assert_eq!(normalize("выделить всё"), "выделить все");
        assert_eq!(normalize("select  all"), "select all");
        assert_eq!(normalize("delete\tlast word"), "delete last word");
        assert_eq!(normalize("undo."), "undo");
        assert_eq!(normalize("  "), "");
        assert_eq!(normalize(""), "");
    }
}
