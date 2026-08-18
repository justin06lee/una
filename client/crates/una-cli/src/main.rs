//! `una` — tiny CLI that drives the running una desktop app over its unix
//! socket. Intended for compositor keybinds (Hyprland bind/bindr, Sway
//! --release, GNOME/KDE custom shortcuts) and scripting.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "una",
    version,
    about = "Control the una dictation client",
    long_about = "Sends a command to the running una desktop app over its unix socket.\n\
                  Useful for compositor keybinds: bind press to `una start` and release\n\
                  to `una stop` for hold-to-talk, or a single key to `una toggle`."
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start dictation if idle, stop it if recording.
    Toggle,
    /// Start recording (no-op unless idle).
    Start,
    /// Stop recording and transcribe (no-op unless recording).
    Stop,
    /// Cancel the current dictation, discarding audio.
    Cancel,
}

impl Cmd {
    fn wire_name(&self) -> &'static str {
        match self {
            Cmd::Toggle => "toggle",
            Cmd::Start => "start",
            Cmd::Stop => "stop",
            Cmd::Cancel => "cancel",
        }
    }
}

#[cfg(unix)]
fn socket_path() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        if !runtime.is_empty() {
            return PathBuf::from(runtime).join("una").join("una.sock");
        }
    }
    // SAFETY: getuid is always safe to call.
    let uid = unsafe { libc_getuid() };
    PathBuf::from(format!("/tmp/una-{uid}")).join("una.sock")
}

#[cfg(unix)]
unsafe fn libc_getuid() -> u32 {
    // Avoid a libc dependency for one syscall.
    extern "C" {
        fn getuid() -> u32;
    }
    getuid()
}

#[cfg(unix)]
fn run(cmd: &Cmd) -> Result<(), String> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let path = socket_path();
    let mut stream = UnixStream::connect(&path).map_err(|e| {
        format!(
            "could not connect to {} ({e}).\nIs the una desktop app running?",
            path.display()
        )
    })?;
    let line = format!("{{\"cmd\":\"{}\"}}\n", cmd.wire_name());
    stream
        .write_all(line.as_bytes())
        .map_err(|e| format!("could not send command: {e}"))?;

    let mut reader = BufReader::new(stream);
    let mut reply = String::new();
    reader
        .read_line(&mut reply)
        .map_err(|e| format!("no reply from una: {e}"))?;
    let reply = reply.trim();
    if reply.contains("\"ok\":true") {
        Ok(())
    } else if reply.is_empty() {
        Err("empty reply from una".into())
    } else {
        Err(format!("una rejected the command: {reply}"))
    }
}

#[cfg(not(unix))]
fn run(_cmd: &Cmd) -> Result<(), String> {
    Err("the una CLI is only supported on unix platforms".into())
}

fn main() {
    let cli = Cli::parse();
    if let Err(msg) = run(&cli.command) {
        eprintln!("una: {msg}");
        std::process::exit(1);
    }
}
