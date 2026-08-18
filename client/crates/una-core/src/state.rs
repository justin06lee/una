//! The dictation state machine.
//!
//! The FSM itself is a pure function: [`step`] takes the current [`Machine`]
//! and an [`Event`] and returns the next machine plus a list of [`Effect`]s
//! for the caller to execute. All timing information travels inside events
//! (`at:` timestamps), so the function is fully deterministic and unit-tested
//! below.
//!
//! [`Controller`] is the tokio actor that owns a `Machine`, receives
//! [`Command`]s over an mpsc channel, executes effects through an
//! [`EffectRunner`] trait object, and broadcasts [`Snapshot`]s on every state
//! change.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

/// A press-then-release faster than this latches hybrid mode into toggle.
pub const HYBRID_LATCH_WINDOW: Duration = Duration::from_millis(250);
/// Utterances shorter than this are discarded silently.
pub const MIN_UTTERANCE: Duration = Duration::from_millis(300);
/// Hard cap: recordings auto-finalize after this long.
pub const MAX_UTTERANCE: Duration = Duration::from_secs(5 * 60);
/// The Done state auto-dismisses after this long.
pub const DONE_DISMISS: Duration = Duration::from_millis(900);
/// The Error state auto-dismisses after this long.
pub const ERROR_DISMISS: Duration = Duration::from_millis(2500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HotkeyMode {
    Hold,
    Toggle,
    Hybrid,
}

impl HotkeyMode {
    pub fn parse(s: &str) -> Self {
        match s {
            "hold" => Self::Hold,
            "toggle" => Self::Toggle,
            _ => Self::Hybrid,
        }
    }
}

/// Broad classification of a dictation failure, used by the HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrKind {
    /// Could not reach the server at all.
    Connect,
    Timeout,
    /// The server answered with an error status.
    Server,
    /// The response could not be decoded.
    Decode,
    Audio,
    Inject,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Idle,
    Recording {
        session: u64,
        latched: bool,
        started_at: Instant,
        pressed_at: Instant,
    },
    /// Waiting for the finalized audio and/or the server response.
    Transcribing {
        session: u64,
    },
    Inserting {
        session: u64,
    },
    Done {
        at: Instant,
    },
    Error {
        kind: ErrKind,
        message: String,
        retryable: bool,
        at: Instant,
    },
}

#[derive(Debug, Clone)]
pub enum Event {
    HotkeyDown {
        at: Instant,
    },
    HotkeyUp {
        at: Instant,
    },
    /// Start/stop from the tray, CLI, or IPC socket.
    Toggle {
        at: Instant,
    },
    Cancel,
    /// The audio engine finished encoding the utterance.
    AudioFinalized {
        session: u64,
        wav: Vec<u8>,
        duration: Duration,
    },
    AudioFailed {
        session: u64,
        message: String,
        at: Instant,
    },
    UploadOk {
        session: u64,
        text: String,
    },
    UploadErr {
        session: u64,
        kind: ErrKind,
        message: String,
        retryable: bool,
        at: Instant,
    },
    InsertOk {
        session: u64,
        at: Instant,
    },
    InsertErr {
        session: u64,
        message: String,
        at: Instant,
    },
    /// Re-upload a previously spooled utterance.
    RetryUpload {
        wav: Vec<u8>,
        duration: Duration,
    },
    /// Periodic timer used for auto-finalize and auto-dismiss.
    Tick {
        at: Instant,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    ShowHud,
    HideHud,
    StartRecording {
        session: u64,
    },
    /// Finalize the recording; the audio engine answers with
    /// [`Event::AudioFinalized`].
    StopRecording {
        session: u64,
    },
    /// Discard the recording silently.
    CancelRecording {
        session: u64,
    },
    Upload {
        session: u64,
        wav: Vec<u8>,
        duration: Duration,
    },
    Inject {
        session: u64,
        text: String,
    },
}

/// The full, pure state of the FSM: mode, session counter, current state.
#[derive(Debug, Clone, PartialEq)]
pub struct Machine {
    pub mode: HotkeyMode,
    pub next_session: u64,
    pub state: State,
}

impl Machine {
    pub fn new(mode: HotkeyMode) -> Self {
        Self {
            mode,
            next_session: 1,
            state: State::Idle,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        match &self.state {
            State::Idle => Snapshot::Idle,
            State::Recording { latched, .. } => Snapshot::Recording { latched: *latched },
            State::Transcribing { .. } => Snapshot::Transcribing,
            State::Inserting { .. } => Snapshot::Inserting,
            State::Done { .. } => Snapshot::Done,
            State::Error {
                kind,
                message,
                retryable,
                ..
            } => Snapshot::Error {
                kind: *kind,
                message: message.clone(),
                retryable: *retryable,
            },
        }
    }
}

/// Serializable view of the state, broadcast to UIs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Snapshot {
    Idle,
    Recording {
        latched: bool,
    },
    Transcribing,
    Inserting,
    Done,
    Error {
        kind: ErrKind,
        message: String,
        retryable: bool,
    },
}

/// The pure transition function. Returns the next machine and the effects to
/// run. Unknown/stale events are ignored (no-op transitions return no
/// effects).
pub fn step(m: Machine, ev: Event) -> (Machine, Vec<Effect>) {
    let Machine {
        mode,
        mut next_session,
        state,
    } = m;

    // Helper closures cannot borrow `next_session` mutably and return it, so
    // this is written as straight-line matches.
    let (state, effects) = match (state, ev) {
        // ---------------------------------------------------------- Idle-ish
        (State::Idle, Event::HotkeyDown { at }) => {
            start_recording(&mut next_session, mode, at, /* forced_toggle */ false)
        }
        (State::Idle, Event::Toggle { at }) => start_recording(&mut next_session, mode, at, true),
        (State::Idle, Event::RetryUpload { wav, duration }) => {
            let session = alloc_session(&mut next_session);
            (
                State::Transcribing { session },
                vec![
                    Effect::ShowHud,
                    Effect::Upload {
                        session,
                        wav,
                        duration,
                    },
                ],
            )
        }

        // Done and Error are transient display states: a fresh hotkey press
        // starts the next dictation immediately instead of being swallowed.
        (State::Done { .. }, Event::HotkeyDown { at }) => {
            start_recording(&mut next_session, mode, at, false)
        }
        (State::Done { .. }, Event::Toggle { at }) => {
            start_recording(&mut next_session, mode, at, true)
        }
        (State::Done { at }, Event::Tick { at: now }) => {
            if now.duration_since(at) >= DONE_DISMISS {
                (State::Idle, vec![Effect::HideHud])
            } else {
                (State::Done { at }, vec![])
            }
        }
        (State::Done { .. }, Event::Cancel) => (State::Idle, vec![Effect::HideHud]),

        (State::Error { .. }, Event::HotkeyDown { at }) => {
            start_recording(&mut next_session, mode, at, false)
        }
        (State::Error { .. }, Event::Toggle { at }) => {
            start_recording(&mut next_session, mode, at, true)
        }
        (State::Error { .. }, Event::RetryUpload { wav, duration }) => {
            let session = alloc_session(&mut next_session);
            (
                State::Transcribing { session },
                vec![
                    Effect::ShowHud,
                    Effect::Upload {
                        session,
                        wav,
                        duration,
                    },
                ],
            )
        }
        (
            State::Error {
                kind,
                message,
                retryable,
                at,
            },
            Event::Tick { at: now },
        ) => {
            if now.duration_since(at) >= ERROR_DISMISS {
                (State::Idle, vec![Effect::HideHud])
            } else {
                (
                    State::Error {
                        kind,
                        message,
                        retryable,
                        at,
                    },
                    vec![],
                )
            }
        }
        (State::Error { .. }, Event::Cancel) => (State::Idle, vec![Effect::HideHud]),

        // -------------------------------------------------------- Recording
        (
            State::Recording {
                session,
                latched,
                started_at,
                pressed_at,
            },
            Event::HotkeyUp { at },
        ) => {
            match mode {
                HotkeyMode::Hold => finalize(session),
                HotkeyMode::Toggle => (
                    State::Recording {
                        session,
                        latched,
                        started_at,
                        pressed_at,
                    },
                    vec![],
                ),
                HotkeyMode::Hybrid => {
                    if latched {
                        // Release of the second (finalizing) press arrives
                        // after we've already left Recording, or a stray
                        // release: ignore.
                        (
                            State::Recording {
                                session,
                                latched,
                                started_at,
                                pressed_at,
                            },
                            vec![],
                        )
                    } else if at.duration_since(pressed_at) < HYBRID_LATCH_WINDOW {
                        // Quick tap: latch into toggle mode.
                        (
                            State::Recording {
                                session,
                                latched: true,
                                started_at,
                                pressed_at,
                            },
                            vec![],
                        )
                    } else {
                        finalize(session)
                    }
                }
            }
        }
        (
            State::Recording {
                session,
                latched,
                started_at,
                pressed_at,
            },
            Event::HotkeyDown { .. },
        ) => {
            if latched || mode == HotkeyMode::Toggle {
                // Toggle-style finalize on the second press.
                finalize(session)
            } else {
                // Key-repeat while holding: ignore.
                (
                    State::Recording {
                        session,
                        latched,
                        started_at,
                        pressed_at,
                    },
                    vec![],
                )
            }
        }
        (State::Recording { session, .. }, Event::Toggle { .. }) => finalize(session),
        (State::Recording { session, .. }, Event::Cancel) => (
            State::Idle,
            vec![Effect::CancelRecording { session }, Effect::HideHud],
        ),
        (
            State::Recording {
                session,
                latched,
                started_at,
                pressed_at,
            },
            Event::Tick { at },
        ) => {
            if at.duration_since(started_at) >= MAX_UTTERANCE {
                finalize(session)
            } else {
                (
                    State::Recording {
                        session,
                        latched,
                        started_at,
                        pressed_at,
                    },
                    vec![],
                )
            }
        }
        (
            State::Recording {
                session,
                latched,
                started_at,
                pressed_at,
            },
            Event::AudioFailed {
                session: s,
                message,
                at,
            },
        ) => {
            if s == session {
                (
                    State::Error {
                        kind: ErrKind::Audio,
                        message,
                        retryable: false,
                        at,
                    },
                    vec![Effect::CancelRecording { session }],
                )
            } else {
                (
                    State::Recording {
                        session,
                        latched,
                        started_at,
                        pressed_at,
                    },
                    vec![],
                )
            }
        }

        // ------------------------------------------------------ Transcribing
        (
            State::Transcribing { session },
            Event::AudioFinalized {
                session: s,
                wav,
                duration,
            },
        ) => {
            if s != session {
                (State::Transcribing { session }, vec![])
            } else if duration < MIN_UTTERANCE {
                // Too short: discard silently.
                (State::Idle, vec![Effect::HideHud])
            } else {
                (
                    State::Transcribing { session },
                    vec![Effect::Upload {
                        session,
                        wav,
                        duration,
                    }],
                )
            }
        }
        (
            State::Transcribing { session },
            Event::AudioFailed {
                session: s,
                message,
                at,
            },
        ) => {
            if s == session {
                (
                    State::Error {
                        kind: ErrKind::Audio,
                        message,
                        retryable: false,
                        at,
                    },
                    vec![],
                )
            } else {
                (State::Transcribing { session }, vec![])
            }
        }
        (State::Transcribing { session }, Event::UploadOk { session: s, text }) => {
            if s == session {
                (
                    State::Inserting { session },
                    vec![Effect::Inject { session, text }],
                )
            } else {
                (State::Transcribing { session }, vec![])
            }
        }
        (
            State::Transcribing { session },
            Event::UploadErr {
                session: s,
                kind,
                message,
                retryable,
                at,
            },
        ) => {
            if s == session {
                (
                    State::Error {
                        kind,
                        message,
                        retryable,
                        at,
                    },
                    vec![],
                )
            } else {
                (State::Transcribing { session }, vec![])
            }
        }
        (State::Transcribing { .. }, Event::Cancel) => (State::Idle, vec![Effect::HideHud]),

        // -------------------------------------------------------- Inserting
        (State::Inserting { session }, Event::InsertOk { session: s, at }) => {
            if s == session {
                (State::Done { at }, vec![])
            } else {
                (State::Inserting { session }, vec![])
            }
        }
        (
            State::Inserting { session },
            Event::InsertErr {
                session: s,
                message,
                at,
            },
        ) => {
            if s == session {
                (
                    State::Error {
                        kind: ErrKind::Inject,
                        message,
                        retryable: false,
                        at,
                    },
                    vec![],
                )
            } else {
                (State::Inserting { session }, vec![])
            }
        }
        (State::Inserting { .. }, Event::Cancel) => (State::Idle, vec![Effect::HideHud]),

        // Everything else: no-op.
        (state, _) => (state, vec![]),
    };

    (
        Machine {
            mode,
            next_session,
            state,
        },
        effects,
    )
}

fn alloc_session(next_session: &mut u64) -> u64 {
    let s = *next_session;
    *next_session += 1;
    s
}

fn start_recording(
    next_session: &mut u64,
    mode: HotkeyMode,
    at: Instant,
    forced_toggle: bool,
) -> (State, Vec<Effect>) {
    let session = alloc_session(next_session);
    let latched = forced_toggle || mode == HotkeyMode::Toggle;
    (
        State::Recording {
            session,
            latched,
            started_at: at,
            pressed_at: at,
        },
        vec![Effect::StartRecording { session }, Effect::ShowHud],
    )
}

fn finalize(session: u64) -> (State, Vec<Effect>) {
    (
        State::Transcribing { session },
        vec![Effect::StopRecording { session }],
    )
}

// ---------------------------------------------------------------------------
// Controller actor
// ---------------------------------------------------------------------------

/// External commands accepted by the controller.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    HotkeyDown,
    HotkeyUp,
    Toggle,
    /// Start recording only if idle.
    Start,
    /// Finalize only if recording.
    Stop,
    Cancel,
    /// Re-upload the most recently spooled utterance.
    RetryLast,
    /// Replace the hotkey mode (config change).
    SetMode(HotkeyMode),
}

/// The controller executes FSM effects through this trait. Implementations
/// must be non-blocking: long work is spawned, and results come back to the
/// controller as [`Event`]s through the sender handed to
/// [`Controller::spawn`]'s returned handle.
pub trait EffectRunner: Send + 'static {
    fn show_hud(&mut self);
    fn hide_hud(&mut self);
    fn start_recording(&mut self, session: u64);
    fn stop_recording(&mut self, session: u64);
    fn cancel_recording(&mut self, session: u64);
    fn upload(&mut self, session: u64, wav: Vec<u8>, duration: Duration);
    fn inject(&mut self, session: u64, text: String);
}

enum Input {
    Cmd(Command),
    Ev(Event),
}

/// Cloneable handle to a running [`Controller`].
#[derive(Clone)]
pub struct ControllerHandle {
    tx: mpsc::UnboundedSender<Input>,
    snapshots: broadcast::Sender<Snapshot>,
}

impl ControllerHandle {
    pub fn command(&self, cmd: Command) {
        let _ = self.tx.send(Input::Cmd(cmd));
    }

    /// Feed an event (used by effect implementations to report results).
    pub fn event(&self, ev: Event) {
        let _ = self.tx.send(Input::Ev(ev));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Snapshot> {
        self.snapshots.subscribe()
    }
}

/// Loads the latest spooled WAV for `Command::RetryLast`.
pub type SpoolLoader = Box<dyn Fn() -> Option<(Vec<u8>, Duration)> + Send + 'static>;

pub struct Controller;

impl Controller {
    /// Spawn the controller actor on the current tokio runtime.
    pub fn spawn(
        mode: HotkeyMode,
        mut runner: Box<dyn EffectRunner>,
        spool_loader: SpoolLoader,
    ) -> ControllerHandle {
        let (tx, mut rx) = mpsc::unbounded_channel::<Input>();
        let (snap_tx, _) = broadcast::channel(64);
        let handle = ControllerHandle {
            tx,
            snapshots: snap_tx.clone(),
        };

        let mut machine = Machine::new(mode);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(100));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut last_snapshot = machine.snapshot();
            loop {
                let ev = tokio::select! {
                    input = rx.recv() => match input {
                        None => break,
                        Some(Input::Ev(ev)) => Some(ev),
                        Some(Input::Cmd(cmd)) => Self::command_to_event(&mut machine, cmd, &spool_loader),
                    },
                    _ = ticker.tick() => {
                        if matches!(machine.state, State::Idle) {
                            None
                        } else {
                            Some(Event::Tick { at: Instant::now() })
                        }
                    }
                };
                let Some(ev) = ev else { continue };
                let (next, effects) = step(machine.clone(), ev);
                machine = next;
                for effect in effects {
                    match effect {
                        Effect::ShowHud => runner.show_hud(),
                        Effect::HideHud => runner.hide_hud(),
                        Effect::StartRecording { session } => runner.start_recording(session),
                        Effect::StopRecording { session } => runner.stop_recording(session),
                        Effect::CancelRecording { session } => runner.cancel_recording(session),
                        Effect::Upload {
                            session,
                            wav,
                            duration,
                        } => runner.upload(session, wav, duration),
                        Effect::Inject { session, text } => runner.inject(session, text),
                    }
                }
                let snap = machine.snapshot();
                if snap != last_snapshot {
                    last_snapshot = snap.clone();
                    let _ = snap_tx.send(snap);
                }
            }
        });

        handle
    }

    fn command_to_event(
        machine: &mut Machine,
        cmd: Command,
        spool_loader: &SpoolLoader,
    ) -> Option<Event> {
        let now = Instant::now();
        match cmd {
            Command::HotkeyDown => Some(Event::HotkeyDown { at: now }),
            Command::HotkeyUp => Some(Event::HotkeyUp { at: now }),
            Command::Toggle => Some(Event::Toggle { at: now }),
            Command::Start => match machine.state {
                State::Idle | State::Done { .. } | State::Error { .. } => {
                    Some(Event::Toggle { at: now })
                }
                _ => None,
            },
            Command::Stop => match machine.state {
                State::Recording { .. } => Some(Event::Toggle { at: now }),
                _ => None,
            },
            Command::Cancel => Some(Event::Cancel),
            Command::RetryLast => {
                let (wav, duration) = spool_loader()?;
                Some(Event::RetryUpload { wav, duration })
            }
            Command::SetMode(mode) => {
                machine.mode = mode;
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    fn ms(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn machine(mode: HotkeyMode) -> Machine {
        Machine::new(mode)
    }

    fn wav() -> Vec<u8> {
        vec![1, 2, 3, 4]
    }

    fn recording_session(m: &Machine) -> u64 {
        match m.state {
            State::Recording { session, .. } => session,
            State::Transcribing { session } => session,
            State::Inserting { session } => session,
            _ => panic!("no session in {:?}", m.state),
        }
    }

    #[test]
    fn hold_flow_happy_path() {
        let base = t0();
        let m = machine(HotkeyMode::Hold);

        // Press: start recording + show HUD.
        let (m, fx) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);
        assert!(matches!(m.state, State::Recording { latched: false, .. }));
        assert_eq!(
            fx,
            vec![Effect::StartRecording { session }, Effect::ShowHud]
        );

        // Release after 1s: finalize.
        let (m, fx) = step(m, Event::HotkeyUp { at: ms(base, 1000) });
        assert!(matches!(m.state, State::Transcribing { .. }));
        assert_eq!(fx, vec![Effect::StopRecording { session }]);

        // Audio arrives: upload.
        let (m, fx) = step(
            m,
            Event::AudioFinalized {
                session,
                wav: wav(),
                duration: Duration::from_millis(1000),
            },
        );
        assert!(matches!(m.state, State::Transcribing { .. }));
        assert_eq!(
            fx,
            vec![Effect::Upload {
                session,
                wav: wav(),
                duration: Duration::from_millis(1000)
            }]
        );

        // Upload ok: inject.
        let (m, fx) = step(
            m,
            Event::UploadOk {
                session,
                text: "hello world".into(),
            },
        );
        assert!(matches!(m.state, State::Inserting { .. }));
        assert_eq!(
            fx,
            vec![Effect::Inject {
                session,
                text: "hello world".into()
            }]
        );

        // Insert ok: done.
        let (m, fx) = step(
            m,
            Event::InsertOk {
                session,
                at: ms(base, 1500),
            },
        );
        assert!(matches!(m.state, State::Done { .. }));
        assert!(fx.is_empty());

        // Done auto-dismisses after 900ms.
        let (m, fx) = step(m, Event::Tick { at: ms(base, 1600) });
        assert!(matches!(m.state, State::Done { .. }), "not dismissed early");
        assert!(fx.is_empty());
        let (m, fx) = step(m, Event::Tick { at: ms(base, 2500) });
        assert_eq!(m.state, State::Idle);
        assert_eq!(fx, vec![Effect::HideHud]);
    }

    #[test]
    fn hybrid_quick_tap_latches_then_second_press_finalizes() {
        let base = t0();
        let m = machine(HotkeyMode::Hybrid);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);

        // Release within the 250ms window: latch, keep recording.
        let (m, fx) = step(m, Event::HotkeyUp { at: ms(base, 100) });
        assert!(matches!(m.state, State::Recording { latched: true, .. }));
        assert!(fx.is_empty());

        // Second press: finalize.
        let (m, fx) = step(m, Event::HotkeyDown { at: ms(base, 3000) });
        assert_eq!(m.state, State::Transcribing { session });
        assert_eq!(fx, vec![Effect::StopRecording { session }]);

        // The corresponding release is ignored.
        let (m, fx) = step(m, Event::HotkeyUp { at: ms(base, 3100) });
        assert_eq!(m.state, State::Transcribing { session });
        assert!(fx.is_empty());
    }

    #[test]
    fn hybrid_long_hold_finalizes_on_release() {
        let base = t0();
        let m = machine(HotkeyMode::Hybrid);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);

        // Release at exactly the window boundary: NOT a latch (>= 250ms).
        let (m, fx) = step(m, Event::HotkeyUp { at: ms(base, 250) });
        assert_eq!(m.state, State::Transcribing { session });
        assert_eq!(fx, vec![Effect::StopRecording { session }]);
    }

    #[test]
    fn toggle_mode_ignores_release_and_stops_on_second_press() {
        let base = t0();
        let m = machine(HotkeyMode::Toggle);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);
        assert!(matches!(m.state, State::Recording { latched: true, .. }));

        let (m, fx) = step(m, Event::HotkeyUp { at: ms(base, 50) });
        assert!(matches!(m.state, State::Recording { .. }));
        assert!(fx.is_empty());

        let (m, fx) = step(m, Event::HotkeyDown { at: ms(base, 4000) });
        assert_eq!(m.state, State::Transcribing { session });
        assert_eq!(fx, vec![Effect::StopRecording { session }]);
    }

    #[test]
    fn hold_mode_key_repeat_is_ignored() {
        let base = t0();
        let m = machine(HotkeyMode::Hold);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let before = m.clone();
        // OS key-repeat fires HotkeyDown again while held.
        let (m, fx) = step(m, Event::HotkeyDown { at: ms(base, 600) });
        assert_eq!(m, before);
        assert!(fx.is_empty());
    }

    #[test]
    fn cancel_while_recording_discards() {
        let base = t0();
        let m = machine(HotkeyMode::Hold);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);
        let (m, fx) = step(m, Event::Cancel);
        assert_eq!(m.state, State::Idle);
        assert_eq!(
            fx,
            vec![Effect::CancelRecording { session }, Effect::HideHud]
        );
    }

    #[test]
    fn cancel_while_transcribing_returns_to_idle_and_ignores_late_upload() {
        let base = t0();
        let m = machine(HotkeyMode::Hold);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);
        let (m, _) = step(m, Event::HotkeyUp { at: ms(base, 1000) });
        let (m, fx) = step(m, Event::Cancel);
        assert_eq!(m.state, State::Idle);
        assert_eq!(fx, vec![Effect::HideHud]);

        // Late results for the cancelled session are dropped.
        let (m, fx) = step(
            m,
            Event::UploadOk {
                session,
                text: "late".into(),
            },
        );
        assert_eq!(m.state, State::Idle);
        assert!(fx.is_empty());
    }

    #[test]
    fn short_audio_is_discarded_silently() {
        let base = t0();
        let m = machine(HotkeyMode::Hold);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);
        let (m, _) = step(m, Event::HotkeyUp { at: ms(base, 200) });
        let (m, fx) = step(
            m,
            Event::AudioFinalized {
                session,
                wav: wav(),
                duration: Duration::from_millis(200),
            },
        );
        assert_eq!(m.state, State::Idle);
        assert_eq!(fx, vec![Effect::HideHud]);
    }

    #[test]
    fn five_minute_cap_auto_finalizes() {
        let base = t0();
        let m = machine(HotkeyMode::Toggle);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);

        let (m, fx) = step(
            m,
            Event::Tick {
                at: base + Duration::from_secs(299),
            },
        );
        assert!(matches!(m.state, State::Recording { .. }));
        assert!(fx.is_empty());

        let (m, fx) = step(
            m,
            Event::Tick {
                at: base + Duration::from_secs(300),
            },
        );
        assert_eq!(m.state, State::Transcribing { session });
        assert_eq!(fx, vec![Effect::StopRecording { session }]);
    }

    #[test]
    fn upload_error_shows_error_then_dismisses() {
        let base = t0();
        let m = machine(HotkeyMode::Hold);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);
        let (m, _) = step(m, Event::HotkeyUp { at: ms(base, 1000) });
        let (m, _) = step(
            m,
            Event::AudioFinalized {
                session,
                wav: wav(),
                duration: Duration::from_millis(1000),
            },
        );
        let (m, fx) = step(
            m,
            Event::UploadErr {
                session,
                kind: ErrKind::Connect,
                message: "connection refused".into(),
                retryable: true,
                at: ms(base, 1500),
            },
        );
        assert!(
            matches!(
                m.state,
                State::Error {
                    kind: ErrKind::Connect,
                    retryable: true,
                    ..
                }
            ),
            "state = {:?}",
            m.state
        );
        assert!(fx.is_empty());

        // Not dismissed before 2.5s.
        let (m, fx) = step(m, Event::Tick { at: ms(base, 3000) });
        assert!(matches!(m.state, State::Error { .. }));
        assert!(fx.is_empty());

        // Dismissed after 2.5s.
        let (m, fx) = step(m, Event::Tick { at: ms(base, 4100) });
        assert_eq!(m.state, State::Idle);
        assert_eq!(fx, vec![Effect::HideHud]);
    }

    #[test]
    fn insert_error_is_not_retryable() {
        let base = t0();
        let m = machine(HotkeyMode::Hold);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);
        let (m, _) = step(m, Event::HotkeyUp { at: ms(base, 1000) });
        let (m, _) = step(
            m,
            Event::AudioFinalized {
                session,
                wav: wav(),
                duration: Duration::from_secs(1),
            },
        );
        let (m, _) = step(
            m,
            Event::UploadOk {
                session,
                text: "x".into(),
            },
        );
        let (m, fx) = step(
            m,
            Event::InsertErr {
                session,
                message: "paste failed".into(),
                at: ms(base, 2000),
            },
        );
        assert!(matches!(
            m.state,
            State::Error {
                kind: ErrKind::Inject,
                retryable: false,
                ..
            }
        ));
        assert!(fx.is_empty());
    }

    #[test]
    fn retry_upload_from_error_starts_new_session() {
        let base = t0();
        let m = Machine {
            mode: HotkeyMode::Hybrid,
            next_session: 7,
            state: State::Error {
                kind: ErrKind::Connect,
                message: "x".into(),
                retryable: true,
                at: base,
            },
        };
        let (m, fx) = step(
            m,
            Event::RetryUpload {
                wav: wav(),
                duration: Duration::from_secs(2),
            },
        );
        assert_eq!(m.state, State::Transcribing { session: 7 });
        assert_eq!(m.next_session, 8);
        assert_eq!(
            fx,
            vec![
                Effect::ShowHud,
                Effect::Upload {
                    session: 7,
                    wav: wav(),
                    duration: Duration::from_secs(2)
                }
            ]
        );
    }

    #[test]
    fn stale_session_events_are_ignored() {
        let base = t0();
        let m = machine(HotkeyMode::Hold);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);
        let (m, _) = step(m, Event::HotkeyUp { at: ms(base, 1000) });

        // A stale AudioFinalized from a previous session is dropped.
        let (m, fx) = step(
            m,
            Event::AudioFinalized {
                session: session + 100,
                wav: wav(),
                duration: Duration::from_secs(1),
            },
        );
        assert_eq!(m.state, State::Transcribing { session });
        assert!(fx.is_empty());

        // Same for UploadOk / UploadErr / InsertOk with wrong sessions.
        let (m, fx) = step(
            m,
            Event::UploadOk {
                session: session + 1,
                text: "no".into(),
            },
        );
        assert_eq!(m.state, State::Transcribing { session });
        assert!(fx.is_empty());
    }

    #[test]
    fn new_dictation_can_start_from_done_and_error() {
        let base = t0();
        let m = Machine {
            mode: HotkeyMode::Hold,
            next_session: 3,
            state: State::Done { at: base },
        };
        let (m, fx) = step(m, Event::HotkeyDown { at: ms(base, 100) });
        assert!(matches!(m.state, State::Recording { session: 3, .. }));
        assert_eq!(
            fx,
            vec![Effect::StartRecording { session: 3 }, Effect::ShowHud]
        );

        let m = Machine {
            mode: HotkeyMode::Hold,
            next_session: 9,
            state: State::Error {
                kind: ErrKind::Server,
                message: "e".into(),
                retryable: false,
                at: base,
            },
        };
        let (m, fx) = step(m, Event::HotkeyDown { at: ms(base, 100) });
        assert!(matches!(m.state, State::Recording { session: 9, .. }));
        assert_eq!(
            fx,
            vec![Effect::StartRecording { session: 9 }, Effect::ShowHud]
        );
    }

    #[test]
    fn ignores_repeat_hotkey_down_while_transcribing() {
        let base = t0();
        let m = machine(HotkeyMode::Hold);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let (m, _) = step(m, Event::HotkeyUp { at: ms(base, 1000) });
        let before = m.clone();
        let (m, fx) = step(m, Event::HotkeyDown { at: ms(base, 1100) });
        assert_eq!(m, before);
        assert!(fx.is_empty());
    }

    #[test]
    fn audio_failure_while_recording_goes_to_error() {
        let base = t0();
        let m = machine(HotkeyMode::Hold);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let session = recording_session(&m);
        let (m, fx) = step(
            m,
            Event::AudioFailed {
                session,
                message: "device lost".into(),
                at: ms(base, 100),
            },
        );
        assert!(matches!(
            m.state,
            State::Error {
                kind: ErrKind::Audio,
                ..
            }
        ));
        assert_eq!(fx, vec![Effect::CancelRecording { session }]);
    }

    #[test]
    fn sessions_are_monotonic() {
        let base = t0();
        let m = machine(HotkeyMode::Hold);
        let (m, _) = step(m, Event::HotkeyDown { at: base });
        let s1 = recording_session(&m);
        let (m, _) = step(m, Event::Cancel);
        let (m, _) = step(m, Event::HotkeyDown { at: ms(base, 500) });
        let s2 = recording_session(&m);
        assert!(s2 > s1);
    }

    #[test]
    fn toggle_event_from_cli_starts_and_stops() {
        let base = t0();
        let m = machine(HotkeyMode::Hybrid);
        let (m, fx) = step(m, Event::Toggle { at: base });
        let session = recording_session(&m);
        // CLI/tray toggle behaves latched even in hybrid mode.
        assert!(matches!(m.state, State::Recording { latched: true, .. }));
        assert_eq!(
            fx,
            vec![Effect::StartRecording { session }, Effect::ShowHud]
        );

        let (m, fx) = step(m, Event::Toggle { at: ms(base, 2000) });
        assert_eq!(m.state, State::Transcribing { session });
        assert_eq!(fx, vec![Effect::StopRecording { session }]);
    }
}
