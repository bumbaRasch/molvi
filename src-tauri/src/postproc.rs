//! Post-processing pipeline (spec §8): the Raw/Smart/Polished transform run on
//! the finalize side-thread between transcription and paste.
//!
//! - Raw: no-op.
//! - Smart: deterministic pipeline of pure `fn(&str) -> String` steps, gated by
//!   `SmartToggles`, composed in the spec §8.2 order.
//! - Polished: HTTP POST to an OpenAI-compatible `/chat/completions` endpoint
//!   via ureq; on failure we fall back to the raw transcript (never lost).
//!
//! Privacy (HARD RULE, spec §10.1): transcript text is never logged. The
//! Polished request body contains the transcript — it is never traced. Error
//! strings surface only HTTP/parse/endpoint metadata, never the text.

use crate::dictionary::Dictionary;
use crate::settings::{PostMode, PostProcessing, SmartToggles};
use crate::snippets::Snippets;

/// Public outcome of `run`. `Used` carries the text to paste; `Failed` carries
/// a metadata-only error string and the raw transcript to paste as fallback.
pub enum PostOutcome {
    Used(String),
    Failed(String /*err*/, String /*raw fallback*/),
}

/// Default system prompt for Polished mode when the user sets none.
const MOLVI_DEFAULT_PROMPT: &str = "Fix punctuation, capitalization, and obvious grammar in the user's dictated text. Preserve the original meaning, language, and any proper nouns or custom terms exactly as written. Do not rephrase, translate, or change the wording or style \u{2014} only correct surface errors.";

/// Run post-processing. `dict` is applied by the Smart pipeline (between the
/// duplicate-word and case steps) when `smart.apply_dictionary` is set.
/// `snippets` runs as a Smart-only step (spec §6.3) positioned after
/// `apply_dictionary` and before `fix_case`: a whole-text cue match REPLACES
/// the text and short-circuits the remaining Smart steps. The snippets gate
/// (`settings.snippets_enabled`) is resolved to Option-Some ONCE at the
/// finalize closure (pipeline.rs), so `Some(snippets)` here means both
/// "enabled" and "store present". Raw/Polished ignore it. `backtrack` runs
/// as the FIRST Smart step (spec §1.4); Raw/Polished ignore it.
pub fn run(
    text: &str,
    settings: &PostProcessing,
    dict: Option<&Dictionary>,
    snippets: Option<&Snippets>,
    backtrack: bool,
) -> PostOutcome {
    match settings.mode {
        PostMode::Raw => PostOutcome::Used(text.to_string()),
        PostMode::Smart => PostOutcome::Used(smart_pipeline(
            text,
            &settings.smart,
            dict,
            snippets,
            backtrack,
        )),
        PostMode::Polished => match polished(text, settings) {
            Ok(out) => PostOutcome::Used(out),
            Err(e) => PostOutcome::Failed(e, text.to_string()),
        },
    }
}

/// Compose the Smart steps in spec §8.2 order, each gated by its toggle.
fn smart_pipeline(
    text: &str,
    smart: &SmartToggles,
    dict: Option<&Dictionary>,
    snippets: Option<&Snippets>,
    backtrack: bool,
) -> String {
    let mut t = text.to_string();
    if backtrack {
        t = smart_step_backtrack(&t);
    }
    if smart.merge_chunks {
        t = smart_step_merge(&t);
    }
    if smart.inter_chunk_punctuation {
        t = smart_step_inter_chunk_punct(&t);
    }
    if smart.cleanup_repeated_marks {
        t = smart_step_repeated_marks(&t);
    }
    if smart.remove_duplicate_words {
        t = smart_step_dup_words(&t);
    }
    if smart.apply_dictionary
        && let Some(d) = dict
    {
        t = d.apply(&t);
    }
    // Snippet expand (spec §6.3): whole-text cue match AFTER apply_dictionary,
    // BEFORE fix_case. On match the expansion is FINAL — skip fix_case/fillers/
    // ws (the stored block is authored verbatim). `Some(snippets)` already
    // implies `snippets_enabled` (the gate is resolved at the finalize closure,
    // pipeline.rs — one param carries both). Privacy §10.1: cue/expansion are
    // user content; this block logs nothing.
    if let Some(s) = snippets
        && let Some(expansion) = s.expand(&t)
    {
        return expansion;
    }
    if smart.fix_case {
        t = smart_step_case(&t);
    }
    if smart.remove_fillers {
        t = smart_step_fillers(&t);
    }
    if smart.normalize_whitespace {
        t = smart_step_ws(&t);
    }
    t
}

/// Backtrack (spec §1.4): when the user self-corrects via `...`, `…`, or the
/// phrase "no wait", discard everything before the marker and keep the
/// correction. Multi-pass: "a… b… c" → "c". Pure text transform; logs nothing
/// (privacy §10.1). Gated by `settings.backtrack_parsing` and run as the FIRST
/// Smart step (before `merge_chunks`) so the correction flows through the rest
/// of Smart.
fn smart_step_backtrack(text: &str) -> String {
    // ponytail: Regex compiled per call; the finalize side-thread runs this
    // ~once per session so the compile cost is negligible. Cache via
    // std::sync::LazyLock if profiling ever flags it.
    let re = regex::Regex::new(r"(?is)^(.*?)\s*(?:\.\.\.|…|,?\s*no wait,?)\s+(.*)$")
        .expect("backtrack regex compiles");
    let mut t = text.to_string();
    // Empty-correction guard (load-bearing): a trailing separator with no
    // actual correction must NOT collapse to "" (would paste silence). The
    // `\s+` before group 2 is greedy, so group 2 is either empty OR starts at
    // a non-whitespace char — `is_empty()` suffices (no `trim()` needed).
    while let Some(c) = re.captures(&t)
        && let Some(g2) = c.get(2)
        && !g2.as_str().is_empty()
    {
        t = g2.as_str().to_string();
    }
    t
}

/// Flatten any whitespace left at a finalized-chunk join (newlines/tabs/multi-
/// space) to single spaces. Sentence/punctuation spacing is finalized by `ws`.
fn smart_step_merge(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// ponytail: true inter-chunk sentence punctuation needs VAD-gap info, which
/// isn't passed to `run` yet (Phase-1 hands a single joined string). Conservatively
/// dedupe doubled mid-sentence marks (stray `,;:` from chunk boundaries).
/// Sentence-final marks are handled by `repeated_marks`; capitalization by `case`.
fn smart_step_inter_chunk_punct(text: &str) -> String {
    // ponytail: Regex compiled per call; the finalize side-thread runs this
    // ~once per session so the compile cost is negligible. Cache via
    // std::sync::LazyLock if profiling ever flags it.
    let re = regex::Regex::new(r"[,;:]{2,}").expect("inter_chunk regex compiles");
    re.replace_all(text, |caps: &regex::Captures| {
        caps[0]
            .chars()
            .next()
            .map(|c| c.to_string())
            .unwrap_or_default()
    })
    .into_owned()
}

/// Collapse stray repeated punctuation: `??`/`???` → `?`, `!!` → `!`,
/// `..`/`...` → `…`, `--`/`---` → `—`.
fn smart_step_repeated_marks(text: &str) -> String {
    // ponytail: Regex compiled per call; the finalize side-thread runs this
    // ~once per session so the compile cost is negligible. Cache via
    // std::sync::LazyLock if profiling ever flags it.
    let re = regex::Regex::new(r"[?]{2,}|[!]{2,}|[.]{2,}|[-]{2,}")
        .expect("repeated_marks regex compiles");
    re.replace_all(text, |caps: &regex::Captures| -> String {
        match caps[0].chars().next().unwrap() {
            '?' => "?".into(),
            '!' => "!".into(),
            '.' => "…".into(),
            _ => "—".into(),
        }
    })
    .into_owned()
}

/// Drop accidental immediate double words (`я я`, `the the`). Case-insensitive,
/// compares the bare-letter portion so trailing punctuation doesn't hide a dup;
/// cascading (`да да да`) collapses in one pass via the last-kept pointer.
/// A sentence-terminal mark (`. ! ? …`) on the kept token RESETS the pointer,
/// so legitimate repetition across a sentence boundary (`привет. привет`) is
/// preserved. Dropping a dup also strips any orphan trailing sentence-internal
/// mark (`, ; :`) from the kept token (`да, да` → `да`, not `да,`).
fn smart_step_dup_words(text: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut last: Option<String> = None;
    for tok in text.split_whitespace() {
        let bare = bare_word(tok);
        let prev_ends_sentence = kept
            .last()
            .map(|p| p.chars().last().is_some_and(is_sentence_terminal))
            .unwrap_or(false);
        if !bare.is_empty() && !prev_ends_sentence && last.as_deref() == Some(bare.as_str()) {
            if let Some(prev) = kept.last_mut() {
                *prev = prev.trim_end_matches([',', ';', ':']);
            }
            continue;
        }
        kept.push(tok);
        last = Some(bare);
    }
    kept.join(" ")
}

fn bare_word(tok: &str) -> String {
    tok.trim_matches(|c: char| !c.is_alphabetic())
        .to_lowercase()
}

fn is_sentence_terminal(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '…')
}

/// Capitalize the first letter of each sentence (after `. ! ? …`). Unicode-aware
/// via `char::to_uppercase` (Cyrillic-safe). Already-capitalized proper nouns are
/// preserved. Abbreviations like "т.д." may over-capitalize — accepted (the
/// canonical sentence heuristic; disable `fix_case` to opt out). ALL-CAPS burst
/// fixing is intentionally skipped: distinguishing shouted speech from acronyms
/// (API, СССР) needs a term list; guessing risks corrupting proper nouns.
fn smart_step_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut new_sentence = true;
    for ch in text.chars() {
        if matches!(ch, '.' | '!' | '?' | '…') {
            out.push(ch);
            new_sentence = true;
        } else if ch.is_whitespace() {
            out.push(ch);
        } else if new_sentence {
            if ch.is_alphabetic() {
                for u in ch.to_uppercase() {
                    out.push(u);
                }
            } else {
                out.push(ch); // digit/punctuation ends the start-of-sentence window
            }
            new_sentence = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Strip the default RU filler list (`ээ`, `э-э`, `мм`, `ну`, `типа`, `короче`)
/// as whole words. OFF by default — aggressive: `короче` is also the comparative
/// "shorter", so opt-in only. Spacing is left to `ws`.
fn smart_step_fillers(text: &str) -> String {
    const FILLERS: &[&str] = &["ээ", "э-э", "мм", "ну", "типа", "короче"];
    let mut out: Vec<&str> = Vec::new();
    for tok in text.split_whitespace() {
        let bare = tok
            .trim_matches(|c: char| !c.is_alphabetic() && c != '-')
            .to_lowercase();
        if !FILLERS.contains(&bare.as_str()) {
            out.push(tok);
        }
    }
    out.join(" ")
}

/// Collapse whitespace runs, remove space before punctuation, ensure exactly one
/// space after punctuation when followed by a letter. Decimal commas (`3,14`)
/// are preserved (no space inserted before a digit).
fn smart_step_ws(text: &str) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    // ponytail: both Regexes below are compiled per call; the finalize
    // side-thread runs this ~once per session so the compile cost is negligible.
    // Cache via std::sync::LazyLock if profiling flags it.
    let before = regex::Regex::new(r" +([,.;:!?…])").expect("ws-before regex compiles");
    let no_sp_before = before.replace_all(&collapsed, "$1");
    let after = regex::Regex::new(r"([,.;:!?…])(\p{L})").expect("ws-after regex compiles");
    let with_sp_after = after.replace_all(&no_sp_before, "$1 $2");
    with_sp_after.trim().to_string()
}

// --- Polished (ureq) -------------------------------------------------------

/// Build the OpenAI-compatible `/chat/completions` JSON body (pure, testable).
fn build_polished_body(text: &str, settings: &PostProcessing) -> serde_json::Value {
    serde_json::json!({
        "model": settings.model.as_deref().unwrap_or(""),
        "messages": [
            {"role": "system", "content": settings.prompt.as_deref().unwrap_or(MOLVI_DEFAULT_PROMPT)},
            {"role": "user", "content": text},
        ],
        "temperature": 0.0,
    })
}

/// POST the transcript to an OpenAI-compatible endpoint and return the model's
/// content string. Errors are metadata-only (HTTP status / endpoint / parse) and
/// NEVER contain the transcript. Bearer auth is added when `api_key` is set.
fn polished(text: &str, settings: &PostProcessing) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(20)))
        .build()
        .into();
    let endpoint = settings
        .endpoint
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "no endpoint configured".to_string())?
        .trim_end_matches('/');
    let body = build_polished_body(text, settings);
    let url = format!("{endpoint}/chat/completions");
    let req = agent.post(&url);
    let req = if let Some(key) = settings.api_key.as_deref().filter(|s| !s.trim().is_empty()) {
        req.header("Authorization", format!("Bearer {key}"))
    } else {
        req
    };
    match req.send_json(&body) {
        Ok(mut r) => {
            let v: serde_json::Value = r
                .body_mut()
                .read_json()
                .map_err(|e| format!("parse response: {e}"))?;
            v["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "response missing choices[0].message.content".to_string())
        }
        Err(ureq::Error::StatusCode(c)) => Err(format!("endpoint returned HTTP {c}")),
        Err(ureq::Error::Timeout(_)) => Err("request timed out".into()),
        Err(ureq::Error::HostNotFound) => Err("endpoint host not found (DNS)".into()),
        Err(ureq::Error::ConnectionFailed) => {
            Err("endpoint unreachable (connection failed)".into())
        }
        Err(e) => Err(format!("{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_repeated_marks() {
        assert_eq!(smart_step_repeated_marks("что??? да..."), "что? да…");
    }

    #[test]
    fn remove_duplicate_words() {
        assert_eq!(
            smart_step_dup_words("я я пошёл пошёл домой"),
            "я пошёл домой"
        );
    }

    #[test]
    fn fix_case_sentence_start() {
        assert_eq!(smart_step_case("привет. как дела"), "Привет. Как дела");
    }

    #[test]
    fn normalize_whitespace() {
        assert_eq!(smart_step_ws("а   б ,в"), "а б, в");
    }

    #[test]
    fn merge_collapses_internal_whitespace_runs() {
        // Chunk joins may leave newlines/tabs/multi-space; merge flattens them.
        assert_eq!(smart_step_merge("а\nб\tв  г"), "а б в г");
    }

    #[test]
    fn inter_chunk_collapses_doubled_mid_marks() {
        assert_eq!(smart_step_inter_chunk_punct("а,, б;; в:: г"), "а, б; в: г");
    }

    #[test]
    fn repeated_marks_handles_runs_and_dashes() {
        assert_eq!(smart_step_repeated_marks("да!!!! нет??"), "да! нет?");
        assert_eq!(
            smart_step_repeated_marks("слово --- слово"),
            "слово — слово"
        );
    }

    #[test]
    fn dup_words_cascading_and_case_insensitive() {
        assert_eq!(smart_step_dup_words("да да да"), "да");
        assert_eq!(smart_step_dup_words("Я я"), "Я");
    }

    #[test]
    fn smart_step_dup_words_preserves_cross_sentence_repetition() {
        // A sentence-terminal mark on the kept token resets the dup pointer —
        // words across a boundary are neither accidental nor immediate.
        for term in [".", "!", "?", "…"] {
            let inp = format!("привет{term} привет, как дела");
            assert_eq!(
                smart_step_dup_words(&inp),
                inp,
                "failed for terminal {term:?}"
            );
        }
    }

    #[test]
    fn smart_step_dup_words_no_orphan_trailing_comma() {
        // Dropping a dup must not leave an orphan sentence-internal comma.
        assert_eq!(smart_step_dup_words("да, да пошёл"), "да пошёл");
        assert_eq!(smart_step_dup_words("привет, привет"), "привет");
    }

    #[test]
    fn case_preserves_already_capitalized_proper_nouns() {
        assert_eq!(smart_step_case("Москва. привет"), "Москва. Привет");
    }

    #[test]
    fn fillers_strips_default_ru_list_preserving_rest() {
        let out = smart_step_fillers("ну типа привет ээ");
        assert_eq!(out, "привет");
    }

    #[test]
    fn ws_preserves_decimal_comma() {
        assert_eq!(smart_step_ws("пи 3,14 равно"), "пи 3,14 равно");
    }

    #[test]
    fn pipeline_is_deterministic() {
        let s = PostProcessing::default();
        let a = run("тест. тест", &s, None, None, false);
        let b = run("тест. тест", &s, None, None, false);
        assert_eq!(a_text(&a), a_text(&b));
    }

    #[test]
    fn pipeline_is_idempotent() {
        let s = PostProcessing::default();
        // Fixture has no cross-sentence repetition — idempotence must hold on
        // legitimate input without relying on dup-step corruption to converge.
        let once = a_text(&run("привет.   как  дела...", &s, None, None, false));
        let twice = a_text(&run(&once, &s, None, None, false));
        assert_eq!(once, twice);
    }

    fn a_text(o: &PostOutcome) -> String {
        match o {
            PostOutcome::Used(t) | PostOutcome::Failed(_, t) => t.clone(),
        }
    }

    #[test]
    fn build_polished_body_shape_uses_default_prompt() {
        let settings = PostProcessing {
            model: Some("gpt-test".into()),
            ..PostProcessing::default()
        };
        let body = build_polished_body("сырой", &settings);
        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["temperature"], 0.0);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], MOLVI_DEFAULT_PROMPT);
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "сырой");
    }

    #[test]
    fn build_polished_body_uses_user_prompt_when_set() {
        let settings = PostProcessing {
            model: Some("m".into()),
            prompt: Some("мой промпт".into()),
            ..PostProcessing::default()
        };
        let body = build_polished_body("x", &settings);
        assert_eq!(body["messages"][0]["content"], "мой промпт");
    }

    #[test]
    fn polished_no_endpoint_returns_err() {
        let settings = PostProcessing::default(); // endpoint None
        assert!(polished("текст", &settings).is_err());
    }

    #[test]
    fn polished_ok_parses_openai_content() {
        // Tiny local server returning a canned OpenAI-style response.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}");
        let handle = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut stream, _) = listener.accept().unwrap();
            // Drain the full request first: responding before the client finishes
            // writing its body races and RSTs the socket (os error 10054).
            let mut req: Vec<u8> = Vec::with_capacity(1024);
            let mut buf = [0u8; 512];
            loop {
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
                let s = String::from_utf8_lossy(&req);
                if let Some(split) = s.find("\r\n\r\n") {
                    let cl = s[..split]
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    let body_seen = req.len() - (split + 4);
                    if body_seen >= cl {
                        break;
                    }
                }
            }
            let body = r#"{"choices":[{"message":{"content":"исправленный текст."}}]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Both);
        });
        let settings = PostProcessing {
            mode: PostMode::Polished,
            endpoint: Some(url),
            model: Some("x".into()),
            ..PostProcessing::default()
        };
        let out = polished("текст", &settings).expect("polished should succeed");
        assert_eq!(out, "исправленный текст.");
        handle.join().unwrap();
    }

    #[test]
    fn polished_connection_failure_maps_to_failed_with_raw_preserved() {
        // Bind then drop to obtain a port that refuses connections.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = format!("http://{addr}");
        let settings = PostProcessing {
            mode: PostMode::Polished,
            endpoint: Some(url),
            model: Some("x".into()),
            ..PostProcessing::default()
        };
        let outcome = run("сырой текст здесь", &settings, None, None, false);
        match outcome {
            PostOutcome::Failed(err, raw) => {
                assert_eq!(raw, "сырой текст здесь"); // raw transcript never lost
                assert!(
                    !err.contains("сырой") && !err.contains("текст"),
                    "error must not leak transcript: {err}"
                );
            }
            other => panic!("expected Failed, got Used({:?})", a_text(&other)),
        }
    }

    /// Snippet-expand Smart step (Task 8b): on a whole-text cue match the
    /// expansion short-circuits fix_case/fillers/ws (stored verbatim); on a
    /// non-match Smart continues normally; and `None` (gate off OR store
    /// absent) keeps the step unconsulted. The gate resolution lives at the
    /// finalize closure (pipeline.rs) — here `Some(store)` ⇒ enabled.
    #[test]
    fn smart_snippet_step_short_circuits_on_match() {
        let store = crate::snippets::Snippets::open_in_memory().unwrap();
        // cue "brb" lowercased; expansion carries mixed-case + trailing ws to
        // prove fix_case/ws were SKIPPED (the stored block is final).
        store.add("brb", "Be Right Back! ").unwrap();
        let smart = SmartToggles {
            fix_case: true,
            normalize_whitespace: true,
            ..SmartToggles::default()
        };
        let post = PostProcessing {
            mode: PostMode::Smart,
            smart,
            ..PostProcessing::default()
        };
        // Match → verbatim expansion (lowercase cue "brb", trailing ws kept,
        // no case/whitespace fix-up applied — proves short-circuit).
        let matched = run("brb", &post, None, Some(&store), false);
        assert_eq!(a_text(&matched), "Be Right Back! ");
        // Non-match → normal Smart output (case + ws applied to the literal).
        let miss = run("brb please", &post, None, Some(&store), false);
        assert_eq!(a_text(&miss), "Brb please");
        // Gate off (None) → Smart runs normally even with a cue that would
        // match; proves the Option-Some gate is load-bearing.
        let gated = run("brb", &post, None, None, false);
        assert_eq!(a_text(&gated), "Brb");
    }

    #[test]
    fn backtrack_three_dots() {
        assert_eq!(smart_step_backtrack("hello... world"), "world");
    }

    #[test]
    fn backtrack_unicode_ellipsis() {
        assert_eq!(smart_step_backtrack("привет… пока"), "пока");
    }

    #[test]
    fn backtrack_no_wait_phrase() {
        assert_eq!(smart_step_backtrack("I think no wait I know"), "I know");
    }

    #[test]
    fn backtrack_multipass_converges() {
        assert_eq!(smart_step_backtrack("one… two… three"), "three");
    }

    #[test]
    fn backtrack_multipass_keeps_carried_words_not_in_alternation() {
        // Locks decision (a): "actually" is NOT a regex alternation member —
        // the regex matches the ellipsis separator, and "actually" is carried
        // through. The regex stays verbatim.
        assert_eq!(
            smart_step_backtrack("X… actually Y… actually Z"),
            "actually Z"
        );
    }

    #[test]
    fn backtrack_no_match_passthrough() {
        assert_eq!(smart_step_backtrack("just normal text"), "just normal text");
    }

    #[test]
    fn backtrack_empty_correction_guard() {
        // Trailing separator + whitespace, no actual correction: must NOT
        // collapse to "" (would paste silence). The empty-correction guard is
        // load-bearing — proves it preserves the original.
        assert_eq!(smart_step_backtrack("hello... "), "hello... ");
    }

    #[test]
    fn backtrack_gate_load_bearing() {
        // Same input, default PostProcessing (Smart + all toggles default).
        // Off → merge/repeated-marks/case/ws run normally → "Hello… World".
        // On  → backtrack first ("world") then case → "World".
        // Asserting they differ proves the `backtrack` flag is load-bearing.
        let post = PostProcessing::default();
        let off = a_text(&run("hello... world", &post, None, None, false));
        let on = a_text(&run("hello... world", &post, None, None, true));
        assert_eq!(off, "Hello… World");
        assert_eq!(on, "World");
        assert_ne!(off, on);
    }
}
