//! Single-key hotkeys on X11: the Linux counterpart of the macOS event tap.
//!
//! XInput2 raw key events, selected on the root window, report every key
//! press and release on the display whichever window has focus, without
//! grabbing anything — so a bare modifier (Right Ctrl, Left Super, …) can be
//! a push-to-talk key and still work as a modifier everywhere else, as on the
//! Mac. A non-modifier key (F13, Pause, …) is also passively grabbed, so that
//! while una runs it stops reaching the focused app, again as on the Mac.
//!
//! Keycodes are X keycodes (evdev + 8: Right Ctrl is 105), stored in the
//! same `native:<keycode>:<Name>` bindings the Mac uses for its own codes.
//!
//! Wayland has no equivalent: a client only sees keys sent to its own
//! windows, so [`KeyTap::spawn`] refuses there and bindings stay combos.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask, Window};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

use super::detect::{session_kind, SessionKind};

/// What the bound key just did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEdge {
    Down,
    Up,
    /// The bound key is a modifier and another key (or a click) came while
    /// it was held, soon after the press: it was part of a shortcut, not a
    /// press to dictate. Whatever the press started should be dropped.
    Shortcut,
}

/// How soon after a bound modifier goes down another key still makes it a
/// shortcut. Later than this, the key is typed mid-dictation and the
/// dictation carries on.
pub const SHORTCUT_WINDOW: Duration = Duration::from_millis(1000);

/// A key press captured for the settings recorder.
#[derive(Debug, Clone)]
pub struct CapturedKey {
    pub keycode: u16,
    /// Human-readable name ("Right Ctrl", "F13", "A", …).
    pub name: String,
    pub is_modifier: bool,
    /// When a non-modifier key was pressed with modifiers held and the key
    /// has a plugin-parseable token: the combo string ("Ctrl+Alt+Space").
    pub combo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CaptureError {
    #[error("no key was pressed before the timeout")]
    Timeout,
    #[error("capture was cancelled")]
    Cancelled,
}

/// The keycode bound as a single-key hotkey, for the paste injector: it must
/// not press that key itself (see `x11::paste`). 0 when none.
static BOUND: AtomicU32 = AtomicU32::new(0);

pub(crate) fn bound_keycode() -> Option<u8> {
    u8::try_from(BOUND.load(Ordering::SeqCst)).ok().filter(|&kc| kc != 0)
}

// ---------------------------------------------------------------------------
// Keysyms
// ---------------------------------------------------------------------------

/// What a keycode is on the current layout, looked up once per keymap.
#[derive(Debug, Clone)]
struct KeyInfo {
    name: String,
    /// The modifier it is, when it is one.
    modifier: Option<Modifier>,
    /// Its token in a combo string, when the shortcut plugin knows it.
    token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Super,
    /// Caps Lock, AltGr, Num Lock: modifiers, but no part of a combo.
    Other,
}

fn describe(keysym: u32, keycode: u8) -> KeyInfo {
    let modifier = |name: &str, m| KeyInfo {
        name: name.to_string(),
        modifier: Some(m),
        token: None,
    };
    let key = |name: &str, token: &str| KeyInfo {
        name: name.to_string(),
        modifier: None,
        token: Some(token.to_string()),
    };
    match keysym {
        0xffe3 => modifier("Left Ctrl", Modifier::Ctrl),
        0xffe4 => modifier("Right Ctrl", Modifier::Ctrl),
        0xffe9 => modifier("Left Alt", Modifier::Alt),
        0xffea => modifier("Right Alt", Modifier::Alt),
        0xffe7 => modifier("Left Meta", Modifier::Alt),
        0xffe8 => modifier("Right Meta", Modifier::Alt),
        0xffe1 => modifier("Left Shift", Modifier::Shift),
        0xffe2 => modifier("Right Shift", Modifier::Shift),
        0xffeb => modifier("Left Super", Modifier::Super),
        0xffec => modifier("Right Super", Modifier::Super),
        0xffed => modifier("Left Hyper", Modifier::Super),
        0xffee => modifier("Right Hyper", Modifier::Super),
        0xfe03 => modifier("AltGr", Modifier::Other),
        0xffe5 => modifier("Caps Lock", Modifier::Other),
        0xff7f => modifier("Num Lock", Modifier::Other),
        0x20 => key("Space", "Space"),
        0xff0d => key("Enter", "Enter"),
        0xff09 => key("Tab", "Tab"),
        0xff1b => key("Esc", "Escape"),
        0xff08 => key("Backspace", "Backspace"),
        0xffff => key("Delete", "Delete"),
        0xff63 => key("Insert", "Insert"),
        0xff50 => key("Home", "Home"),
        0xff57 => key("End", "End"),
        0xff55 => key("Page Up", "PageUp"),
        0xff56 => key("Page Down", "PageDown"),
        0xff51 => key("←", "ArrowLeft"),
        0xff52 => key("↑", "ArrowUp"),
        0xff53 => key("→", "ArrowRight"),
        0xff54 => key("↓", "ArrowDown"),
        0xff13 => key("Pause", "Pause"),
        0xff14 => key("Scroll Lock", "ScrollLock"),
        0xff61 => key("Print Screen", "PrintScreen"),
        0xff67 => key("Menu", "ContextMenu"),
        0x2d => key("-", "Minus"),
        0x3d => key("=", "Equal"),
        0x5b => key("[", "BracketLeft"),
        0x5d => key("]", "BracketRight"),
        0x3b => key(";", "Semicolon"),
        0x27 => key("'", "Quote"),
        0x2c => key(",", "Comma"),
        0x2e => key(".", "Period"),
        0x2f => key("/", "Slash"),
        0x5c => key("\\", "Backslash"),
        0x60 => key("`", "Backquote"),
        // F1..F24
        0xffbe..=0xffd5 => {
            let n = format!("F{}", keysym - 0xffbe + 1);
            key(&n, &n)
        }
        0x30..=0x39 | 0x61..=0x7a => {
            let c = char::from_u32(keysym).unwrap_or('?').to_ascii_uppercase();
            key(&c.to_string(), &c.to_string())
        }
        _ => KeyInfo {
            name: format!("Key {keycode}"),
            modifier: None,
            token: None,
        },
    }
}

/// Every keycode's first keysym, described.
fn load_keymap(conn: &RustConnection) -> HashMap<u8, KeyInfo> {
    let setup = conn.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let mut map = HashMap::new();
    let Ok(reply) = conn
        .get_keyboard_mapping(min, max - min + 1)
        .map_err(|e| e.to_string())
        .and_then(|c| c.reply().map_err(|e| e.to_string()))
    else {
        return map;
    };
    let per = reply.keysyms_per_keycode.max(1) as usize;
    for (i, syms) in reply.keysyms.chunks(per).enumerate() {
        let keycode = min + i as u8;
        if let Some(&sym) = syms.iter().find(|&&s| s != 0) {
            map.insert(keycode, describe(sym, keycode));
        }
    }
    map
}

// ---------------------------------------------------------------------------
// The tap
// ---------------------------------------------------------------------------

enum RawCapture {
    /// Non-modifier press; `mods` are the modifiers held with it.
    Key { keycode: u8, mods: Vec<Modifier> },
    /// Bare modifier press-then-release.
    Modifier { keycode: u8 },
}

struct CaptureState {
    tx: mpsc::Sender<RawCapture>,
    /// Modifier pressed but not yet released (resolves on its release).
    candidate: Option<u8>,
}

struct Shared {
    on_hotkey: Box<dyn Fn(HotkeyEdge) + Send + Sync>,
    conn: RustConnection,
    root: Window,
    keymap: Mutex<HashMap<u8, KeyInfo>>,
    /// The bound key and whether it is grabbed (non-modifiers only).
    binding: Mutex<Option<(u8, bool)>>,
    pressed: AtomicBool,
    /// When the bound modifier went down, while it is held.
    held_since: Mutex<Option<Instant>>,
    /// A shortcut was already reported for the current press.
    shortcut_sent: AtomicBool,
    /// Modifier keys currently held, for combos in the recorder.
    held: Mutex<Vec<u8>>,
    capture: Mutex<Option<CaptureState>>,
}

impl Shared {
    fn emit(&self, edge: HotkeyEdge) {
        (self.on_hotkey)(edge);
    }

    fn release(&self) {
        if self.pressed.swap(false, Ordering::SeqCst) {
            self.held_since.lock().unwrap().take();
            self.emit(HotkeyEdge::Up);
        }
    }

    fn info(&self, keycode: u8) -> KeyInfo {
        self.keymap
            .lock()
            .unwrap()
            .get(&keycode)
            .cloned()
            .unwrap_or_else(|| describe(0, keycode))
    }

    fn is_modifier(&self, keycode: u8) -> bool {
        self.info(keycode).modifier.is_some()
    }
}

pub struct KeyTap {
    shared: Arc<Shared>,
}

impl KeyTap {
    /// Connect to the X server and start listening. `on_hotkey` receives the
    /// bound key's press, release and shortcut edges; it must not block.
    pub fn spawn(on_hotkey: Box<dyn Fn(HotkeyEdge) + Send + Sync>) -> Result<Self, String> {
        if session_kind() != SessionKind::X11 {
            return Err("single-key hotkeys need an X11 session; use a key combination".into());
        }
        let (conn, screen) =
            x11rb::connect(None).map_err(|e| format!("could not connect to X: {e}"))?;
        let root = conn.setup().roots[screen].root;
        let version = conn
            .xinput_xi_query_version(2, 2)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| format!("the X server has no XInput2: {e}"))?;
        if (version.major_version, version.minor_version) < (2, 1) {
            return Err(format!(
                "XInput {}.{} is too old for raw key events",
                version.major_version, version.minor_version
            ));
        }
        conn.xinput_xi_select_events(
            root,
            &[xinput::EventMask {
                deviceid: xinput::Device::ALL_MASTER.into(),
                mask: vec![
                    xinput::XIEventMask::RAW_KEY_PRESS
                        | xinput::XIEventMask::RAW_KEY_RELEASE
                        | xinput::XIEventMask::RAW_BUTTON_PRESS,
                ],
            }],
        )
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| format!("could not listen for keys: {e}"))?;

        let keymap = load_keymap(&conn);
        let shared = Arc::new(Shared {
            on_hotkey,
            conn,
            root,
            keymap: Mutex::new(keymap),
            binding: Mutex::new(None),
            pressed: AtomicBool::new(false),
            held_since: Mutex::new(None),
            shortcut_sent: AtomicBool::new(false),
            held: Mutex::new(Vec::new()),
            capture: Mutex::new(None),
        });
        let thread_shared = shared.clone();
        std::thread::Builder::new()
            .name("una-keytap".into())
            .spawn(move || run(&thread_shared))
            .map_err(|e| e.to_string())?;
        Ok(Self { shared })
    }

    /// Replace the bound keycode (None disables native matching). Clears
    /// any believed-pressed state so a binding change mid-hold cannot wedge.
    pub fn set_binding(&self, keycode: Option<u16>) {
        let shared = &self.shared;
        let mut binding = shared.binding.lock().unwrap();
        if let Some((old, true)) = *binding {
            let _ = shared.conn.ungrab_key(old, shared.root, ModMask::ANY);
        }
        *binding = None;
        BOUND.store(0, Ordering::SeqCst);
        if let Some(kc) = keycode.and_then(|k| u8::try_from(k).ok()) {
            // A key that isn't a modifier is the Mac's consumed key: grab it
            // so it stops reaching the focused app. Someone else holding the
            // grab (the desktop's own shortcut) only costs that.
            let grabbed = !shared.is_modifier(kc)
                && shared
                    .conn
                    .grab_key(true, shared.root, ModMask::ANY, kc, GrabMode::ASYNC, GrabMode::ASYNC)
                    .ok()
                    .and_then(|c| c.check().ok())
                    .is_some();
            if !shared.is_modifier(kc) && !grabbed {
                tracing_warn(&format!(
                    "key {kc} is taken by another app; una hears it but can't keep it from them"
                ));
            }
            *binding = Some((kc, grabbed));
            BOUND.store(kc as u32, Ordering::SeqCst);
        }
        let _ = shared.conn.flush();
        drop(binding);
        shared.release();
    }

    /// Block until the next key press resolves or `timeout` elapses: a bare
    /// modifier resolves on its release, any other key on its press (with the
    /// modifiers held as a combo). Plain Escape cancels. Call from a worker.
    pub fn capture_next(&self, timeout: Duration) -> Result<CapturedKey, CaptureError> {
        let (tx, rx) = mpsc::channel::<RawCapture>();
        *self.shared.capture.lock().unwrap() = Some(CaptureState {
            tx,
            candidate: None,
        });
        let raw = match rx.recv_timeout(timeout) {
            Ok(raw) => raw,
            Err(RecvTimeoutError::Timeout) => {
                self.shared.capture.lock().unwrap().take();
                return Err(CaptureError::Timeout);
            }
            Err(RecvTimeoutError::Disconnected) => return Err(CaptureError::Cancelled),
        };
        Ok(self.resolve(raw))
    }

    /// Abort a pending [`Self::capture_next`] (it returns `Cancelled`).
    pub fn cancel_capture(&self) {
        self.shared.capture.lock().unwrap().take();
    }

    fn resolve(&self, raw: RawCapture) -> CapturedKey {
        match raw {
            RawCapture::Modifier { keycode } => CapturedKey {
                keycode: keycode as u16,
                name: self.shared.info(keycode).name,
                is_modifier: true,
                combo: None,
            },
            RawCapture::Key { keycode, mods } => {
                let info = self.shared.info(keycode);
                let combo = (!mods.is_empty())
                    .then_some(info.token.as_deref())
                    .flatten()
                    .map(|token| {
                        let mut parts = Vec::new();
                        for (m, label) in [
                            (Modifier::Ctrl, "Ctrl"),
                            (Modifier::Alt, "Alt"),
                            (Modifier::Shift, "Shift"),
                            (Modifier::Super, "Super"),
                        ] {
                            if mods.contains(&m) {
                                parts.push(label);
                            }
                        }
                        parts.push(token);
                        parts.join("+")
                    });
                CapturedKey {
                    keycode: keycode as u16,
                    name: info.name,
                    is_modifier: false,
                    combo,
                }
            }
        }
    }
}

fn tracing_warn(msg: &str) {
    // una-platform has no logging dependency; stderr reaches the app's log.
    eprintln!("una-platform: {msg}");
}

fn run(shared: &Shared) {
    loop {
        let event = match shared.conn.wait_for_event() {
            Ok(event) => event,
            Err(e) => {
                tracing_warn(&format!("key listener lost the X connection: {e}"));
                shared.release();
                return;
            }
        };
        match event {
            Event::XinputRawKeyPress(e) => {
                let repeat = e.flags.contains(xinput::KeyEventFlags::KEY_REPEAT);
                if let Ok(kc) = u8::try_from(e.detail) {
                    on_key(shared, kc, true, repeat);
                }
            }
            Event::XinputRawKeyRelease(e) => {
                if let Ok(kc) = u8::try_from(e.detail) {
                    on_key(shared, kc, false, false);
                }
            }
            Event::XinputRawButtonPress(_) => note_other_input(shared),
            // The layout changed: names and modifiers may have moved.
            Event::MappingNotify(_) => {
                let keymap = load_keymap(&shared.conn);
                *shared.keymap.lock().unwrap() = keymap;
            }
            _ => {}
        }
    }
}

fn on_key(shared: &Shared, keycode: u8, down: bool, repeat: bool) {
    let info = shared.info(keycode);
    let is_modifier = info.modifier.is_some();
    if is_modifier {
        let mut held = shared.held.lock().unwrap();
        held.retain(|&k| k != keycode);
        if down {
            held.push(keycode);
        }
    }

    // ---- One-shot capture (takes precedence over matching) ---------------
    {
        let mut guard = shared.capture.lock().unwrap();
        if guard.is_some() {
            if repeat {
                return;
            }
            if is_modifier {
                if down {
                    if let Some(cap) = guard.as_mut() {
                        cap.candidate = Some(keycode);
                    }
                } else if guard.as_ref().is_some_and(|c| c.candidate == Some(keycode)) {
                    if let Some(cap) = guard.take() {
                        let _ = cap.tx.send(RawCapture::Modifier { keycode });
                    }
                }
            } else if down {
                let mods: Vec<Modifier> = shared
                    .held
                    .lock()
                    .unwrap()
                    .iter()
                    .filter_map(|&k| shared.info(k).modifier)
                    .collect();
                if info.token.as_deref() == Some("Escape") && mods.is_empty() {
                    // Dropping the sender makes capture_next return Cancelled.
                    guard.take();
                } else if let Some(cap) = guard.take() {
                    let _ = cap.tx.send(RawCapture::Key { keycode, mods });
                }
            }
            return;
        }
    }

    // ---- Matching ---------------------------------------------------------
    let Some((bound, _)) = *shared.binding.lock().unwrap() else {
        return;
    };
    if keycode != bound {
        if down && !repeat {
            note_other_input(shared);
        }
        return;
    }
    if down {
        if !repeat && !shared.pressed.swap(true, Ordering::SeqCst) {
            if is_modifier {
                *shared.held_since.lock().unwrap() = Some(Instant::now());
                shared.shortcut_sent.store(false, Ordering::SeqCst);
            }
            shared.emit(HotkeyEdge::Down);
        }
    } else {
        shared.release();
    }
}

/// A key or click while the bound modifier is held: within
/// [`SHORTCUT_WINDOW`] of the press, the modifier is being used in a
/// shortcut, so report it (once per press).
fn note_other_input(shared: &Shared) {
    if !shared.pressed.load(Ordering::SeqCst) {
        return;
    }
    let Some(since) = *shared.held_since.lock().unwrap() else {
        return; // a non-modifier key: other keys don't matter
    };
    if since.elapsed() <= SHORTCUT_WINDOW && !shared.shortcut_sent.swap(true, Ordering::SeqCst) {
        shared.emit(HotkeyEdge::Shortcut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_are_named_by_side() {
        let right_ctrl = describe(0xffe4, 105);
        assert_eq!(right_ctrl.name, "Right Ctrl");
        assert_eq!(right_ctrl.modifier, Some(Modifier::Ctrl));
        assert_eq!(describe(0xffeb, 133).modifier, Some(Modifier::Super));
    }

    #[test]
    fn keys_carry_the_plugins_tokens() {
        assert_eq!(describe(0x20, 65).token.as_deref(), Some("Space"));
        assert_eq!(describe(0x61, 38).token.as_deref(), Some("A"));
        assert_eq!(describe(0xffca, 191).token.as_deref(), Some("F13"));
        assert_eq!(describe(0x1234_5678, 250).name, "Key 250");
    }
}
