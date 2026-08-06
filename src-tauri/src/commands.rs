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
/// is true `Key::Control` is held (Press) before and released after the clicks.
/// `Clone, Debug` only — `Vec` is not `Copy` (DECISION 1).
#[derive(Debug, Clone, PartialEq)]
pub struct KeyChord {
    pub keys: Vec<Key>,
    pub hold_ctrl: bool,
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
// (Enter/Tab/Backspace) use the named `Key` variants; Ctrl+letter uses
// `Key::Other(VK)` — `Key::Unicode('z')` would type the literal char and NOT
// combine with held Ctrl (see paste.rs:73-77). Win32 VK: A=0x41..Z=0x5A.
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
    // ── undo: Ctrl+Z (VK 0x5A) ──
    ("undo", true, &[Key::Other(0x5A)]),
    ("rückgängig", true, &[Key::Other(0x5A)]),
    ("annuler", true, &[Key::Other(0x5A)]),
    ("отмена", true, &[Key::Other(0x5A)]),
    ("deshacer", true, &[Key::Other(0x5A)]),
    // ── redo: Ctrl+Y (VK 0x59) ──
    ("redo", true, &[Key::Other(0x59)]),
    ("wiederherstellen", true, &[Key::Other(0x59)]),
    ("rétablir", true, &[Key::Other(0x59)]),
    ("повтор", true, &[Key::Other(0x59)]),
    ("rehacer", true, &[Key::Other(0x59)]),
    // ── select all: Ctrl+A (VK 0x41) ──
    ("select all", true, &[Key::Other(0x41)]),
    ("alles markieren", true, &[Key::Other(0x41)]),
    ("tout sélectionner", true, &[Key::Other(0x41)]),
    ("выделить все", true, &[Key::Other(0x41)]),
    ("seleccionar todo", true, &[Key::Other(0x41)]),
    // ── delete last word: Ctrl+Backspace ──
    ("delete last word", true, &[Key::Backspace]),
    ("letztes wort löschen", true, &[Key::Backspace]),
    ("supprimer le dernier mot", true, &[Key::Backspace]),
    ("удалить последнее слово", true, &[Key::Backspace]),
    ("borrar última palabra", true, &[Key::Backspace]),
    // ── copy: Ctrl+C (VK 0x43) ──
    ("copy", true, &[Key::Other(0x43)]),
    ("kopieren", true, &[Key::Other(0x43)]),
    ("copier", true, &[Key::Other(0x43)]),
    ("копировать", true, &[Key::Other(0x43)]),
    ("copiar", true, &[Key::Other(0x43)]),
    // ── paste: Ctrl+V (VK 0x56) ──
    ("paste", true, &[Key::Other(0x56)]),
    ("einfügen", true, &[Key::Other(0x56)]),
    ("coller", true, &[Key::Other(0x56)]),
    ("вставить", true, &[Key::Other(0x56)]),
    ("pegar", true, &[Key::Other(0x56)]),
    // ── cut: Ctrl+X (VK 0x58) ──
    ("cut", true, &[Key::Other(0x58)]),
    ("ausschneiden", true, &[Key::Other(0x58)]),
    ("couper", true, &[Key::Other(0x58)]),
    ("вырезать", true, &[Key::Other(0x58)]),
    ("cortar", true, &[Key::Other(0x58)]),
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
        ch(&[Key::Other(0x5A)], true)
    }
    fn redo_chord() -> KeyChord {
        ch(&[Key::Other(0x59)], true)
    }
    fn selall_chord() -> KeyChord {
        ch(&[Key::Other(0x41)], true)
    }
    fn delword_chord() -> KeyChord {
        ch(&[Key::Backspace], true)
    }
    fn copy_chord() -> KeyChord {
        ch(&[Key::Other(0x43)], true)
    }
    fn paste_chord() -> KeyChord {
        ch(&[Key::Other(0x56)], true)
    }
    fn cut_chord() -> KeyChord {
        ch(&[Key::Other(0x58)], true)
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
