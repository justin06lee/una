//! macOS virtual-keycode knowledge: modifier tables, human-readable key
//! names (static map for specials, UCKeyTranslate for character keys on the
//! current layout), and plugin-parseable combo tokens.

use std::ffi::c_void;

use core_graphics::event::CGEventFlags;

// ---------------------------------------------------------------------------
// Carbon externs for layout-aware key translation
// ---------------------------------------------------------------------------

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut c_void;
    fn TISGetInputSourceProperty(source: *mut c_void, property_key: *const c_void) -> *mut c_void;
    static kTISPropertyUnicodeKeyLayoutData: *const c_void;
    fn UCKeyTranslate(
        key_layout_ptr: *const c_void,
        virtual_key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        key_translate_options: u32,
        dead_key_state: *mut u32,
        max_string_length: usize,
        actual_string_length: *mut usize,
        unicode_string: *mut u16,
    ) -> i32;
    fn LMGetKbdType() -> u8;
    fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
    fn CFRelease(cf: *const c_void);
}

const K_UC_KEY_ACTION_DISPLAY: u16 = 3;
const K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK: u32 = 1;

/// Translate a virtual keycode to the character it produces on the current
/// keyboard layout (no modifiers). Returns None for non-character keys or
/// when layout resolution fails.
pub fn char_for_keycode(keycode: u16) -> Option<char> {
    unsafe {
        let source = TISCopyCurrentKeyboardLayoutInputSource();
        if source.is_null() {
            return None;
        }
        let layout_data = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
        if layout_data.is_null() {
            CFRelease(source);
            return None;
        }
        let layout = CFDataGetBytePtr(layout_data) as *const c_void;
        let kbd_type = LMGetKbdType() as u32;
        let mut dead_key_state: u32 = 0;
        let mut chars = [0u16; 4];
        let mut len: usize = 0;
        let status = UCKeyTranslate(
            layout,
            keycode,
            K_UC_KEY_ACTION_DISPLAY,
            0,
            kbd_type,
            K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_MASK,
            &mut dead_key_state,
            chars.len(),
            &mut len,
            chars.as_mut_ptr(),
        );
        CFRelease(source);
        if status == 0 && len == 1 {
            char::from_u32(chars[0] as u32).filter(|c| !c.is_control() && *c != ' ')
        } else {
            None
        }
    }
}

/// Find the virtual keycode producing `wanted` on the current layout.
pub fn keycode_for_char(wanted: char) -> Option<u16> {
    (0u16..128).find(|&kc| char_for_keycode(kc) == Some(wanted))
}

// ---------------------------------------------------------------------------
// Modifier keys
// ---------------------------------------------------------------------------

pub const KC_RIGHT_CMD: u16 = 54;
pub const KC_LEFT_CMD: u16 = 55;
pub const KC_LEFT_SHIFT: u16 = 56;
pub const KC_CAPS_LOCK: u16 = 57;
pub const KC_LEFT_OPT: u16 = 58;
pub const KC_LEFT_CTRL: u16 = 59;
pub const KC_RIGHT_SHIFT: u16 = 60;
pub const KC_RIGHT_OPT: u16 = 61;
pub const KC_RIGHT_CTRL: u16 = 62;
pub const KC_FN: u16 = 63;
pub const KC_ESCAPE: u16 = 53;

/// The device-independent flag bit a modifier keycode contributes to
/// `CGEventFlags`, or None for non-modifier keys.
pub fn modifier_flag(keycode: u16) -> Option<CGEventFlags> {
    match keycode {
        KC_RIGHT_CMD | KC_LEFT_CMD => Some(CGEventFlags::CGEventFlagCommand),
        KC_LEFT_SHIFT | KC_RIGHT_SHIFT => Some(CGEventFlags::CGEventFlagShift),
        KC_LEFT_OPT | KC_RIGHT_OPT => Some(CGEventFlags::CGEventFlagAlternate),
        KC_LEFT_CTRL | KC_RIGHT_CTRL => Some(CGEventFlags::CGEventFlagControl),
        KC_CAPS_LOCK => Some(CGEventFlags::CGEventFlagAlphaShift),
        KC_FN => Some(CGEventFlags::CGEventFlagSecondaryFn),
        _ => None,
    }
}

pub fn is_modifier(keycode: u16) -> bool {
    modifier_flag(keycode).is_some()
}

// ---------------------------------------------------------------------------
// Human-readable names
// ---------------------------------------------------------------------------

/// Static names for modifiers, F-keys, navigation and keypad keys.
fn special_name(keycode: u16) -> Option<&'static str> {
    Some(match keycode {
        KC_RIGHT_CMD => "Right ⌘",
        KC_LEFT_CMD => "Left ⌘",
        KC_LEFT_SHIFT => "Left ⇧",
        KC_CAPS_LOCK => "Caps Lock",
        KC_LEFT_OPT => "Left ⌥",
        KC_LEFT_CTRL => "Left ⌃",
        KC_RIGHT_SHIFT => "Right ⇧",
        KC_RIGHT_OPT => "Right ⌥",
        KC_RIGHT_CTRL => "Right ⌃",
        KC_FN => "Fn",
        36 => "Return",
        48 => "Tab",
        49 => "Space",
        51 => "Delete",
        53 => "Esc",
        64 => "F17",
        65 => "Num .",
        67 => "Num *",
        69 => "Num +",
        71 => "Num Clear",
        75 => "Num /",
        76 => "Num Enter",
        78 => "Num -",
        79 => "F18",
        80 => "F19",
        81 => "Num =",
        82 => "Num 0",
        83 => "Num 1",
        84 => "Num 2",
        85 => "Num 3",
        86 => "Num 4",
        87 => "Num 5",
        88 => "Num 6",
        89 => "Num 7",
        90 => "F20",
        91 => "Num 8",
        92 => "Num 9",
        96 => "F5",
        97 => "F6",
        98 => "F7",
        99 => "F3",
        100 => "F8",
        101 => "F9",
        103 => "F11",
        105 => "F13",
        106 => "F16",
        107 => "F14",
        109 => "F10",
        111 => "F12",
        113 => "F15",
        114 => "Help",
        115 => "Home",
        116 => "Page Up",
        117 => "Forward Delete",
        118 => "F4",
        119 => "End",
        120 => "F2",
        121 => "Page Down",
        122 => "F1",
        123 => "←",
        124 => "→",
        125 => "↓",
        126 => "↑",
        _ => return None,
    })
}

/// Best human-readable name for a keycode: static specials first, then the
/// current layout's character (uppercased), then a numeric fallback.
pub fn key_name(keycode: u16) -> String {
    if let Some(name) = special_name(keycode) {
        return name.to_string();
    }
    if let Some(c) = char_for_keycode(keycode) {
        return c.to_uppercase().to_string();
    }
    format!("Key {keycode}")
}

// ---------------------------------------------------------------------------
// Combo tokens for the global-shortcut plugin
// ---------------------------------------------------------------------------

/// Position-based token understood by the global-shortcut plugin's
/// `Shortcut::from_str` (W3C `Code`-style, with plain letters/digits).
/// Conservative: returns None for keys the plugin can't reliably express, in
/// which case the caller falls back to a native single-key binding.
pub fn combo_token(keycode: u16) -> Option<&'static str> {
    Some(match keycode {
        // ANSI letter positions.
        0 => "A",
        11 => "B",
        8 => "C",
        2 => "D",
        14 => "E",
        3 => "F",
        5 => "G",
        4 => "H",
        34 => "I",
        38 => "J",
        40 => "K",
        37 => "L",
        46 => "M",
        45 => "N",
        31 => "O",
        35 => "P",
        12 => "Q",
        15 => "R",
        1 => "S",
        17 => "T",
        32 => "U",
        9 => "V",
        13 => "W",
        7 => "X",
        16 => "Y",
        6 => "Z",
        // Digit row.
        29 => "0",
        18 => "1",
        19 => "2",
        20 => "3",
        21 => "4",
        23 => "5",
        22 => "6",
        26 => "7",
        28 => "8",
        25 => "9",
        // Punctuation.
        24 => "Equal",
        27 => "Minus",
        30 => "BracketRight",
        33 => "BracketLeft",
        39 => "Quote",
        41 => "Semicolon",
        42 => "Backslash",
        43 => "Comma",
        44 => "Slash",
        47 => "Period",
        50 => "Backquote",
        // Whitespace / editing.
        36 => "Enter",
        48 => "Tab",
        49 => "Space",
        51 => "Backspace",
        53 => "Escape",
        117 => "Delete",
        // Navigation.
        115 => "Home",
        119 => "End",
        116 => "PageUp",
        121 => "PageDown",
        123 => "ArrowLeft",
        124 => "ArrowRight",
        125 => "ArrowDown",
        126 => "ArrowUp",
        // Function keys.
        122 => "F1",
        120 => "F2",
        99 => "F3",
        118 => "F4",
        96 => "F5",
        97 => "F6",
        98 => "F7",
        100 => "F8",
        101 => "F9",
        109 => "F10",
        103 => "F11",
        111 => "F12",
        105 => "F13",
        107 => "F14",
        113 => "F15",
        106 => "F16",
        64 => "F17",
        79 => "F18",
        80 => "F19",
        90 => "F20",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_tables_agree() {
        for kc in [54, 55, 56, 57, 58, 59, 60, 61, 62, 63] {
            assert!(is_modifier(kc), "keycode {kc} should be a modifier");
            assert!(special_name(kc).is_some());
            assert!(combo_token(kc).is_none());
        }
        assert!(!is_modifier(49));
        assert_eq!(combo_token(49), Some("Space"));
    }

    #[test]
    fn f_keys_named() {
        assert_eq!(key_name(96), "F5");
        assert_eq!(key_name(111), "F12");
        assert_eq!(key_name(63), "Fn");
        assert_eq!(key_name(54), "Right ⌘");
    }
}
