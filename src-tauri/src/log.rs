//! Tracing init. PRIVACY (spec §10.1): NEVER log transcript text, partial
//! transcripts, or audio samples — not even at trace. This module sets up the
//! appender + filter AND deliberately does NOT bridge the external `log` crate
//! (transcribe-rs logs transcript text via `log::info!`; the bridge would leak
//! it). `set_global_default` is used instead of `.init()` precisely to skip the
//! `tracing_log::LogTracer` auto-install — see tests/log_privacy.rs
//! `log_bridge_is_absent` for the runnable regression guard. If you change this
//! init path, that test MUST still pass; never substitute `.init()` /
//! `LogTracer::init()` here.

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use crate::errors::Result;
use crate::paths;

/// Daily-rotated logs older than this are deleted on startup. Ponytail: 14 days
/// for a local single-user app — bounds disk use, still leaves ~2 weeks to
/// debug "a few days ago". tracing-appender 0.2 has NO native retention option
/// (verified via ctx7 + docs.rs 0.2.3: `Builder` exposes only rotation/prefix/
/// suffix), so a filename-date sweep on startup is the minimum that works.
const LOG_RETENTION_DAYS: i64 = 14;

/// Initialize file+stderr logging. The returned `WorkerGuard` MUST be held
/// for the lifetime of the app (keep it in `main`), or buffered logs are lost.
///
/// Uses `tracing::subscriber::set_global_default` (not `SubscriberInitExt::init`)
/// so the `tracing-log` bridge is NOT installed — that bridge would route
/// transcribe-rs's transcript-bearing `log::info!` records into our subscriber
/// (spec §10.1 violation). The `tracing-log` feature cannot be disabled at the
/// Cargo level (feature-unified back on via tracing-appender), so skipping
/// `LogTracer::init` here is the load-bearing privacy guard. The runnable
/// regression check is `tests/log_privacy.rs::log_bridge_is_absent`.
///
/// Also runs a one-shot startup retention sweep (see `sweep_old_logs`) BEFORE
/// the appender is created, so daily logs older than `LOG_RETENTION_DAYS` days
/// don't accumulate without bound.
pub fn init() -> Result<WorkerGuard> {
    let log_dir = paths::log_dir()?;

    // Retention sweep runs first, sequentially — no race with the appender
    // (it isn't created yet). The cutoff is taken from the filename's UTC date,
    // which is the authority and matches the daily rotation scheme; mtime is
    // never consulted.
    let today = today_ordinal();
    let removed = sweep_old_logs(&log_dir, today);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "molvi.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Default `info`, plus `ort=warn`: ort 2.x logs GraphTransformer /
    // Session-init machinery at INFO via tracing directly (NOT the `log` crate,
    // so the no-bridge privacy guard doesn't suppress it) — hundreds of lines
    // per model load. `ort=warn` keeps real ort warnings, drops the INFO spam.
    // RUST_LOG still overrides (try_from_default_env runs first).
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,ort=warn"));

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .with(fmt::layer().with_writer(std::io::stderr).with_ansi(true));
    tracing::subscriber::set_global_default(subscriber).expect("set global tracing subscriber");

    tracing::info!(
        "molvi logging initialized (dir = {})",
        crate::paths::redact_appdata(&log_dir)
    );
    // Metadata-only: a count of swept files, never file contents (spec §10.1).
    tracing::info!("log retention: removed {removed} files older than {LOG_RETENTION_DAYS} days");
    Ok(guard)
}

/// Best-effort startup sweep over `log_dir`: delete every `molvi.log.YYYY-MM-DD`
/// whose date is strictly older than `today - LOG_RETENTION_DAYS`. Runs ONCE,
/// before the appender is created (sequential, no race). Fail-open — any
/// parse/IO error on a single file is skipped, never breaks startup. Returns
/// the removed count so `init` can emit the metadata line AFTER the subscriber
/// is up (the sweep itself stays silent: no subscriber is installed yet).
fn sweep_old_logs(log_dir: &Path, today: i64) -> u32 {
    let cutoff = today - LOG_RETENTION_DAYS;
    let mut removed = 0u32;
    let Ok(entries) = std::fs::read_dir(log_dir) else {
        return 0; // best-effort: dir is created by paths::log_dir before this.
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        let Some(day) = log_file_day(name) else {
            continue; // non-log file or unparseable date: never touch.
        };
        if day >= cutoff {
            continue; // within the retention window (today's file included).
        }
        // ponytail: per-file best-effort remove; a held handle/lock skips it
        // silently. Fail-open — cleanup must never block app startup.
        if std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Parse a `molvi.log.YYYY-MM-DD` filename into a day-ordinal (days since
/// 1970-01-01), or `None` if it isn't a valid rotated log. Strict on the shape
/// (zero-padded, in-range) so foreign files are never mistaken for logs.
fn log_file_day(name: &str) -> Option<i64> {
    let s = name.strip_prefix("molvi.log.")?;
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    if !b[..4].iter().all(u8::is_ascii_digit)
        || !b[5..7].iter().all(u8::is_ascii_digit)
        || !b[8..].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let y = s[..4].parse::<i64>().ok()?;
    let m = s[5..7].parse::<u32>().ok()?;
    let d = s[8..].parse::<u32>().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

/// Hinnant's days-from-civil (proleptic Gregorian) — the inverse of the spike's
/// `utc_stamp` civil-from-days. Day count since 1970-01-01; std-only, no
/// `chrono`/`time` dep. Matches the UTC basis tracing-appender uses for the
/// daily rotation filename (`OffsetDateTime::now_utc()`).
fn days_from_civil(mut y: i64, m: u32, d: u32) -> i64 {
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp as i64 + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Today's UTC day-ordinal (days since 1970-01-01) from `SystemTime`, matching
/// tracing-appender's UTC rotation basis. `unwrap_or(0)` is fail-open for a
/// clock before epoch (impossible in practice).
fn today_ordinal() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86_400) as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_from_civil_anchors() {
        // Mirrors the spike's hand-derived anchors (utc_stamp_known_epoch).
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1970, 1, 2), 1);
        // 2021-01-01 = (51*365 + 13 leap days in [1972,2020]) days after epoch = 18628.
        assert_eq!(days_from_civil(2021, 1, 1), 18628);
    }

    #[test]
    fn log_file_day_parses_rotation_names_only() {
        assert_eq!(
            log_file_day("molvi.log.2026-08-04"),
            Some(days_from_civil(2026, 8, 4))
        );
        // Non-conforming names parse to None (never mistaken for a deletable log).
        assert_eq!(log_file_day("molvi.log"), None);
        assert_eq!(log_file_day("molvi.log.today"), None);
        assert_eq!(log_file_day("settings.json"), None);
        assert_eq!(log_file_day("2026-08-04.molvi.log"), None);
        // Out-of-range / non-zero-padded values are rejected (garbage never matches).
        assert_eq!(log_file_day("molvi.log.2026-13-01"), None); // bad month
        assert_eq!(log_file_day("molvi.log.2026-08-40"), None); // bad day
        assert_eq!(log_file_day("molvi.log.2026-8-4"), None); // not zero-padded (len != 10)
    }

    #[test]
    fn retention_deletes_only_old_rotation_files() {
        // Cutoff = 2026-07-21; "strictly older than cutoff" → delete iff day < cutoff.
        let cutoff = days_from_civil(2026, 7, 21);
        let cases = [
            ("molvi.log.2026-08-04", false), // recent: keep
            ("molvi.log.2026-07-21", false), // == cutoff: keep (not strictly older)
            ("molvi.log.2026-07-20", true),  // one day past cutoff: delete
            ("molvi.log.2026-06-01", true),  // old: delete
            ("settings.json", false),        // not a log file: untouched
            ("molvi.log.bogus", false),      // unparseable date: untouched
        ];
        for (name, expect_delete) in cases {
            let delete = log_file_day(name).is_some_and(|d| d < cutoff);
            assert_eq!(
                delete, expect_delete,
                "{name}: expected delete={expect_delete}, got {delete}"
            );
        }
    }
}
