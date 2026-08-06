//! P-core process affinity (spec §11, measure-first). Restricts the whole
//! molvi process to performance cores so ort's intra-op thread pool — which
//! ort spawns itself, so worker-thread affinity wouldn't reach it — is born
//! into the P-core set. Mechanism = PROCESS affinity.
//!
//! Fail-open is the hard rule: any Win32 error OR a homogeneous CPU (no
//! E-cores) returns `None` and the caller skips. Never breaks startup over a
//! perf optimization.

use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationProcessorCore,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetProcessAffinityMask, SetProcessAffinityMask,
};

/// Compute a process-affinity mask covering only P-cores (`EfficiencyClass ==
/// 0`). Returns `None` when the CPU is homogeneous (no E-cores → affinity is
/// pointless) or on any Win32/shape error (fail-open).
pub fn p_core_mask() -> Option<usize> {
    // Grow-and-retry (verified windows 0.62.2 shape): the first call with no
    // buffer returns Err(ERROR_INSUFFICIENT_BUFFER) and writes the required
    // byte length; the second call with an allocated buffer returns Ok(()).
    let mut len: u32 = 0;
    // SAFETY: documented size query — only writes `len`, never dereferences the
    // null buffer pointer.
    unsafe {
        let _ = GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut len);
    }
    if len == 0 {
        return None;
    }
    let mut buf: Vec<u8> = vec![0u8; len as usize];
    // SAFETY: `buf` owns `len` bytes; cast to the documented element pointer
    // type. The second call fills exactly `len` bytes on success.
    let filled = unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            Some(buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX),
            &mut len,
        )
    };
    if filled.is_err() {
        return None;
    }

    // Walk the variable-size buffer in strides of each entry's own `Size`.
    // The API returns one `RelationProcessorCore` entry per logical processor
    // group-set; we still gate on Relationship defensively.
    let total = len as usize;
    // Header is Relationship (i32) + Size (u32) = 8 bytes — enough to read Size.
    const HEADER: usize = 8;
    let mut mask: usize = 0;
    let mut is_heterogeneous = false;
    let mut offset = 0usize;
    while offset + HEADER <= total {
        // SAFETY: `offset` is within `buf` (checked) and 8-aligned. The buffer
        // is a Vec<u8> whose data has Rust alignment 1, but on Windows the
        // global allocator (HeapAlloc) returns memory aligned to
        // MEMORY_ALLOCATION_ALIGNMENT (8 bytes on x64), and the Win32 API
        // returns each entry's Size as a multiple of
        // align_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>() — so every
        // stride keeps us aligned to the struct we cast to. The cast is sound
        // on every supported target; a future porter to a custom allocator
        // would need to uphold this alignment. Reading `Relationship` and
        // `Size` needs only the verified HEADER bytes.
        let entry = unsafe {
            &*(buf.as_ptr().add(offset) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX)
        };
        let size = entry.Size as usize;
        let relationship = entry.Relationship;
        if size == 0 || offset + size > total {
            return None; // malformed buffer — fail open rather than risk UB
        }
        if relationship == RelationProcessorCore {
            // SAFETY: Relationship == RelationProcessorCore ⇒ the Processor
            // union variant is active, and `offset + size <= total` (checked
            // above) guarantees the full Processor variant fits in the buffer.
            let proc = unsafe { &entry.Anonymous.Processor };
            if proc.EfficiencyClass == 1 {
                is_heterogeneous = true;
            } else if proc.GroupCount >= 1 {
                // ponytail: collapses per-group masks into one usize. molvi
                // targets single-group consumer CPUs (<64 logical CPUs/group),
                // so GroupCount == 1 and GroupMask[0] is the whole story. A
                // multi-group / >64-thread box needs per-group affinity
                // (SetThreadAffinityMask per group) — out of scope here.
                mask |= proc.GroupMask[0].Mask;
            }
        }
        offset += size;
    }

    if !is_heterogeneous || mask == 0 {
        return None;
    }
    Some(mask)
}

/// Capture the current process affinity mask (all logical cores by default,
/// before any pinning). Returned to `apply_for_engine` so Nemotron can restore
/// the full set (P-core pinning is ~40% slower for Nemotron — Task-17 spike).
/// `None` on any Win32 error (fail-open).
pub fn capture_process_affinity() -> Option<usize> {
    let mut proc_mask: usize = 0;
    let mut sys_mask: usize = 0;
    // SAFETY: GetCurrentProcess() returns a pseudo-handle valid for the calling
    // process; both out-params are stack `usize` vars of the documented type
    // (PDWORD_PTR = pointer to usize on x64).
    let ok = unsafe { GetProcessAffinityMask(GetCurrentProcess(), &mut proc_mask, &mut sys_mask) };
    if ok.is_ok() && proc_mask != 0 {
        Some(proc_mask)
    } else {
        None
    }
}

/// Apply the engine-appropriate process affinity:
/// - Nemotron → restore `original_mask` (all logical cores; parakeet-rs uses
///   the full 4P+4E set — measured ~40% faster than P-core pinning).
/// - GigaAM (default) → P-cores (helps GigaAM, Task 5); fall back to
///   `original_mask` if P-core detection fails. Fail-open on any Win32 error
///   (warn + leave affinity as-is).
pub fn apply_for_engine(original_mask: usize, is_nemotron: bool) {
    let mask = if is_nemotron {
        original_mask
    } else {
        p_core_mask().unwrap_or(original_mask)
    };
    if mask == 0 {
        tracing::warn!("affinity mask resolved to 0; leaving affinity as-is");
        return;
    }
    // SAFETY: SetProcessAffinityMask on the current process; `mask` is either
    // the captured original (real topology) or a P-core mask computed from it.
    // GetCurrentProcess() is always valid for the calling process.
    let ok = unsafe { SetProcessAffinityMask(GetCurrentProcess(), mask) };
    if ok.is_ok() {
        let policy = if is_nemotron { "all-cores" } else { "p-cores" };
        tracing::info!("process affinity set to {policy} (mask=0x{mask:X})");
    } else {
        tracing::warn!("SetProcessAffinityMask failed; affinity unchanged");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test of the real Win32 enumeration. On the dev i5-12450H (4P+4E)
    /// this is `Some(non-zero)`; on a homogeneous CI VM it may be `None`.
    /// Either is acceptable — we only assert it never panics and never returns 0.
    #[test]
    fn p_core_mask_is_some_or_none_gracefully() {
        if let Some(m) = p_core_mask() {
            assert_ne!(m, 0);
        }
    }

    /// `capture_process_affinity` reads the live process mask. Always `Some`
    /// (non-zero) on a real Windows process; never panics. Mirrors the
    /// p_core_mask smoke test's fail-open contract.
    #[test]
    fn capture_process_affinity_smoke() {
        if let Some(m) = capture_process_affinity() {
            assert_ne!(m, 0);
        }
    }
}
