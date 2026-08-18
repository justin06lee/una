//! Unix-socket IPC so `una` (the CLI) and compositor keybinds can drive
//! dictation. Newline-delimited JSON: `{"cmd":"toggle"|"start"|"stop"|"cancel"}`.
//! Each request is answered with one JSON line: `{"ok":true}` or
//! `{"ok":false,"error":"..."}`.

use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use crate::state::{Command, ControllerHandle};

/// `$XDG_RUNTIME_DIR/una/una.sock`, falling back to `/tmp/una-$UID/una.sock`.
pub fn socket_path() -> PathBuf {
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        if !runtime.is_empty() {
            return PathBuf::from(runtime).join("una").join("una.sock");
        }
    }
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/una-{uid}")).join("una.sock")
}

#[derive(Debug, serde::Deserialize)]
struct Request {
    cmd: String,
}

fn parse_command(line: &str) -> Result<Command, String> {
    let req: Request = serde_json::from_str(line).map_err(|e| format!("invalid request: {e}"))?;
    match req.cmd.as_str() {
        "toggle" => Ok(Command::Toggle),
        "start" => Ok(Command::Start),
        "stop" => Ok(Command::Stop),
        "cancel" => Ok(Command::Cancel),
        other => Err(format!("unknown cmd: {other:?}")),
    }
}

/// Bind the socket (replacing a stale one) and serve forever.
pub async fn serve(controller: ControllerHandle) -> std::io::Result<()> {
    let path = socket_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }
    if path.exists() {
        // If nothing answers, the socket is stale from a previous run.
        match std::os::unix::net::UnixStream::connect(&path) {
            Ok(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    "another una instance owns the IPC socket",
                ));
            }
            Err(_) => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    let listener = UnixListener::bind(&path)?;
    tracing::info!("ipc: listening on {}", path.display());

    loop {
        let (stream, _) = listener.accept().await?;
        let controller = controller.clone();
        tokio::spawn(async move {
            let (read_half, mut write_half) = stream.into_split();
            let mut lines = BufReader::new(read_half).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let reply = match parse_command(line) {
                    Ok(cmd) => {
                        controller.command(cmd);
                        "{\"ok\":true}\n".to_string()
                    }
                    Err(e) => {
                        format!("{{\"ok\":false,\"error\":{}}}\n", serde_json::json!(e))
                    }
                };
                if write_half.write_all(reply.as_bytes()).await.is_err() {
                    break;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        assert_eq!(
            parse_command(r#"{"cmd":"toggle"}"#).unwrap(),
            Command::Toggle
        );
        assert_eq!(parse_command(r#"{"cmd":"start"}"#).unwrap(), Command::Start);
        assert_eq!(parse_command(r#"{"cmd":"stop"}"#).unwrap(), Command::Stop);
        assert_eq!(
            parse_command(r#"{"cmd":"cancel"}"#).unwrap(),
            Command::Cancel
        );
        assert!(parse_command(r#"{"cmd":"nope"}"#).is_err());
        assert!(parse_command("garbage").is_err());
    }

    #[test]
    fn socket_path_prefers_xdg_runtime() {
        // Just shape-check the fallback logic without mutating global env.
        let p = socket_path();
        assert!(p.ends_with("una.sock"), "{p:?}");
    }
}
