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
//!   event's keycode field; flagsChanged events cannot be meaningfully
//!   consumed, so they pass through.
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
use std::time::Duration;

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
    on_hotkey: Box<dyn Fn(bool) + Send + Sync>,
    /// Bound keycode + 1; 0 means no native binding.
    matcher: AtomicU32,
    /// Whether the bound key is currently believed held.
    pressed: AtomicBool,
    capture: Mutex<Option<CaptureState>>,
    mach_port: AtomicPtr<c_void>,
    runloop: AtomicPtr<c_void>,
    alive: AtomicBool,
}

impl Shared {
    fn emit(&self, down: bool) {
        (self.on_hotkey)(down);
    }
}

pub struct EventTap {
    shared: Arc<Shared>,
}

impl EventTap {
    /// Spawn the tap thread + watchdog. `on_hotkey` receives `true` on press
    /// and `false` on release of the bound key; it must be non-blocking.
    ///
    /// Fails when the tap cannot be created (most commonly: the
    /// Accessibility permission has not been granted).
    pub fn spawn(on_hotkey: Box<dyn Fn(bool) + Send + Sync>) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            on_hotkey,
            matcher: AtomicU32::new(0),
            pressed: AtomicBool::new(false),
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
                    if !key_physically_down(kc) && wd_shared.pressed.swap(false, Ordering::SeqCst) {
                        tracing_forced_release(kc);
                        wd_shared.emit(false);
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
            keycode.map(|kc| kc as u32 + 1).unwrap_or(0),
            Ordering::SeqCst,
        );
        if self.shared.pressed.swap(false, Ordering::SeqCst) {
            self.shared.emit(false);
        }
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
            // Errs toward "held" when the twin (other-side) modifier is
            // down; real releases still arrive via flagsChanged.
            return unsafe { CGEventSourceFlagsState(HID_STATE) } & flag.bits() != 0;
        }
    }
    false
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
        _ => return CallbackResult::Keep,
    }

    let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;

    // ---- One-shot capture (takes precedence over runtime matching) --------
    {
        let mut guard = shared.capture.lock().unwrap();
        if guard.is_some() {
            match etype {
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

    // ---- Runtime matching -------------------------------------------------
    let bound = shared.matcher.load(Ordering::SeqCst);
    if bound == 0 {
        return CallbackResult::Keep;
    }
    let bound_kc = (bound - 1) as u16;

    if keys::is_modifier(bound_kc) {
        if matches!(etype, CGEventType::FlagsChanged) && keycode == bound_kc {
            let down = modifier_down_now(bound_kc, event.get_flags());
            if shared.pressed.swap(down, Ordering::SeqCst) != down {
                shared.emit(down);
            }
        }
        // flagsChanged can't be consumed meaningfully; pass through.
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
                shared.emit(true);
            }
            CallbackResult::Drop
        }
        CGEventType::KeyUp => {
            if shared.pressed.swap(false, Ordering::SeqCst) {
                shared.emit(false);
            }
            CallbackResult::Drop
        }
        _ => CallbackResult::Keep,
    }
}

/// Whether the modifier key `kc` is physically down right after this
/// flagsChanged event. `CGEventSourceKeyState` distinguishes left/right
/// twins sharing one flag bit; the event's own flag bit is the fallback for
/// keys where the key-state register is unreliable (Fn on some keyboards).
fn modifier_down_now(kc: u16, event_flags: CGEventFlags) -> bool {
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

        let (tx, rx) = mpsc::channel::<bool>();
        let tap = EventTap::spawn(Box::new(move |down| {
            let _ = tx.send(down);
        }))
        .expect("event tap (accessibility granted?)");
        tap.set_binding(Some(KC_F20));
        std::thread::sleep(Duration::from_millis(100));

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
        let down = CGEvent::new_keyboard_event(source.clone(), KC_F20, true).unwrap();
        down.post(CGEventTapLocation::HID);
        let up = CGEvent::new_keyboard_event(source, KC_F20, false).unwrap();
        up.post(CGEventTapLocation::HID);

        assert_eq!(rx.recv_timeout(Duration::from_secs(3)), Ok(true));
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)), Ok(false));
        // No stray third edge.
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
        tap.set_binding(None);
    }
}
