//! Per-app post-processing profiles (spec §1.6). The resolver building block:
//! `foreground_exe()` → UPPERCASED basename of the foreground window's process
//! image; `resolve()` → first enabled profile whose `exe` matches case-
//! insensitively. Profiles live in `settings.json` (`Vec<ProfileEntry>`) — no
//! separate DB (spec §6.4's `profiles.db` was dropped, ponytail: settings
//! already persist this config-shaped data). Fail-open: any Win32 error →
//! `Err` → the Task-8 caller falls back to global settings. Task 8 wires these
//! into `begin_session`; this module changes no runtime behavior on its own.
//!
//! Privacy (spec §10.1): logs metadata only (exe basename + Win32 error
//! strings, debug level); never window title / transcript / text.

use crate::errors::{MolviError, Result};
use crate::settings::ProfileEntry;

/// Foreground window's process image basename, UPPERCASED (e.g. `"WINWORD.EXE"`).
/// Win32 chain: foreground HWND → PID → `OpenProcess` (least-privilege) →
/// `QueryFullProcessImageNameW` → basename. Fail-open on every step (no fg
/// window, `OpenProcess` denied for an elevated proc, etc.); the caller treats
/// `Err` as "no profile match, use global settings". Mirrors `ort_affinity`'s
/// unsafe + SAFETY-comment + fail-open pattern.
#[cfg(target_os = "windows")]
pub fn foreground_exe() -> Result<String> {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    use windows::core::PWSTR;
    // SAFETY: GetForegroundWindow is a thread-safe query; no handles retained.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0 as isize == 0 {
        return Err(MolviError::Profile("no foreground window".into()));
    }
    let mut pid: u32 = 0;
    // SAFETY: hwnd is a live foreground HWND; &mut pid is a valid *mut u32.
    let tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if tid == 0 || pid == 0 {
        return Err(MolviError::Profile(
            "GetWindowThreadProcessId failed".into(),
        ));
    }
    // SAFETY: PROCESS_QUERY_LIMITED_INFORMATION is least-privilege; false = no
    // inherit. Handle is closed via CloseHandle below; no aliasing of it.
    let handle: HANDLE = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .map_err(|e| MolviError::Profile(format!("OpenProcess: {e}")))?;
    // QueryFullProcessImageNameW writes a wide path. 1024 u16s is generous (paths
    // can exceed MAX_PATH=260 with the \\?\ prefix). ponytail: fixed stack buf —
    // no path in practice exceeds 1024 wchars; a heap grow loop is YAGNI.
    let mut buf = [0u16; 1024];
    let mut len: u32 = buf.len() as u32;
    // SAFETY: handle is a valid PROCESS_QUERY_LIMITED_INFORMATION handle; buf
    // owns 1024 u16s; len = capacity in chars (incl null). PWSTR wraps the ptr.
    let r = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    // Always close, even on query error (don't leak the handle we opened).
    let _ = unsafe { CloseHandle(handle) };
    let () = r.map_err(|e| MolviError::Profile(format!("QueryFullProcessImageNameW: {e}")))?;
    // Trim at first NUL — robust whether `len` includes the terminator or not.
    let end = buf.iter().position(|&c| c == 0).unwrap_or(len as usize);
    let full = String::from_utf16_lossy(&buf[..end]);
    // Basename = chars after the last separator (Windows uses '\', tolerate '/'
    // for robustness). UPPERCASE per spec (case-insensitive FS, canonical form).
    let base = full
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&full)
        .to_ascii_uppercase();
    if base.is_empty() {
        return Err(MolviError::Profile("empty image basename".into()));
    }
    // Exe basename IS metadata (privacy-safe). debug, not warn: OpenProcess
    // denies elevated windows routinely, so an Err here is not warn-worthy.
    tracing::debug!("foreground exe: {base}");
    Ok(base)
}

/// Non-Windows stub (Step 0). macOS (NSWorkspace) lands in Phase 2; Linux X11
/// (_NET_WM_PID) in Phase 3. Fail-open: caller treats Err as "no profile match,
/// use global settings" (pipeline.rs).
#[cfg(not(target_os = "windows"))]
pub fn foreground_exe() -> Result<String> {
    Err(MolviError::Profile(
        "foreground_exe not implemented on this OS yet".into(),
    ))
}

/// First ENABLED profile whose `exe` matches `exe` case-insensitively (both
/// sides uppercased; spec §6.4 case-insensitive basename match). A disabled
/// profile is treated as no match (fail-open to global settings). `exe` is
/// expected to be the UPPERCASED basename from `foreground_exe()`, but is
/// uppercased again defensively (cheap; caller may pass raw). Pure — no
/// logging (privacy §10.1).
//
// ponytail: two entries for the same exe is a user error; first-found is
// deterministic and enough — duplicate resolution (warn / merge / last-wins)
// is YAGNI until a UI surfaces it.
pub fn resolve<'a>(profiles: &'a [ProfileEntry], exe: &str) -> Option<&'a ProfileEntry> {
    let needle = exe.to_ascii_uppercase();
    profiles
        .iter()
        .find(|p| p.enabled && p.exe.to_ascii_uppercase() == needle)
}

/// Apply a resolved profile's per-app override to a `PostProcessing` clone.
/// `post_mode` always overrides (the profile exists to change the mode);
/// `prompt` overrides only when the profile carries one (else the global
/// prompt / `MOLVI_DEFAULT_PROMPT` stands). `endpoint` / `model` / `api_key`
/// are CONNECTION config — never overridden by a profile (global). Pure — no
/// logging (privacy §10.1: the prompt is user content, never traced).
pub fn apply_profile_override(
    post: &mut crate::settings::PostProcessing,
    profile: Option<&ProfileEntry>,
) {
    if let Some(p) = profile {
        post.mode = p.post_mode;
        if let Some(prompt) = &p.prompt {
            post.prompt = Some(prompt.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{PostMode, PostProcessing};

    fn profile(exe: &str, enabled: bool) -> ProfileEntry {
        ProfileEntry {
            exe: exe.into(),
            post_mode: PostMode::Smart,
            prompt: None,
            enabled,
        }
    }

    #[test]
    fn resolve_finds_stored_profile_by_uppercase_basename() {
        let ps = [profile("WINWORD.EXE", true)];
        let m = resolve(&ps, "WINWORD.EXE").expect("match");
        assert_eq!(m.exe, "WINWORD.EXE");
        assert!(m.enabled);
    }

    #[test]
    fn resolve_returns_none_for_unknown_exe() {
        let ps = [profile("WINWORD.EXE", true)];
        assert!(resolve(&ps, "EXCEL.EXE").is_none());
    }

    #[test]
    fn resolve_is_case_insensitive() {
        // Stored lowercase, query uppercase — must still match.
        let ps = [profile("winword.exe", true)];
        let m = resolve(&ps, "WINWORD.EXE").expect("match");
        assert_eq!(m.exe, "winword.exe");
    }

    #[test]
    fn resolve_skips_disabled_profile() {
        // enabled=false → treated as absent (fail-open to global settings).
        let ps = [profile("WINWORD.EXE", false)];
        assert!(resolve(&ps, "WINWORD.EXE").is_none());
    }

    #[test]
    fn resolve_first_enabled_wins() {
        // Disabled entry followed by an enabled one for the same exe: the
        // enabled one must win (disabled entries are skipped, not matched).
        let ps = [profile("WINWORD.EXE", false), profile("WINWORD.EXE", true)];
        let m = resolve(&ps, "WINWORD.EXE").expect("match");
        assert!(m.enabled);
    }

    #[test]
    fn resolve_none_for_empty_profiles() {
        assert!(resolve(&[], "WINWORD.EXE").is_none());
    }

    /// Smoke test of the real Win32 chain. On an interactive dev session this
    /// is `Ok(non-empty)`; on a headless CI VM with no foreground window it may
    /// be `Err`. Either is acceptable — we only assert it never panics. Mirrors
    /// `ort_affinity::p_core_mask_is_some_or_none_gracefully`.
    #[test]
    fn foreground_exe_smoke() {
        if let Ok(name) = foreground_exe() {
            assert!(!name.is_empty());
        }
    }

    // Helper: build a base `PostProcessing` for the override tests. Only mode
    // + prompt vary; the rest come from `Default` (Smart defaults, no conn).
    fn post(mode: PostMode, prompt: Option<&str>) -> PostProcessing {
        PostProcessing {
            mode,
            prompt: prompt.map(String::from),
            ..PostProcessing::default()
        }
    }

    fn prof(post_mode: PostMode, prompt: Option<&str>) -> ProfileEntry {
        ProfileEntry {
            exe: "WINWORD.EXE".into(),
            post_mode,
            prompt: prompt.map(String::from),
            enabled: true,
        }
    }

    #[test]
    fn apply_profile_override_none_is_noop() {
        // None must leave every field byte-identical (PostProcessing isn't
        // PartialEq; assert each field). Base carries non-default values so a
        // silent mutation would be caught.
        let mut p = post(PostMode::Polished, Some("global"));
        p.endpoint = Some("https://e".into());
        p.model = Some("m".into());
        p.api_key = Some("k".into());
        apply_profile_override(&mut p, None);
        assert_eq!(p.mode, PostMode::Polished);
        assert_eq!(p.prompt.as_deref(), Some("global"));
        assert_eq!(p.endpoint.as_deref(), Some("https://e"));
        assert_eq!(p.model.as_deref(), Some("m"));
        assert_eq!(p.api_key.as_deref(), Some("k"));
        // smart toggles untouched — assert both a default-true and a
        // default-false toggle so a hypothetical accidental write trips.
        assert!(p.smart.apply_dictionary);
        assert!(p.smart.fix_case);
        assert!(!p.smart.remove_fillers);
    }

    #[test]
    fn apply_profile_override_applies_post_mode_and_prompt() {
        // Profile (Polished, Some) over a Smart/None base → both applied.
        let mut p = post(PostMode::Smart, None);
        apply_profile_override(
            &mut p,
            Some(&prof(PostMode::Polished, Some("polish words"))),
        );
        assert_eq!(p.mode, PostMode::Polished);
        assert_eq!(p.prompt.as_deref(), Some("polish words"));
    }

    #[test]
    fn apply_profile_override_prompt_none_keeps_global() {
        // Profile (Raw, None) over a base with prompt Some("global") → mode
        // overridden to Raw, prompt stays the GLOBAL value.
        let mut p = post(PostMode::Smart, Some("global"));
        apply_profile_override(&mut p, Some(&prof(PostMode::Raw, None)));
        assert_eq!(p.mode, PostMode::Raw);
        assert_eq!(p.prompt.as_deref(), Some("global"));
    }

    #[test]
    fn apply_profile_override_prompt_replaces_global() {
        // Profile prompt REPLACES an existing global prompt (the helper's
        // `post.prompt = Some(prompt.clone())` path): base Some("global
        // prompt") + profile Some("polish words") → Some("polish words").
        let mut p = post(PostMode::Smart, Some("global prompt"));
        apply_profile_override(
            &mut p,
            Some(&prof(PostMode::Polished, Some("polish words"))),
        );
        assert_eq!(p.mode, PostMode::Polished);
        assert_eq!(p.prompt.as_deref(), Some("polish words"));
    }

    #[test]
    fn apply_profile_override_never_touches_connection_config() {
        // Load-bearing invariant: profiles change HOW to process (mode/prompt),
        // not WHERE the LLM lives (endpoint/model/api_key stay global).
        let mut p = post(PostMode::Smart, None);
        p.endpoint = Some("https://e".into());
        p.model = Some("m".into());
        p.api_key = Some("k".into());
        apply_profile_override(&mut p, Some(&prof(PostMode::Polished, Some("p"))));
        assert_eq!(p.endpoint.as_deref(), Some("https://e"));
        assert_eq!(p.model.as_deref(), Some("m"));
        assert_eq!(p.api_key.as_deref(), Some("k"));
        assert_eq!(p.mode, PostMode::Polished);
        assert_eq!(p.prompt.as_deref(), Some("p"));
    }
}
