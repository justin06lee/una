//! Learning from what you do to the text after una pastes it.
//!
//! Every dictation the server stores can carry one correction, and those
//! corrections are the entire training set for both fine-tunes: the ASR
//! target teaches Whisper what you actually said, the style target teaches
//! the cleanup model how you want it written. Before this module the only way
//! to produce one was to review dictations by hand in the dashboard, so in
//! practice there were almost none.
//!
//! After a successful paste this watches for one of three endings:
//!
//! 1. **Nothing happens.** No editing key is pressed for the whole watch
//!    window, so the transcription was right — reported as `accepted`, which
//!    is where most of the training data comes from. Works everywhere,
//!    including terminals, because it needs no reading of the field at all.
//! 2. **You edit it, in an app whose text can be read.** The keystrokes are
//!    only used as an activity signal; once you stop typing the field is read
//!    back and diffed against the snapshot, and the pasted span's new text is
//!    reported as `edited`. No UI is shown at any point.
//! 3. **You edit it, in an app whose text cannot be read** — a terminal, a
//!    canvas editor. There is nothing to diff, so the correction window opens
//!    with what was pasted, and what you submit is both reported and written
//!    back into the app you were in.
//!
//! Everything here is best-effort by design: an edit that can't be attributed
//! to the dictation with confidence is dropped rather than guessed at. A
//! wrong training pair is worse than a missing one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use una_core::config::{Config, CorrectionConfig};
use una_core::correction::{classify, Action, EditCounter, EditKey};
use una_core::net::CorrectionRequest;

use crate::app_state::AppState;
use crate::windows;

/// Only the newest paste is watched; an older watcher sees the generation
/// move and stops.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// What the correction window needs to know, and what a write-back needs.
#[derive(Debug, Clone)]
pub struct Pending {
    pub dictation_id: String,
    /// Exactly what una pasted.
    pub inserted: String,
    /// The app the paste went into, so focus can be handed back.
    pub app_pid: Option<i32>,
    pub app_name: Option<String>,
    /// Current length of the pasted span in characters, when it is still
    /// known — `None` once the user has moved the caret, at which point
    /// nothing can safely be deleted on their behalf.
    pub span_len: Option<usize>,
}

/// The pending correction the window is editing, if it is open.
pub type PendingSlot = Arc<Mutex<Option<Pending>>>;

/// Note a successful paste and start watching it. Cheap and non-blocking:
/// everything real happens on a spawned task.
pub fn on_inserted(app: &AppHandle, dictation_id: String, inserted: String) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let cfg = state.config_snapshot();
    if !cfg.correction.enabled || !una_platform::supports_correction_capture() {
        return;
    }
    if inserted.trim().is_empty() {
        return;
    }
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        watch(app, generation, dictation_id, inserted, cfg).await;
    });
}

/// True while `generation` is still the newest paste.
fn current(generation: u64) -> bool {
    GENERATION.load(Ordering::SeqCst) == generation
}

async fn watch(
    app: AppHandle,
    generation: u64,
    dictation_id: String,
    inserted: String,
    cfg: Config,
) {
    let settings = cfg.correction.clone();
    let inserted_chars = inserted.chars().count();

    // Snapshot the field and remember which app it belongs to. Both are AX /
    // AppKit calls that talk to another process, so they go to a blocking
    // thread.
    let (target_pid, snapshot) = tauri::async_runtime::spawn_blocking({
        let inserted = inserted.clone();
        move || (una_platform::frontmost_pid(), Snapshot::take(&inserted))
    })
    .await
    .unwrap_or((None, None));

    let Some(mut rx) = arm_watcher(&app) else {
        tracing::debug!("correction: no event tap available; not watching this paste");
        return;
    };

    let deadline = Instant::now() + Duration::from_secs(settings.watch_seconds);
    let settle = Duration::from_millis(settings.settle_ms);
    let mut counter = EditCounter::new(inserted_chars);
    let mut popped = false;

    loop {
        let now = Instant::now();
        if now >= deadline || !current(generation) {
            break;
        }
        let wait = settle.min(deadline - now);
        match tokio::time::timeout(wait, rx.recv()).await {
            // A key landed.
            Ok(Some(key)) => {
                counter.apply(key);
                if !counter.touched() {
                    continue;
                }
                // The first real edit in an app whose text can't be read is
                // the only moment the window can usefully open.
                if snapshot.is_none() && settings.popup && !popped {
                    popped = true;
                    if same_app_still_focused(target_pid).await {
                        open_window(&app, &dictation_id, &inserted, &counter, target_pid).await;
                        break;
                    }
                }
            }
            // The tap disarmed underneath us.
            Ok(None) => break,
            // Silence: the edit, if there was one, is finished.
            Err(_) => {
                if counter.touched() {
                    break;
                }
            }
        }
    }

    let superseded = !current(generation);
    if !superseded {
        // When superseded, the newer paste has already armed its own watcher
        // on the same tap; disarming here would deafen it.
        disarm_watcher(&app);
    }
    if popped {
        return; // the window owns the outcome from here
    }

    finalize(
        &app,
        &dictation_id,
        &inserted,
        snapshot,
        &counter,
        &settings,
        superseded,
    )
    .await;
}

/// What the field looked like at paste time, when it could be read at all.
///
/// macOS-only: it is the accessibility API that makes silent capture
/// possible, and there is no cross-toolkit equivalent on Linux.
#[cfg(target_os = "macos")]
mod ax {
    use una_core::correction::{edited_region_text, Region};

    pub struct Snapshot {
        field: una_platform::macos::axtext::FocusedField,
        region: Region,
    }

    impl Snapshot {
        /// Locate the pasted text inside the focused field.
        ///
        /// Returns `None` unless the field's text can be read *and* the paste
        /// sits exactly where the caret says it should. That second check is
        /// what keeps this honest: if the app rewrote the text on the way in
        /// (smart quotes, autocorrect) or the caret isn't at the end of the
        /// paste, the span cannot be tracked and the correction window is
        /// used instead.
        pub fn take(inserted: &str) -> Option<Self> {
            let field = una_platform::macos::axtext::FocusedField::capture()?;
            let chars = inserted.chars().count();
            let start = field.caret.checked_sub(chars)?;
            let region = Region::new(start, chars);
            let landed: String = field
                .value
                .chars()
                .skip(region.start)
                .take(region.len)
                .collect();
            (landed == inserted).then_some(Self { field, region })
        }

        /// Read the field again and return what the pasted span has become,
        /// or `None` if focus left the field or the edit was somewhere else.
        pub fn edited_text(&self) -> Option<String> {
            let after = self.field.reread()?;
            edited_region_text(&self.field.value, &after, self.region)
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod ax {
    pub struct Snapshot;

    impl Snapshot {
        pub fn take(_inserted: &str) -> Option<Self> {
            None
        }
        pub fn edited_text(&self) -> Option<String> {
            None
        }
    }
}

use ax::Snapshot;

/// Whether the user is still in the app the text was pasted into. Keystrokes
/// arriving after they've switched apps are somebody else's.
async fn same_app_still_focused(expected: Option<i32>) -> bool {
    let Some(expected) = expected else {
        // Nothing to compare against, so nothing to rule out.
        return true;
    };
    tauri::async_runtime::spawn_blocking(move || una_platform::frontmost_pid() == Some(expected))
        .await
        .unwrap_or(false)
}

/// Read the field back, work out what the pasted span became, and report it.
async fn finalize(
    app: &AppHandle,
    dictation_id: &str,
    inserted: &str,
    snapshot: Option<Snapshot>,
    counter: &EditCounter,
    settings: &CorrectionConfig,
    superseded: bool,
) {
    // Untouched: the transcription stands. This needs no field access, which
    // is why it works in terminals too.
    if !counter.touched() {
        if settings.auto_accept {
            submit(app, dictation_id, Action::Accepted, inserted.to_string()).await;
        }
        return;
    }

    if superseded {
        // Another dictation has since been pasted, very likely into this same
        // field. Whatever the field says now describes both, so a diff would
        // attribute the second dictation's text to the first.
        return;
    }
    let Some(snapshot) = snapshot else {
        return; // edited, but nothing could be read and no window was shown
    };
    let edited = tauri::async_runtime::spawn_blocking(move || snapshot.edited_text())
        .await
        .ok()
        .flatten();
    let Some(current) = edited else {
        // Focus left the field, or the change was outside the pasted span —
        // either way the edit cannot be attributed to this dictation.
        tracing::debug!("correction: edit could not be attributed to the paste; ignoring");
        return;
    };
    if current == inserted {
        if settings.auto_accept {
            submit(app, dictation_id, Action::Accepted, inserted.to_string()).await;
        }
        return;
    }
    submit(app, dictation_id, classify(inserted, &current), current).await;
}

/// PUT the correction to whichever server address is live.
pub async fn submit(app: &AppHandle, dictation_id: &str, action: Action, text: String) {
    submit_with_source(app, dictation_id, action, text, "auto").await;
}

pub async fn submit_with_source(
    app: &AppHandle,
    dictation_id: &str,
    action: Action,
    text: String,
    source: &str,
) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let (urls, autodiscover) = {
        let cfg = state.config.read().unwrap();
        (cfg.server.urls.clone(), cfg.server.autodiscover)
    };
    let Some(base) = state.endpoints.resolve(&urls, autodiscover).await else {
        tracing::debug!("correction: no server reachable; dropping this pair");
        return;
    };
    // An excluded dictation carries no target text; an accepted one is the
    // transcript itself, which the server already has.
    let payload = match action {
        Action::Accepted => CorrectionRequest {
            action: action.as_str().into(),
            corrected_text: None,
            polished_text: Some(text),
            source: source.into(),
        },
        Action::Edited => CorrectionRequest {
            action: action.as_str().into(),
            corrected_text: Some(text.clone()),
            polished_text: Some(text),
            source: source.into(),
        },
        Action::Excluded => CorrectionRequest {
            action: action.as_str().into(),
            corrected_text: None,
            polished_text: None,
            source: source.into(),
        },
    };
    match state.api.put_correction(&base, dictation_id, &payload).await {
        Ok(resp) => tracing::info!(
            "correction: {} for {dictation_id} (eligible: {})",
            action.as_str(),
            resp.training_eligible.unwrap_or(false)
        ),
        Err(e) => tracing::warn!("correction: could not submit for {dictation_id}: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Event-tap plumbing
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn arm_watcher(app: &AppHandle) -> Option<UnboundedReceiver<EditKey>> {
    use una_platform::macos::eventtap::KeyClass;

    // The tap is created lazily for native hotkeys; a user on a combo binding
    // has none yet. Creating it here costs nothing when no binding is set,
    // and needs the Accessibility permission the paste already required.
    let tap = match crate::hotkey::ensure_tap(app) {
        Ok(tap) => tap,
        Err(e) => {
            tracing::debug!("correction: could not start the event tap: {e}");
            return None;
        }
    };
    let (tx, rx) = mpsc::unbounded_channel();
    tap.watch_edits(Box::new(move |class| {
        let key = match class {
            KeyClass::Insert => EditKey::Insert,
            KeyClass::Delete => EditKey::Delete,
            KeyClass::Navigate => EditKey::Navigate,
        };
        let _ = tx.send(key);
    }));
    Some(rx)
}

#[cfg(not(target_os = "macos"))]
fn arm_watcher(_app: &AppHandle) -> Option<UnboundedReceiver<EditKey>> {
    None
}

#[cfg(target_os = "macos")]
fn disarm_watcher(app: &AppHandle) {
    if let Ok(tap) = crate::hotkey::ensure_tap(app) {
        tap.stop_watching_edits();
    }
}

#[cfg(not(target_os = "macos"))]
fn disarm_watcher(_app: &AppHandle) {}

// ---------------------------------------------------------------------------
// The correction window
// ---------------------------------------------------------------------------

async fn open_window(
    app: &AppHandle,
    dictation_id: &str,
    inserted: &str,
    counter: &EditCounter,
    pid: Option<i32>,
) {
    let app_name = tauri::async_runtime::spawn_blocking(|| una_platform::frontmost().current())
        .await
        .ok()
        .flatten();
    let pending = Pending {
        dictation_id: dictation_id.to_string(),
        inserted: inserted.to_string(),
        app_pid: pid,
        app_name,
        span_len: counter.len(),
    };
    if let Some(state) = app.try_state::<AppState>() {
        *state.pending_correction.lock().unwrap() = Some(pending);
    }
    windows::show_correction(app);
}

/// Put the user's corrected text back where it came from.
///
/// Only attempted when the span's length is still known; otherwise the text
/// is left on the clipboard, because deleting the wrong number of characters
/// in someone's terminal is a much worse outcome than not pasting.
pub async fn write_back(pending: Pending, corrected: String, restore_clipboard: bool) -> bool {
    let Some(pid) = pending.app_pid else {
        return false;
    };
    let Some(span) = pending.span_len else {
        tracing::info!("correction: caret moved during the edit; not rewriting the text");
        return false;
    };
    tauri::async_runtime::spawn_blocking(move || {
        if !una_platform::activate_pid(pid) {
            return false;
        }
        // Let the app finish coming forward before typing into it.
        std::thread::sleep(Duration::from_millis(120));
        if let Err(e) = una_platform::send_backspaces(span) {
            tracing::warn!("correction: could not clear the old text: {e}");
            return false;
        }
        let opts = una_platform::InjectOptions {
            restore_clipboard,
            ..Default::default()
        };
        match una_platform::injector().inject(&corrected, &opts) {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("correction: could not paste the corrected text: {e}");
                false
            }
        }
    })
    .await
    .unwrap_or(false)
}
