//! X11 EWMH helpers (Linux only): `_NET_ACTIVE_WINDOW` / `_NET_WM_PID` queries
//! + focus restore. Pure-Rust via x11rb (no external binary). Fail-open on any
//! X11 error. NOT a platform abstraction — concrete free functions, cfg-gated.
//! Privacy §10.1: logs carry no window titles or content (a window id / pid is
//! metadata). On Wayland (`WAYLAND_DISPLAY` set) callers skip these entirely.

#[cfg(all(unix, not(target_os = "macos")))]
use x11rb::connection::Connection;
#[cfg(all(unix, not(target_os = "macos")))]
use x11rb::protocol::xproto::{
    AtomEnum, CLIENT_MESSAGE_EVENT, ClientMessageData, ClientMessageEvent, ConnectionExt, EventMask,
};

/// True when the session is Wayland (no X11 active-window API). Callers use this
/// to skip the X11 path (paste Wayland-bypass; foreground_exe→Err; capture_target
/// →None). `WAYLAND_DISPLAY` is the standard Wayland session env var.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Connect to the X server + return (conn, root_window). None on any error
/// (no DISPLAY, X server down). The connection is dropped by the caller.
#[cfg(all(unix, not(target_os = "macos")))]
fn connect_root() -> Option<(x11rb::rust_connection::RustConnection, u32)> {
    let (conn, screen_num) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots[screen_num].root;
    Some((conn, root))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn read_u32_prop(value: &[u8]) -> Option<u32> {
    // GetPropertyReply.value is raw bytes; a single 32-bit property = 4 LE bytes.
    let bytes: [u8; 4] = value.get(..4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn active_window_id() -> Option<u32> {
    let (conn, root) = connect_root()?;
    let atom = conn
        .intern_atom(false, b"_NET_ACTIVE_WINDOW")
        .ok()?
        .reply()
        .ok()?
        .atom;
    let reply = conn
        .get_property(false, root, atom, AtomEnum::ANY, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    read_u32_prop(&reply.value).filter(|&w| w != 0)
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn window_pid(window: u32) -> Option<u32> {
    let (conn, _root) = connect_root()?;
    let atom = conn
        .intern_atom(false, b"_NET_WM_PID")
        .ok()?
        .reply()
        .ok()?
        .atom;
    let reply = conn
        .get_property(false, window, atom, AtomEnum::ANY, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    read_u32_prop(&reply.value).filter(|&p| p != 0)
}

/// Verify the active window is still `target`; if not, request activation via a
/// `_NET_ACTIVE_WINDOW` client message (mirrors xdotool/wmctrl), re-verify, else
/// Err so the caller leaves text on the clipboard + toasts (§6.6).
#[cfg(all(unix, not(target_os = "macos")))]
pub fn ensure_active_window(target: u32) -> crate::errors::Result<()> {
    if active_window_id() == Some(target) {
        return Ok(());
    }
    tracing::warn!("paste: X11 active-window mismatch, requesting activation");
    let (conn, root) = connect_root().ok_or_else(|| {
        crate::errors::MolviError::Paste("X11 connection failed (focus restore)".into())
    })?;
    let atom = conn
        .intern_atom(false, b"_NET_ACTIVE_WINDOW")
        .map_err(|e| crate::errors::MolviError::Paste(format!("intern atom: {e}")))?
        .reply()
        .map_err(|e| crate::errors::MolviError::Paste(format!("intern reply: {e}")))?
        .atom;
    // [source=2 (pager), timestamp=0 (CurrentTime), 0, 0, 0] — EWMH spec. The
    // concrete ClientMessageEvent is passed directly (Event does not impl
    // Into<[u8;32]>; ClientMessageEvent does — xproto.rs From impl).
    let ev = ClientMessageEvent {
        response_type: CLIENT_MESSAGE_EVENT,
        format: 32,
        sequence: 0,
        window: target,
        type_: atom,
        data: ClientMessageData::from([2u32, 0, 0, 0, 0]),
    };
    let _ = conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_NOTIFY | EventMask::SUBSTRUCTURE_REDIRECT,
        ev,
    );
    let _ = conn.flush();
    std::thread::sleep(std::time::Duration::from_millis(40));
    if active_window_id() == Some(target) {
        Ok(())
    } else {
        tracing::warn!("paste: X11 could not restore focus; left on clipboard");
        Err(crate::errors::MolviError::Paste(
            "focus mismatch; text left on clipboard".into(),
        ))
    }
}

#[cfg(test)]
#[cfg(all(unix, not(target_os = "macos")))]
mod tests {
    use super::read_u32_prop;

    #[test]
    fn read_u32_prop_parses_le() {
        assert_eq!(read_u32_prop(&[0x78, 0x56, 0x34, 0x12]), Some(0x12345678));
    }

    #[test]
    fn read_u32_prop_rejects_short() {
        assert_eq!(read_u32_prop(&[]), None);
        assert_eq!(read_u32_prop(&[1, 2, 3]), None);
    }

    #[test]
    fn read_u32_prop_ignores_trailing() {
        // Only the first 4 bytes matter (long_length=1 requested).
        assert_eq!(
            read_u32_prop(&[0x78, 0x56, 0x34, 0x12, 0xff, 0xff]),
            Some(0x12345678)
        );
    }
}
