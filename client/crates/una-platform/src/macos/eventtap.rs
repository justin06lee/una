//! Active CGEventTap-based hotkey backend.
//!
//! A single `EventTap` owns a dedicated thread running a CFRunLoop with an
//! ACTIVE tap (`kCGHIDEventTap` + `kCGEventTapOptionDefault`) on
//! keyDown/keyUp/flagsChanged. Active taps work with the Accessibility
//! permission the app already requires (listen-only taps would need the
//! separate Input Monitoring permission instead).
//!
//! Two jobs share the tap:
//!
//! - **Runtime matching**: when a binding keycode is set via [`EventTap::
//!   set_binding`], matching keyDown/keyUp events are CONSUMED (returned as
//!   `Drop`) and reported as press/release through the `on_hotkey` callback.
//!   Bare modifier keys (Fn, Right ⌘, …) are matched on flagsChanged via the
//!   event's keycode field, and the device-dependent flag bits tell right ⌥
//!   from left ⌥; flagsChanged events cannot be meaningfully consumed, so
//!   they pass through. A bound modifier still works as a modifier, so a key
//!   or click shortly after its press means it was part of a shortcut
//!   (⌘-Tab, ⌥-←, fn-⌫), reported as [`HotkeyEdge::Shortcut`].
//! - **One-shot capture**: [`EventTap::capture_next`] arms the tap so the
//!   next key press resolves to a [`CapturedKey`] for the settings recorder.
//!   Non-modifier keys resolve (and are consumed) on keyDown, carrying any
//!   held modifier flags so combos can be formed. Bare modifiers resolve on
//!   release (press-then-release with no other key), because resolving on
//!   the press would make combos impossible to record.
//!
//! The callback is fast and panic-free: it only reads two atomics in the
//! common pass-through case, and all work is wrapped in `catch_unwind`.
//! `kCGEventTapDisabledByTimeout`/`ByUserInput` re-enable the tap in place —
//! no polling, so an idle tap costs nothing.
//!
//! A watchdog thread doubles as a stuck-key guard: every 500ms while the
//! bound key is believed pressed, it queries the physical key state
//! (`CGEventSourceKeyState`, with `CGEventSourceFlagsState` as a secondary
//! signal for modifiers) and force-releases if the key is actually up (e.g.
//! the release was swallowed by secure input or a disabled tap).

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use core_foundation::base::TCFType;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CallbackResult, EventField,
};

use super::keys;

/// kCGEventSourceStateHIDSystemState.
const HID_STATE: i32 = 1;

extern "C" {
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    fn CGEventSourceKeyState(state_id: i32, keycode: u16) -> bool;
    fn CGEventSourceFlagsState(state_id: i32) -> u64;
    fn CFRunLoopStop(rl: *mut c_void);
}

/// What a key press does to text that was just pasted.
///
/// Used by the correction watcher to follow an edit in apps whose text the
/// accessibility API cannot read. Only keyDown events are classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyClass {
    /// Adds a character (any printable key, plus space).
    Insert,
    /// Backspace or forward delete.
    Delete,
    /// Moves the caret or does something unaccountable (arrows, Home/End,
    /// Return, Tab, Escape, and anything held with ⌘ or ⌃ — including the
    /// readline motions terminals bind to ⌃A/⌃E/⌃W/⌃U).
    Navigate,
}

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
    /// Human-readable name ("Fn", "Right ⌘", "F5", "A", …).
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

enum RawCapture {
    /// Non-modifier keyDown; `flags` are the modifier bits held with it.
    Key { keycode: u16, flags: u64 },
    /// Bare modifier press-then-release.
    Modifier { keycode: u16 },
}

struct CaptureState {
    tx: mpsc::Sender<RawCapture>,
    /// Modifier pressed but not yet released (resolves on its release).
    candidate: Option<u16>,
}

struct Shared {
    on_hotkey: Box<dyn Fn(HotkeyEdge) + Send + Sync>,
    /// Fast path: skip the watcher mutex entirely when nothing is armed.
    watching: AtomicBool,
    on_edit: Mutex<Option<Box<dyn Fn(KeyClass) + Send + Sync>>>,
    /// Bound keycode + 1; 0 means no native binding.
    matcher: AtomicU32,
    /// Whether the bound key is currently believed held.
    pressed: AtomicBool,
    /// When the bound modifier went down, while it is held.
    held_since: Mutex<Option<Instant>>,
    /// A shortcut was already reported for the current press.
    shortcut_sent: AtomicBool,
    capture: Mutex<Option<CaptureState>>,
    mach_port: AtomicPtr<c_void>,
    runloop: AtomicPtr<c_void>,
    alive: AtomicBool,
}

impl Shared {
    fn emit(&self, edge: HotkeyEdge) {
        (self.on_hotkey)(edge);
    }

    /// Report a release of the bound key if it was believed held.
    fn release(&self) {
        if self.pressed.swap(false, Ordering::SeqCst) {
            self.held_since.lock().unwrap().take();
            self.emit(HotkeyEdge::Up);
        }
    }
}

pub struct EventTap {
    shared: Arc<Shared>,
}

impl EventTap {
    /// Spawn the tap thread + watchdog. `on_hotkey` receives the bound key's
    /// press, release and shortcut edges; it must be non-blocking.
    ///
    /// Fails when the tap cannot be created (most commonly: the
    /// Accessibility permission has not been granted).
    pub fn spawn(on_hotkey: Box<dyn Fn(HotkeyEdge) + Send + Sync>) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            on_hotkey,
            watching: AtomicBool::new(false),
            on_edit: Mutex::new(None),
            matcher: AtomicU32::new(0),
            pressed: AtomicBool::new(false),
            held_since: Mutex::new(None),
            shortcut_sent: AtomicBool::new(false),
            capture: Mutex::new(None),
            mach_port: AtomicPtr::new(std::ptr::null_mut()),
            runloop: AtomicPtr::new(std::ptr::null_mut()),
            alive: AtomicBool::new(true),
        });

        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let cb_shared = shared.clone();
        let thread_shared = shared.clone();
        std::thread::Builder::new()
            .name("una-eventtap".into())
            .spawn(move || {
                let tap = CGEventTap::new(
                    CGEventTapLocation::HID,
                    CGEventTapPlacement::HeadInsertEventTap,
                    CGEventTapOptions::Default,
                    vec![
                        CGEventType::KeyDown,
                        CGEventType::KeyUp,
                        CGEventType::FlagsChanged,
                        // Only to spot ⌘-click and friends on a bound
                        // modifier; clicks always pass through.
                        CGEventType::LeftMouseDown,
                        CGEventType::RightMouseDown,
                        CGEventType::OtherMouseDown,
                    ],
                    move |_proxy, etype, event| {
                        catch_unwind(AssertUnwindSafe(|| handle_event(&cb_shared, etype, event)))
                            .unwrap_or(CallbackResult::Keep)
                    },
                );
                let tap = match tap {
                    Ok(tap) => tap,
                    Err(()) => {
                        let _ = ready_tx.send(Err(
                            "could not create the keyboard event tap — is the Accessibility \
                             permission granted? (System Settings > Privacy & Security > \
                             Accessibility)"
                                .into(),
                        ));
                        return;
                    }
                };
                let source = match tap.mach_port().create_runloop_source(0) {
                    Ok(s) => s,
                    Err(()) => {
                        let _ = ready_tx.send(Err("could not create tap runloop source".into()));
                        return;
                    }
                };
                let rl = CFRunLoop::get_current();
                rl.add_source(&source, unsafe { kCFRunLoopCommonModes });
                tap.enable();
                thread_shared.mach_port.store(
                    tap.mach_port().as_concrete_TypeRef() as *mut c_void,
                    Ordering::SeqCst,
                );
                thread_shared
                    .runloop
                    .store(rl.as_concrete_TypeRef() as *mut c_void, Ordering::SeqCst);
                let _ = ready_tx.send(Ok(()));
                CFRunLoop::run_current();
                // Runloop stopped: clear the shared pointers before the tap
                // (and its mach port) are dropped.
                thread_shared
                    .mach_port
                    .store(std::ptr::null_mut(), Ordering::SeqCst);
                thread_shared
                    .runloop
                    .store(std::ptr::null_mut(), Ordering::SeqCst);
                drop(tap);
            })
            .map_err(|e| format!("could not spawn event-tap thread: {e}"))?;

        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err("event-tap thread did not start in time".into()),
        }

        // Stuck-key watchdog: 500ms cadence, only queries while pressed.
        let wd_shared = shared.clone();
        std::thread::Builder::new()
            .name("una-eventtap-watchdog".into())
            .spawn(move || {
                let mut elapsed = Duration::ZERO;
                const STEP: Duration = Duration::from_millis(100);
                const CADENCE: Duration = Duration::from_millis(500);
                while wd_shared.alive.load(Ordering::SeqCst) {
                    std::thread::sleep(STEP);
                    elapsed += STEP;
                    if elapsed < CADENCE {
                        continue;
                    }
                    elapsed = Duration::ZERO;
                    let bound = wd_shared.matcher.load(Ordering::SeqCst);
                    if bound == 0 || !wd_shared.pressed.load(Ordering::SeqCst) {
                        continue;
                    }
                    let kc = (bound - 1) as u16;
                    if !key_physically_down(kc) && wd_shared.pressed.load(Ordering::SeqCst) {
                        tracing_forced_release(kc);
                        wd_shared.release();
                    }
                }
            })
            .map_err(|e| format!("could not spawn watchdog thread: {e}"))?;

        Ok(Self { shared })
    }

    /// Replace the matched keycode (None disables native matching). Clears
    /// any believed-pressed state so a binding change mid-hold cannot wedge.
    pub fn set_binding(&self, keycode: Option<u16>) {
        self.shared.matcher.store(
            keycode
                .map(|kc| keys::canonical(kc) as u32 + 1)
                .unwrap_or(0),
            Ordering::SeqCst,
        );
        self.shared.release();
    }

    /// Block until the next key press resolves (see module docs) or `timeout`
    /// elapses. Pressing plain Escape cancels. Call from a worker thread.
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
        Ok(resolve_capture(raw))
    }

    /// Abort a pending [`Self::capture_next`] (it returns `Cancelled`).
    pub fn cancel_capture(&self) {
        self.shared.capture.lock().unwrap().take();
    }

    /// Report every subsequent key press to `on_edit` until
    /// [`Self::stop_watching_edits`].
    ///
    /// Never consumes events — the keys still reach the focused app. The
    /// callback runs inside the tap callback, so it must not block: send on a
    /// channel and do the work elsewhere.
    pub fn watch_edits(&self, on_edit: Box<dyn Fn(KeyClass) + Send + Sync>) {
        *self.shared.on_edit.lock().unwrap() = Some(on_edit);
        self.shared.watching.store(true, Ordering::SeqCst);
    }

    pub fn stop_watching_edits(&self) {
        self.shared.watching.store(false, Ordering::SeqCst);
        self.shared.on_edit.lock().unwrap().take();
    }
}

impl Drop for EventTap {
    fn drop(&mut self) {
        self.shared.alive.store(false, Ordering::SeqCst);
        let rl = self.shared.runloop.load(Ordering::SeqCst);
        if !rl.is_null() {
            unsafe { CFRunLoopStop(rl) };
        }
    }
}

/// Physical key state, with the flags-state register as a secondary signal
/// for modifiers (Caps Lock excluded: its flag bit is the lock state, not
/// the key state).
fn key_physically_down(kc: u16) -> bool {
    if unsafe { CGEventSourceKeyState(HID_STATE, kc) } {
        return true;
    }
    if kc != keys::KC_CAPS_LOCK {
        if let Some(flag) = keys::modifier_flag(kc) {
            let state = unsafe { CGEventSourceFlagsState(HID_STATE) };
            if let Some((side, pair)) = keys::side_flags(kc) {
                if state & pair != 0 {
                    return state & side != 0;
                }
            }
            // No side bits: errs toward "held" when the twin (other-side)
            // modifier is down; real releases still arrive via flagsChanged.
            return state & flag.bits() != 0;
        }
    }
    false
}

/// Virtual keycodes that move the caret rather than change text.
const KC_RETURN: u16 = 36;
const KC_TAB: u16 = 48;
const KC_KEYPAD_ENTER: u16 = 76;
const KC_HOME: u16 = 115;
const KC_PAGE_UP: u16 = 116;
const KC_FORWARD_DELETE: u16 = 117;
const KC_END: u16 = 119;
const KC_PAGE_DOWN: u16 = 121;
const KC_DELETE: u16 = 51;
const KC_ARROW_FIRST: u16 = 123;
const KC_ARROW_LAST: u16 = 126;

fn classify_edit_key(keycode: u16, flags: CGEventFlags) -> KeyClass {
    // ⌘ and ⌃ combos can do anything at all — undo, select-all, a readline
    // kill — so they end the run of accountable keystrokes.
    if flags.contains(CGEventFlags::CGEventFlagCommand)
        || flags.contains(CGEventFlags::CGEventFlagControl)
    {
        return KeyClass::Navigate;
    }
    match keycode {
        KC_DELETE | KC_FORWARD_DELETE => KeyClass::Delete,
        KC_ARROW_FIRST..=KC_ARROW_LAST
        | KC_HOME
        | KC_END
        | KC_PAGE_UP
        | KC_PAGE_DOWN
        | KC_TAB
        | KC_RETURN
        | KC_KEYPAD_ENTER
        | keys::KC_ESCAPE => KeyClass::Navigate,
        kc if keys::is_modifier(kc) => KeyClass::Navigate,
        _ => KeyClass::Insert,
    }
}

fn tracing_forced_release(kc: u16) {
    // una-platform has no tracing dependency; stderr is fine for a rare
    // safety-net event.
    eprintln!("una: watchdog released stuck hotkey (keycode {kc})");
}

// ---------------------------------------------------------------------------
// Tap callback
// ---------------------------------------------------------------------------

const MODIFIER_MASK: u64 = CGEventFlags::CGEventFlagCommand.bits()
    | CGEventFlags::CGEventFlagAlternate.bits()
    | CGEventFlags::CGEventFlagControl.bits()
    | CGEventFlags::CGEventFlagShift.bits();

fn handle_event(shared: &Shared, etype: CGEventType, event: &CGEvent) -> CallbackResult {
    match etype {
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
            let port = shared.mach_port.load(Ordering::SeqCst);
            if !port.is_null() {
                unsafe { CGEventTapEnable(port, true) };
            }
            return CallbackResult::Keep;
        }
        CGEventType::KeyDown | CGEventType::KeyUp | CGEventType::FlagsChanged => {}
        CGEventType::LeftMouseDown | CGEventType::RightMouseDown | CGEventType::OtherMouseDown => {
            note_other_input(shared);
            return CallbackResult::Keep;
        }
        _ => return CallbackResult::Keep,
    }

    let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;

    // ---- One-shot capture (takes precedence over runtime matching) --------
    {
        let mut guard = shared.capture.lock().unwrap();
        if guard.is_some() {
            match etype {
                CGEventType::KeyDown if keycode == keys::KC_GLOBE => {
                    // The 🌐 keyDown a Fn press sends alongside its
                    // flagsChanged: it is Fn, so keep waiting for the Fn
                    // release instead of recording "key 179".
                    if let Some(cap) = guard.as_mut() {
                        cap.candidate = Some(keys::KC_FN);
                    }
                    return CallbackResult::Drop;
                }
                CGEventType::KeyDown => {
                    let flags = event.get_flags().bits() & MODIFIER_MASK;
                    if keycode == keys::KC_ESCAPE && flags == 0 {
                        // Plain Escape cancels: drop the sender so the
                        // waiting capture_next returns Cancelled.
                        guard.take();
                    } else if let Some(cap) = guard.take() {
                        let _ = cap.tx.send(RawCapture::Key { keycode, flags });
                    }
                    // Never leak the captured (or cancelling) press.
                    return CallbackResult::Drop;
                }
                CGEventType::KeyUp => {
                    // Swallow releases while armed so half of a consumed
                    // press doesn't reach the focused app.
                    return CallbackResult::Drop;
                }
                CGEventType::FlagsChanged => {
                    if keys::is_modifier(keycode) {
                        if modifier_down_now(keycode, event.get_flags()) {
                            if let Some(cap) = guard.as_mut() {
                                cap.candidate = Some(keycode);
                            }
                        } else if guard.as_ref().is_some_and(|c| c.candidate == Some(keycode)) {
                            if let Some(cap) = guard.take() {
                                let _ = cap.tx.send(RawCapture::Modifier { keycode });
                            }
                        }
                    }
                    return CallbackResult::Keep;
                }
                _ => return CallbackResult::Keep,
            }
        }
    }

    // ---- Correction watcher ----------------------------------------------
    // Observational only: classify the press and pass it straight through.
    // The Globe keyDown of a Fn press changes no text.
    if shared.watching.load(Ordering::SeqCst)
        && matches!(etype, CGEventType::KeyDown)
        && keycode != keys::KC_GLOBE
    {
        let class = classify_edit_key(keycode, event.get_flags());
        if let Some(on_edit) = shared.on_edit.lock().unwrap().as_ref() {
            on_edit(class);
        }
    }

    // ---- Runtime matching -------------------------------------------------
    let bound = shared.matcher.load(Ordering::SeqCst);
    if bound == 0 {
        return CallbackResult::Keep;
    }
    let bound_kc = (bound - 1) as u16;

    if keys::is_modifier(bound_kc) {
        match etype {
            CGEventType::FlagsChanged if keycode == bound_kc => {
                if modifier_down_now(bound_kc, event.get_flags()) {
                    if !shared.pressed.swap(true, Ordering::SeqCst) {
                        *shared.held_since.lock().unwrap() = Some(Instant::now());
                        shared.shortcut_sent.store(false, Ordering::SeqCst);
                        shared.emit(HotkeyEdge::Down);
                    }
                } else {
                    shared.release();
                }
            }
            // The Globe keyDown is Fn's own, not a second key.
            CGEventType::KeyDown if keycode != keys::KC_GLOBE => note_other_input(shared),
            _ => {}
        }
        // flagsChanged can't be consumed meaningfully, and a bound modifier
        // stays a modifier for everything else; pass through.
        return CallbackResult::Keep;
    }

    if keycode != bound_kc || matches!(etype, CGEventType::FlagsChanged) {
        return CallbackResult::Keep;
    }
    match etype {
        CGEventType::KeyDown => {
            let autorepeat =
                event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) != 0;
            // Consume repeats without re-emitting: the FSM debounces, but a
            // repeat must not leak into the focused app either.
            if !autorepeat && !shared.pressed.swap(true, Ordering::SeqCst) {
                shared.emit(HotkeyEdge::Down);
            }
            CallbackResult::Drop
        }
        CGEventType::KeyUp => {
            shared.release();
            CallbackResult::Drop
        }
        _ => CallbackResult::Keep,
    }
}

/// A key or click while the bound modifier is held: within
/// [`SHORTCUT_WINDOW`] of the press, the modifier is being used in a
/// shortcut, so report it (once per press).
fn note_other_input(shared: &Shared) {
    if !shared.pressed.load(Ordering::SeqCst) {
        return;
    }
    let bound = shared.matcher.load(Ordering::SeqCst);
    if bound == 0 || !keys::is_modifier((bound - 1) as u16) {
        return;
    }
    let Some(since) = *shared.held_since.lock().unwrap() else {
        return;
    };
    if since.elapsed() <= SHORTCUT_WINDOW && !shared.shortcut_sent.swap(true, Ordering::SeqCst) {
        shared.emit(HotkeyEdge::Shortcut);
    }
}

/// Whether the modifier key `kc` is physically down right after this
/// flagsChanged event. The event's device-dependent bits say exactly which
/// side of a pair is down (right ⌥ vs left ⌥ share `CGEventFlagAlternate`);
/// `CGEventSourceKeyState` covers sources that don't set them, and the
/// event's own flag bit is the fallback for keys where the key-state
/// register is unreliable (Fn on some keyboards).
fn modifier_down_now(kc: u16, event_flags: CGEventFlags) -> bool {
    let bits = event_flags.bits();
    if let (Some((side, pair)), Some(class)) = (keys::side_flags(kc), keys::modifier_flag(kc)) {
        if bits & class.bits() == 0 {
            return false; // neither key of the pair is held
        }
        if bits & pair != 0 {
            return bits & side != 0;
        }
    }
    if unsafe { CGEventSourceKeyState(HID_STATE, kc) } {
        return true;
    }
    if kc == keys::KC_FN {
        return event_flags.contains(CGEventFlags::CGEventFlagSecondaryFn);
    }
    false
}

// ---------------------------------------------------------------------------
// Capture resolution (runs on the capturer's thread, not in the callback)
// ---------------------------------------------------------------------------

fn resolve_capture(raw: RawCapture) -> CapturedKey {
    match raw {
        RawCapture::Modifier { keycode } => CapturedKey {
            keycode,
            name: keys::key_name(keycode),
            is_modifier: true,
            combo: None,
        },
        RawCapture::Key { keycode, flags } => {
            let name = keys::key_name(keycode);
            let combo = if flags != 0 {
                keys::combo_token(keycode).map(|token| {
                    let mut parts: Vec<&str> = Vec::with_capacity(5);
                    if flags & CGEventFlags::CGEventFlagControl.bits() != 0 {
                        parts.push("Ctrl");
                    }
                    if flags & CGEventFlags::CGEventFlagAlternate.bits() != 0 {
                        parts.push("Alt");
                    }
                    if flags & CGEventFlags::CGEventFlagShift.bits() != 0 {
                        parts.push("Shift");
                    }
                    if flags & CGEventFlags::CGEventFlagCommand.bits() != 0 {
                        parts.push("Super");
                    }
                    parts.push(token);
                    parts.join("+")
                })
            } else {
                None
            };
            CapturedKey {
                keycode,
                name,
                is_modifier: false,
                combo,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_edit_key, KeyClass};
    use core_graphics::event::CGEventFlags;

    /// Backspace is the signal the whole correction flow hangs on.
    #[test]
    fn delete_keys_are_deletes() {
        assert_eq!(
            classify_edit_key(super::KC_DELETE, CGEventFlags::empty()),
            KeyClass::Delete
        );
        assert_eq!(
            classify_edit_key(super::KC_FORWARD_DELETE, CGEventFlags::empty()),
            KeyClass::Delete
        );
    }

    #[test]
    fn letters_and_space_insert() {
        for kc in [0u16 /* a */, 49 /* space */, 18 /* 1 */] {
            assert_eq!(classify_edit_key(kc, CGEventFlags::empty()), KeyClass::Insert);
        }
    }

    #[test]
    fn caret_movers_navigate() {
        for kc in [123u16, 126, super::KC_HOME, super::KC_RETURN, super::KC_TAB] {
            assert_eq!(
                classify_edit_key(kc, CGEventFlags::empty()),
                KeyClass::Navigate
            );
        }
    }

    /// ⌘Z, ⌃W and friends can rewrite the line arbitrarily, so a held ⌘/⌃
    /// voids the count no matter which key it is combined with.
    #[test]
    fn command_and_control_combos_navigate() {
        assert_eq!(
            classify_edit_key(6 /* z */, CGEventFlags::CGEventFlagCommand),
            KeyClass::Navigate
        );
        assert_eq!(
            classify_edit_key(13 /* w */, CGEventFlags::CGEventFlagControl),
            KeyClass::Navigate
        );
        assert_eq!(
            classify_edit_key(super::KC_DELETE, CGEventFlags::CGEventFlagCommand),
            KeyClass::Navigate
        );
    }

    use super::*;

    /// End-to-end capture check with a synthetic HID event. Requires the
    /// Accessibility permission for the test runner, so it is ignored by
    /// default; run manually with
    /// `cargo test -p una-platform -- --ignored --test-threads=1`
    /// (serially: two live HID taps posting the same key interfere).
    ///
    /// F20 (keycode 90) is used because no app reacts to it if the tap
    /// somehow fails to consume it.
    #[test]
    #[ignore = "requires Accessibility permission and a real HID event tap"]
    fn capture_roundtrip_with_posted_event() {
        use core_graphics::event::{CGEvent, CGEventTapLocation};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        const KC_F20: u16 = 90;

        let tap = EventTap::spawn(Box::new(|_| {})).expect("event tap (accessibility granted?)");

        let handle = {
            let shared = tap.shared.clone();
            std::thread::spawn(move || {
                let t = EventTap { shared };
                let r = t.capture_next(Duration::from_secs(5));
                std::mem::forget(t); // don't stop the runloop from the clone
                r
            })
        };
        // Let the capture arm before posting.
        std::thread::sleep(Duration::from_millis(200));

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
        let down = CGEvent::new_keyboard_event(source.clone(), KC_F20, true).unwrap();
        down.post(CGEventTapLocation::HID);
        let up = CGEvent::new_keyboard_event(source, KC_F20, false).unwrap();
        up.post(CGEventTapLocation::HID);

        let captured = handle.join().unwrap().expect("capture should resolve");
        assert_eq!(captured.keycode, KC_F20);
        assert_eq!(captured.name, "F20");
        assert!(!captured.is_modifier);
        assert_eq!(captured.combo, None);
    }

    /// Runtime-matching check: bind F20, post down/up, expect a press edge
    /// then a release edge (and key repeats to be swallowed silently).
    #[test]
    #[ignore = "requires Accessibility permission and a real HID event tap"]
    fn runtime_match_emits_edges_for_posted_events() {
        use core_graphics::event::{CGEvent, CGEventTapLocation};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        const KC_F20: u16 = 90;

        let (tx, rx) = mpsc::channel::<HotkeyEdge>();
        let tap = EventTap::spawn(Box::new(move |edge| {
            let _ = tx.send(edge);
        }))
        .expect("event tap (accessibility granted?)");
        tap.set_binding(Some(KC_F20));
        std::thread::sleep(Duration::from_millis(100));

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
        let down = CGEvent::new_keyboard_event(source.clone(), KC_F20, true).unwrap();
        down.post(CGEventTapLocation::HID);
        let up = CGEvent::new_keyboard_event(source, KC_F20, false).unwrap();
        up.post(CGEventTapLocation::HID);

        assert_eq!(
            rx.recv_timeout(Duration::from_secs(3)),
            Ok(HotkeyEdge::Down)
        );
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)), Ok(HotkeyEdge::Up));
        // No stray third edge.
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        tap.set_binding(None);
    }

    const ALT: u64 = 0x0008_0000; // CGEventFlagAlternate
    const L_OPT: u64 = 0x20;
    const R_OPT: u64 = 0x40;

    /// Right ⌥ and left ⌥ share the Alternate flag; the side bits decide.
    #[test]
    fn side_bits_tell_right_option_from_left() {
        use super::keys::{KC_LEFT_OPT, KC_RIGHT_OPT};
        let f = CGEventFlags::from_bits_retain;
        assert!(modifier_down_now(KC_RIGHT_OPT, f(ALT | R_OPT)));
        assert!(!modifier_down_now(KC_LEFT_OPT, f(ALT | R_OPT)));
        // Left held, right just released.
        assert!(!modifier_down_now(KC_RIGHT_OPT, f(ALT | L_OPT)));
        assert!(modifier_down_now(KC_LEFT_OPT, f(ALT | L_OPT)));
        // Both released.
        assert!(!modifier_down_now(KC_RIGHT_OPT, f(0)));
    }

    /// Post a flagsChanged for `keycode` carrying `flags`, as a real
    /// modifier press or release would.
    fn post_modifier(keycode: u16, flags: u64) {
        use core_graphics::event::{CGEvent, CGEventTapLocation};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
        let ev = CGEvent::new_keyboard_event(source, keycode, flags != 0).unwrap();
        ev.set_type(CGEventType::FlagsChanged);
        ev.set_flags(CGEventFlags::from_bits_retain(flags));
        ev.post(CGEventTapLocation::HID);
    }

    fn post_key(keycode: u16, down: bool, flags: u64) {
        use core_graphics::event::{CGEvent, CGEventTapLocation};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
        let ev = CGEvent::new_keyboard_event(source, keycode, down).unwrap();
        ev.set_flags(CGEventFlags::from_bits_retain(flags));
        ev.post(CGEventTapLocation::HID);
    }

    /// Recording a bare right ⌥ resolves to right ⌥, not left.
    #[test]
    #[ignore = "requires Accessibility permission and a real HID event tap"]
    fn capture_records_right_option() {
        use super::keys::KC_RIGHT_OPT;
        let tap = EventTap::spawn(Box::new(|_| {})).expect("event tap (accessibility granted?)");
        let handle = {
            let shared = tap.shared.clone();
            std::thread::spawn(move || {
                let t = EventTap { shared };
                let r = t.capture_next(Duration::from_secs(5));
                std::mem::forget(t);
                r
            })
        };
        std::thread::sleep(Duration::from_millis(200));
        post_modifier(KC_RIGHT_OPT, ALT | R_OPT);
        post_modifier(KC_RIGHT_OPT, 0);
        let captured = handle.join().unwrap().expect("capture should resolve");
        assert_eq!(captured.keycode, KC_RIGHT_OPT);
        assert_eq!(captured.name, "Right ⌥");
        assert!(captured.is_modifier);
    }

    /// Bound to right ⌥: its press and release are edges, left ⌥ is ignored,
    /// and right ⌥ + another key reports a shortcut between the two.
    #[test]
    #[ignore = "requires Accessibility permission and a real HID event tap"]
    fn right_option_alone_matches_and_shortcut_is_reported() {
        use super::keys::{KC_LEFT_OPT, KC_RIGHT_OPT};
        const KC_F20: u16 = 90;

        let (tx, rx) = mpsc::channel::<HotkeyEdge>();
        let tap = EventTap::spawn(Box::new(move |edge| {
            let _ = tx.send(edge);
        }))
        .expect("event tap (accessibility granted?)");
        tap.set_binding(Some(KC_RIGHT_OPT));
        std::thread::sleep(Duration::from_millis(100));
        let recv = || rx.recv_timeout(Duration::from_secs(2));

        // Right ⌥ alone.
        post_modifier(KC_RIGHT_OPT, ALT | R_OPT);
        post_modifier(KC_RIGHT_OPT, 0);
        assert_eq!(recv(), Ok(HotkeyEdge::Down));
        assert_eq!(recv(), Ok(HotkeyEdge::Up));

        // Left ⌥ alone: nothing.
        post_modifier(KC_LEFT_OPT, ALT | L_OPT);
        post_modifier(KC_LEFT_OPT, 0);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());

        // Left ⌥ held, right ⌥ pressed and released, left released.
        post_modifier(KC_LEFT_OPT, ALT | L_OPT);
        post_modifier(KC_RIGHT_OPT, ALT | L_OPT | R_OPT);
        post_modifier(KC_RIGHT_OPT, ALT | L_OPT);
        post_modifier(KC_LEFT_OPT, 0);
        assert_eq!(recv(), Ok(HotkeyEdge::Down));
        assert_eq!(recv(), Ok(HotkeyEdge::Up));
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());

        // Right ⌥ + F20: a shortcut, reported once, then the release.
        post_modifier(KC_RIGHT_OPT, ALT | R_OPT);
        post_key(KC_F20, true, ALT | R_OPT);
        post_key(KC_F20, false, ALT | R_OPT);
        post_key(KC_F20, true, ALT | R_OPT);
        post_key(KC_F20, false, ALT | R_OPT);
        post_modifier(KC_RIGHT_OPT, 0);
        assert_eq!(recv(), Ok(HotkeyEdge::Down));
        assert_eq!(recv(), Ok(HotkeyEdge::Shortcut));
        assert_eq!(recv(), Ok(HotkeyEdge::Up));
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());

        tap.set_binding(None);
    }
}
