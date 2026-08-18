//! Graphical-session detection via environment variables.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Wayland,
    X11,
    Unknown,
}

pub fn session_kind() -> SessionKind {
    let has = |k: &str| std::env::var(k).map(|v| !v.is_empty()).unwrap_or(false);
    if has("WAYLAND_DISPLAY") {
        SessionKind::Wayland
    } else if has("DISPLAY") {
        SessionKind::X11
    } else {
        SessionKind::Unknown
    }
}
