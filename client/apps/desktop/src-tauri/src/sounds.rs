//! Best-effort dictation feedback sounds (ui.sounds). Spawned detached; any
//! failure is silent — sounds are never worth an error state.

use std::process::{Command, Stdio};

pub enum Cue {
    Done,
    Error,
}

pub fn play(cue: Cue) {
    #[cfg(target_os = "macos")]
    let candidates: &[&str] = match cue {
        Cue::Done => &["/System/Library/Sounds/Pop.aiff"],
        Cue::Error => &["/System/Library/Sounds/Basso.aiff"],
    };
    #[cfg(target_os = "macos")]
    let players: &[&str] = &["afplay"];

    #[cfg(not(target_os = "macos"))]
    let candidates: &[&str] = match cue {
        Cue::Done => &[
            "/usr/share/sounds/freedesktop/stereo/complete.oga",
            "/usr/share/sounds/freedesktop/stereo/message.oga",
        ],
        Cue::Error => &["/usr/share/sounds/freedesktop/stereo/dialog-error.oga"],
    };
    #[cfg(not(target_os = "macos"))]
    let players: &[&str] = &["paplay", "pw-play"];

    for player in players {
        for path in candidates {
            if !std::path::Path::new(path).exists() {
                continue;
            }
            let spawned = Command::new(player)
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if spawned.is_ok() {
                return;
            }
        }
    }
}
