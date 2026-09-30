//! X11 paste via the XTEST extension (x11rb).
//!
//! The keycode resolution scans the server keymap for the keysyms instead of
//! hardcoding keycodes.

use x11rb::connection::Connection;
use x11rb::protocol::xtest::ConnectionExt as XTestExt;

use super::PasteChord;
use crate::InjectError;

const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;

// X11 keysyms.
const XK_V_LOWER: u32 = 0x0076;
const XK_CONTROL_L: u32 = 0xffe3;
const XK_CONTROL_R: u32 = 0xffe4;
const XK_SHIFT_L: u32 = 0xffe1;
const XK_SHIFT_R: u32 = 0xffe2;

/// The first of `keysyms` whose key isn't the single-key hotkey: with Left
/// Ctrl bound, a paste pressing Left Ctrl would start the next dictation.
fn modifier_keycode(conn: &impl Connection, keysyms: [u32; 2]) -> Result<u8, InjectError> {
    let bound = super::keytap::bound_keycode();
    let mut first = None;
    for keysym in keysyms {
        match keycode_for_keysym(conn, keysym) {
            Ok(kc) if Some(kc) != bound => return Ok(kc),
            Ok(kc) => first = first.or(Some(kc)),
            Err(_) => {}
        }
    }
    first.ok_or_else(|| InjectError::Keystroke(format!("no keycode maps keysym {:#x}", keysyms[0])))
}

fn keycode_for_keysym(conn: &impl Connection, keysym: u32) -> Result<u8, InjectError> {
    let setup = conn.setup();
    let min = setup.min_keycode;
    let max = setup.max_keycode;
    let mapping =
        x11rb::protocol::xproto::ConnectionExt::get_keyboard_mapping(conn, min, max - min + 1)
            .map_err(|e| InjectError::Keystroke(format!("get_keyboard_mapping: {e}")))?
            .reply()
            .map_err(|e| InjectError::Keystroke(format!("get_keyboard_mapping reply: {e}")))?;
    let per = mapping.keysyms_per_keycode as usize;
    for (i, chunk) in mapping.keysyms.chunks(per).enumerate() {
        if chunk.contains(&keysym) {
            return Ok(min + i as u8);
        }
    }
    Err(InjectError::Keystroke(format!(
        "no keycode maps keysym {keysym:#x}"
    )))
}

/// A gap between synthesized key events, as xdotool leaves: some toolkits
/// drop a chord whose events all arrive at once.
const KEY_GAP: std::time::Duration = std::time::Duration::from_millis(12);

fn fake_key(conn: &impl Connection, kind: u8, keycode: u8) -> Result<(), InjectError> {
    conn.xtest_fake_input(kind, keycode, x11rb::CURRENT_TIME, x11rb::NONE, 0, 0, 0)
        .map_err(|e| InjectError::Keystroke(format!("xtest_fake_input: {e}")))?;
    conn.flush()
        .map_err(|e| InjectError::Keystroke(format!("flush: {e}")))?;
    std::thread::sleep(KEY_GAP);
    Ok(())
}

/// Name of the focused application via EWMH: _NET_ACTIVE_WINDOW -> WM_CLASS.
/// Returns the class half of WM_CLASS ("Alacritty", "kitty", ...), which is
/// what per-app paste overrides match against.
pub fn frontmost_app() -> Option<String> {
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    let active_atom = conn
        .intern_atom(false, b"_NET_ACTIVE_WINDOW")
        .ok()?
        .reply()
        .ok()?
        .atom;
    let active = conn
        .get_property(false, root, active_atom, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?;
    let window = active.value32()?.next()?;
    if window == 0 {
        return None;
    }
    let class = conn
        .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
        .ok()?
        .reply()
        .ok()?;
    // WM_CLASS is "instance\0class\0"; the class is the application name.
    let raw = class.value;
    let mut parts = raw.split(|b| *b == 0).filter(|s| !s.is_empty());
    let instance = parts.next();
    let class_name = parts.next().or(instance)?;
    String::from_utf8(class_name.to_vec()).ok()
}

/// Send Ctrl+V (or Ctrl+Shift+V) via XTest.
pub fn paste(chord: PasteChord) -> Result<(), InjectError> {
    let (conn, _screen) = x11rb::connect(None)
        .map_err(|e| InjectError::Unavailable(format!("could not connect to X server: {e}")))?;

    let ctrl = modifier_keycode(&conn, [XK_CONTROL_L, XK_CONTROL_R])?;
    let v = keycode_for_keysym(&conn, XK_V_LOWER)?;
    let shift = if chord.shift {
        Some(modifier_keycode(&conn, [XK_SHIFT_L, XK_SHIFT_R])?)
    } else {
        None
    };

    fake_key(&conn, KEY_PRESS, ctrl)?;
    if let Some(shift) = shift {
        fake_key(&conn, KEY_PRESS, shift)?;
    }
    fake_key(&conn, KEY_PRESS, v)?;
    fake_key(&conn, KEY_RELEASE, v)?;
    if let Some(shift) = shift {
        fake_key(&conn, KEY_RELEASE, shift)?;
    }
    fake_key(&conn, KEY_RELEASE, ctrl)?;
    // A round trip before the connection goes away, so the server has
    // processed every event this client sent.
    x11rb::protocol::xproto::ConnectionExt::get_input_focus(&conn)
        .map_err(|e| InjectError::Keystroke(format!("sync: {e}")))?
        .reply()
        .map_err(|e| InjectError::Keystroke(format!("sync: {e}")))?;
    Ok(())
}
