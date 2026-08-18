//! X11 paste via the XTEST extension (x11rb).
//!
//! Unverified on this development machine (written on macOS); the keycode
//! resolution scans the server keymap for the keysyms instead of hardcoding
//! keycodes.

use x11rb::connection::Connection;
use x11rb::protocol::xtest::ConnectionExt as XTestExt;

use super::PasteChord;
use crate::InjectError;

const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;

// X11 keysyms.
const XK_V_LOWER: u32 = 0x0076;
const XK_CONTROL_L: u32 = 0xffe3;
const XK_SHIFT_L: u32 = 0xffe1;

fn keycode_for_keysym(
    conn: &impl Connection,
    keysym: u32,
) -> Result<u8, InjectError> {
    let setup = conn.setup();
    let min = setup.min_keycode;
    let max = setup.max_keycode;
    let mapping = x11rb::protocol::xproto::ConnectionExt::get_keyboard_mapping(
        conn,
        min,
        max - min + 1,
    )
    .map_err(|e| InjectError::Keystroke(format!("get_keyboard_mapping: {e}")))?
    .reply()
    .map_err(|e| InjectError::Keystroke(format!("get_keyboard_mapping reply: {e}")))?;
    let per = mapping.keysyms_per_keycode as usize;
    for (i, chunk) in mapping.keysyms.chunks(per).enumerate() {
        if chunk.contains(&keysym) {
            return Ok(min + i as u8);
        }
    }
    Err(InjectError::Keystroke(format!("no keycode maps keysym {keysym:#x}")))
}

fn fake_key(conn: &impl Connection, kind: u8, keycode: u8) -> Result<(), InjectError> {
    conn.xtest_fake_input(kind, keycode, x11rb::CURRENT_TIME, x11rb::NONE, 0, 0, 0)
        .map_err(|e| InjectError::Keystroke(format!("xtest_fake_input: {e}")))?;
    Ok(())
}

/// Send Ctrl+V (or Ctrl+Shift+V) via XTest.
pub fn paste(chord: PasteChord) -> Result<(), InjectError> {
    let (conn, _screen) = x11rb::connect(None)
        .map_err(|e| InjectError::Unavailable(format!("could not connect to X server: {e}")))?;

    let ctrl = keycode_for_keysym(&conn, XK_CONTROL_L)?;
    let v = keycode_for_keysym(&conn, XK_V_LOWER)?;
    let shift = if chord.shift { Some(keycode_for_keysym(&conn, XK_SHIFT_L)?) } else { None };

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
    conn.flush()
        .map_err(|e| InjectError::Keystroke(format!("flush: {e}")))?;
    Ok(())
}
