//! Client for yagami: the coding-agent CLIs signed in on this machine (Claude
//! Code, Codex, OpenCode, …) served as one Anthropic-style API on loopback.
//!
//! una uses it for the "Fix up" button in the fix and review windows: the
//! transcript goes to whichever model the user picked in settings, on their
//! own subscriptions, with no API key of una's. The address and key come from
//! yagami's own config file, and una starts the server (`yagami start
//! --daemon`) when it isn't running, so nobody has to remember to.
//!
//! A GUI app launched from the Dock gets a bare PATH (`/usr/bin:/bin:…`),
//! where neither `yagami` nor the `node` it runs on can be found, and neither
//! could the CLIs it drives. The daemon is therefore started with the PATH
//! the user's interactive shell sets up, asked for once and remembered.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Fast and good at this: a relaxed three-sentence ramble comes back clean in
/// about 2.5s. Opus and GPT-6 were no better on it and took 6–8s.
pub const DEFAULT_MODEL: &str = "sonnet";
pub const DEFAULT_EFFORT: &str = "low";

const DEFAULT_URL: &str = "http://127.0.0.1:8787";
const HEALTH_TIMEOUT: Duration = Duration::from_millis(1500);
/// `yagami start --daemon` itself waits up to 15s for the server to be ready.
const START_TIMEOUT: Duration = Duration::from_secs(20);
/// The first model listing probes every CLI; later ones are cached by yagami.
const MODELS_TIMEOUT: Duration = Duration::from_secs(60);
/// A high-effort model on a long transcript can think for a while.
const COMPLETE_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, thiserror::Error)]
pub enum YagamiError {
    #[error("yagami isn't installed — `bun add -g @justin06lee/yagami`, or `make` in its repo")]
    NotInstalled,
    #[error("yagami didn't start: {0}")]
    StartFailed(String),
    #[error("yagami's config {path} can't be read: {reason}")]
    Config { path: String, reason: String },
    /// yagami's own message, e.g. a CLI that isn't signed in.
    #[error("{0}")]
    Api(String),
    #[error("couldn't reach yagami: {0}")]
    Http(#[from] reqwest::Error),
    #[error("the model sent back nothing")]
    Empty,
}

/// One model a CLI on this machine can run, as the settings window lists it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Model {
    /// What to send as `model`: a bare id for yagami's default provider,
    /// `"<provider>:<model>"` for the rest.
    pub id: String,
    /// The CLI that runs it (`claude`, `codex`, `opencode`, …).
    pub provider: String,
    pub name: String,
    /// Reasoning levels this model takes, lowest first; empty when it has none.
    pub efforts: Vec<String>,
    pub default_effort: Option<String>,
}

/// What's installed and runnable, for the settings window.
#[derive(Debug, Clone, Serialize)]
pub struct Inventory {
    pub version: Option<String>,
    /// Every CLI yagami found, including ones that reported no models.
    pub providers: Vec<String>,
    pub default_provider: Option<String>,
    pub models: Vec<Model>,
}

pub struct Yagami {
    http: reqwest::Client,
    base: String,
    key: String,
}

impl Yagami {
    /// Connect to the server on this machine, starting it first if it isn't up.
    pub async fn connect() -> Result<Self, YagamiError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .build()?;
        if let Some(client) = Self::running(&http).await {
            return Ok(client);
        }
        start_daemon().await?;
        let deadline = Instant::now() + START_TIMEOUT;
        loop {
            if let Some(client) = Self::running(&http).await {
                return Ok(client);
            }
            if Instant::now() >= deadline {
                return Err(YagamiError::StartFailed(
                    "it never answered — see ~/.config/yagami/yagami.log".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    /// A client for the server the config files describe, if it answers.
    async fn running(http: &reqwest::Client) -> Option<Self> {
        let (base, key) = host_endpoint().ok()??;
        let health: Health = http
            .get(format!("{base}/healthz"))
            .header("x-api-key", &key)
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        (health.ok && health.service.as_deref() == Some("yagami")).then(|| Self {
            http: http.clone(),
            base,
            key,
        })
    }

    /// Every model the installed CLIs report, and the CLIs themselves.
    pub async fn inventory(&self) -> Result<Inventory, YagamiError> {
        let health: Health = self
            .http
            .get(format!("{}/healthz", self.base))
            .header("x-api-key", &self.key)
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await?
            .json()
            .await?;
        let res = self
            .http
            .get(format!("{}/v1/models", self.base))
            .header("x-api-key", &self.key)
            .timeout(MODELS_TIMEOUT)
            .send()
            .await?;
        let list: ModelList = checked(res).await?.json().await?;
        Ok(Inventory {
            version: health.version,
            providers: health.providers,
            default_provider: health.provider,
            models: dedupe(list.data),
        })
    }

    /// One turn: the prompt in, the model's text out.
    pub async fn complete(
        &self,
        model: &str,
        effort: Option<&str>,
        prompt: &str,
    ) -> Result<String, YagamiError> {
        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": 4096,
            "messages": [{ "role": "user", "content": prompt }],
        });
        if let Some(effort) = effort.filter(|e| !e.is_empty()) {
            body["effort"] = effort.into();
        }
        let res = self
            .http
            .post(format!("{}/v1/messages", self.base))
            .header("x-api-key", &self.key)
            .timeout(COMPLETE_TIMEOUT)
            .json(&body)
            .send()
            .await?;
        let reply: MessageReply = checked(res).await?.json().await?;
        let text: String = reply
            .content
            .into_iter()
            .filter(|b| b.kind == "text")
            .filter_map(|b| b.text)
            .collect();
        Ok(text)
    }
}

/// Fix up a dictated transcript: what the user's own quick prompt to Claude
/// did, which read a rambling, um-filled request back as the clean sentence
/// they meant. The prompt is theirs, word for word; the additions only stop
/// a chat-tuned model from wrapping the answer or replying to the request
/// the transcript contains.
pub async fn fix_up(model: &str, effort: Option<&str>, text: &str) -> Result<String, YagamiError> {
    let client = Yagami::connect().await?;
    let reply = client.complete(model, effort, &fixup_prompt(text)).await?;
    let fixed = unwrap_reply(&reply);
    if fixed.is_empty() {
        return Err(YagamiError::Empty);
    }
    Ok(fixed)
}

pub fn fixup_prompt(text: &str) -> String {
    format!(
        "Just straight up fix up this transcription no context do ur best. \
         It's something I dictated, not a message to you, so don't answer it. \
         Reply with only the fixed text.\n\n<transcription>\n{}\n</transcription>",
        text.trim()
    )
}

/// The text alone, should the model echo the tags or quote its answer.
fn unwrap_reply(reply: &str) -> String {
    let mut s = reply.trim();
    if let Some(inner) = s
        .strip_prefix("<transcription>")
        .and_then(|r| r.strip_suffix("</transcription>"))
    {
        s = inner.trim();
    }
    for (open, close) in [("\"", "\""), ("“", "”")] {
        if let Some(inner) = s.strip_prefix(open).and_then(|r| r.strip_suffix(close)) {
            if !inner.contains(open) && !inner.contains(close) {
                s = inner.trim();
            }
        }
    }
    s.to_string()
}

// ---------------------------------------------------------------------------
// Wire shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct Health {
    #[serde(default)]
    ok: bool,
    service: Option<String>,
    version: Option<String>,
    /// Default provider; only with a valid key.
    provider: Option<String>,
    #[serde(default)]
    providers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ModelList {
    data: Vec<WireModel>,
}

#[derive(Debug, Deserialize)]
struct WireModel {
    id: String,
    display_name: Option<String>,
    provider: Option<String>,
    #[serde(default)]
    reasoning_efforts: Vec<WireEffort>,
    default_reasoning_effort: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WireEffort {
    id: String,
}

#[derive(Debug, Deserialize)]
struct MessageReply {
    #[serde(default)]
    content: Vec<Block>,
}

#[derive(Debug, Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    message: String,
}

/// The response, or yagami's error message when it answered with one.
async fn checked(res: reqwest::Response) -> Result<reqwest::Response, YagamiError> {
    if res.status().is_success() {
        return Ok(res);
    }
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    let message = serde_json::from_str::<ErrorEnvelope>(&body)
        .map(|e| e.error.message)
        .unwrap_or_else(|_| format!("yagami answered {status}"));
    Err(YagamiError::Api(message))
}

/// yagami lists the default provider's models twice, bare (`opus`) and
/// prefixed (`claude:opus`); keep the bare one, which is what people type.
fn dedupe(models: Vec<WireModel>) -> Vec<Model> {
    let bare: std::collections::HashSet<(String, String)> = models
        .iter()
        .filter(|m| !m.id.contains(':'))
        .map(|m| (m.provider.clone().unwrap_or_default(), m.id.clone()))
        .collect();
    models
        .into_iter()
        .filter(|m| {
            let provider = m.provider.clone().unwrap_or_default();
            match m.id.strip_prefix(&format!("{provider}:")) {
                Some(rest) => !bare.contains(&(provider, rest.to_string())),
                None => true,
            }
        })
        .map(|m| Model {
            name: m.display_name.unwrap_or_else(|| m.id.clone()),
            provider: m.provider.unwrap_or_else(|| "claude".into()),
            efforts: m.reasoning_efforts.into_iter().map(|e| e.id).collect(),
            default_effort: m.default_reasoning_effort,
            id: m.id,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Finding and starting the server
// ---------------------------------------------------------------------------

/// `YAGAMI_CONFIG_DIR`, else `~/.config/yagami` (on every OS, like yagami).
fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("YAGAMI_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    Some(directories::BaseDirs::new()?.home_dir().join(".config/yagami"))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostConfig {
    host: Option<String>,
    port: Option<u16>,
    #[serde(default)]
    api_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ServerState {
    url: Option<String>,
}

/// The running server's URL and a key for it; None until yagami has made one.
fn host_endpoint() -> Result<Option<(String, String)>, YagamiError> {
    let Some(dir) = config_dir() else {
        return Ok(None);
    };
    read_endpoint(&dir)
}

fn read_endpoint(dir: &Path) -> Result<Option<(String, String)>, YagamiError> {
    let path = dir.join("config.json");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(YagamiError::Config {
                path: path.display().to_string(),
                reason: e.to_string(),
            })
        }
    };
    let config: HostConfig = serde_json::from_str(&raw).map_err(|e| YagamiError::Config {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;
    let Some(key) = config.api_keys.into_iter().next() else {
        return Ok(None);
    };
    // The running server writes where it actually listens.
    let running = std::fs::read_to_string(dir.join("server.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<ServerState>(&s).ok())
        .and_then(|s| s.url);
    let base = running.unwrap_or_else(|| match (config.host, config.port) {
        (None, None) => DEFAULT_URL.to_string(),
        (host, port) => {
            let host = match host.as_deref() {
                // bound to every interface: loopback is one of them
                None | Some("0.0.0.0") | Some("::") => "127.0.0.1".to_string(),
                Some(h) => h.to_string(),
            };
            format!("http://{host}:{}", port.unwrap_or(8787))
        }
    });
    Ok(Some((base.trim_end_matches('/').to_string(), key)))
}

async fn start_daemon() -> Result<(), YagamiError> {
    tokio::task::spawn_blocking(|| {
        let path = login_path();
        let exe = find_on_path("yagami", path).ok_or(YagamiError::NotInstalled)?;
        tracing::info!(exe = %exe.display(), "starting yagami");
        let out = Command::new(&exe)
            .args(["start", "--daemon"])
            .env("PATH", path)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| YagamiError::StartFailed(e.to_string()))?;
        let stderr = String::from_utf8_lossy(&out.stderr);
        // "already running" is fine: something else started it meanwhile.
        if out.status.success() || stderr.contains("already running") {
            Ok(())
        } else {
            Err(YagamiError::StartFailed(
                stderr.lines().last().unwrap_or("no output").trim().to_string(),
            ))
        }
    })
    .await
    .map_err(|e| YagamiError::StartFailed(e.to_string()))?
}

/// The PATH the user's interactive login shell sets up, where `yagami`, the
/// `node` it runs on, and the agent CLIs live. Falls back to this process's
/// PATH plus the usual install directories.
fn login_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        shell_path().unwrap_or_else(|| {
            let mut dirs: Vec<String> = std::env::var("PATH")
                .unwrap_or_default()
                .split(':')
                .map(str::to_string)
                .collect();
            if let Some(home) = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()) {
                for d in [".local/bin", ".bun/bin", ".npm-global/bin", ".volta/bin"] {
                    dirs.push(home.join(d).display().to_string());
                }
            }
            dirs.extend(["/opt/homebrew/bin".into(), "/usr/local/bin".into()]);
            dirs.join(":")
        })
    })
}

fn shell_path() -> Option<String> {
    const MARK: &str = "__UNA_PATH__";
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    // Interactive, because rc files (~/.zshrc) are where most PATH lines live.
    let mut child = Command::new(shell)
        .args(["-ilc", &format!("printf '{MARK}%s{MARK}' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // An rc file that waits on something must not hang the fix window.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    let path = out.split(MARK).nth(1)?.trim();
    (!path.is_empty()).then(|| path.to_string())
}

fn find_on_path(name: &str, path: &str) -> Option<PathBuf> {
    path.split(':')
        .filter(|d| !d.is_empty())
        .map(|d| Path::new(d).join(name))
        .find(|p| is_executable(p))
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(id: &str, provider: &str, efforts: &[&str]) -> WireModel {
        WireModel {
            id: id.into(),
            display_name: Some(id.to_uppercase()),
            provider: Some(provider.into()),
            reasoning_efforts: efforts.iter().map(|e| WireEffort { id: (*e).into() }).collect(),
            default_reasoning_effort: None,
        }
    }

    #[test]
    fn default_provider_models_are_listed_once_bare() {
        let models = dedupe(vec![
            wire("opus", "claude", &["low", "max"]),
            wire("claude:opus", "claude", &["low", "max"]),
            wire("haiku", "claude", &[]),
            wire("codex:gpt-6-sol", "codex", &["low", "ultra"]),
        ]);
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["opus", "haiku", "codex:gpt-6-sol"]);
        assert_eq!(models[2].efforts, ["low", "ultra"]);
        assert!(models[1].efforts.is_empty());
    }

    #[test]
    fn prefixed_model_without_a_bare_twin_is_kept() {
        let models = dedupe(vec![wire("claude:odd", "claude", &[])]);
        assert_eq!(models[0].id, "claude:odd");
    }

    #[test]
    fn reply_is_unwrapped_from_echoed_tags_and_quotes() {
        assert_eq!(unwrap_reply("  Just this.  "), "Just this.");
        assert_eq!(
            unwrap_reply("<transcription>\nMake it longer.\n</transcription>"),
            "Make it longer."
        );
        assert_eq!(unwrap_reply("\"Make it longer.\""), "Make it longer.");
        assert_eq!(unwrap_reply("“Make it longer.”"), "Make it longer.");
        // quotes that belong to the text stay
        assert_eq!(
            unwrap_reply("\"a\" and then \"b\""),
            "\"a\" and then \"b\""
        );
    }

    #[test]
    fn prompt_carries_the_users_words_and_the_transcript() {
        let p = fixup_prompt("  um so like make it longer  ");
        assert!(p.starts_with("Just straight up fix up this transcription no context do ur best."));
        assert!(p.ends_with("<transcription>\num so like make it longer\n</transcription>"));
    }

    #[test]
    fn endpoint_comes_from_the_running_server_then_the_config() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_endpoint(dir.path()).unwrap().is_none(), "no config yet");

        std::fs::write(dir.path().join("config.json"), r#"{"port":9000,"apiKeys":[]}"#).unwrap();
        assert!(read_endpoint(dir.path()).unwrap().is_none(), "no key yet");

        std::fs::write(
            dir.path().join("config.json"),
            r#"{"host":"0.0.0.0","port":9000,"apiKeys":["ygm_a","ygm_b"]}"#,
        )
        .unwrap();
        assert_eq!(
            read_endpoint(dir.path()).unwrap(),
            Some(("http://127.0.0.1:9000".into(), "ygm_a".into()))
        );

        std::fs::write(
            dir.path().join("server.json"),
            r#"{"pid":1,"url":"http://127.0.0.1:8788/"}"#,
        )
        .unwrap();
        assert_eq!(
            read_endpoint(dir.path()).unwrap().unwrap().0,
            "http://127.0.0.1:8788"
        );

        std::fs::write(dir.path().join("config.json"), "{ nope").unwrap();
        assert!(matches!(
            read_endpoint(dir.path()),
            Err(YagamiError::Config { .. })
        ));
    }

    #[test]
    fn executables_are_found_on_a_given_path() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("yagami");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        let path = format!("/nonexistent:{}", dir.path().display());
        assert_eq!(find_on_path("yagami", &path), None, "not executable yet");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(find_on_path("yagami", &path), Some(exe));
        }
    }

    /// Talks to (and if need be starts) the real yagami on this machine:
    /// `cargo test -p una-core --lib yagami -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn live_fix_up_and_inventory() {
        let t = Instant::now();
        let fixed = fix_up(DEFAULT_MODEL, Some(DEFAULT_EFFORT), "um so like can you uh make the the button hold thing longer know what i mean")
            .await
            .expect("fix up");
        println!("fixed in {:?}: {fixed}", t.elapsed());
        let inv = Yagami::connect().await.unwrap().inventory().await.unwrap();
        println!("yagami {:?}, providers {:?}, {} models", inv.version, inv.providers, inv.models.len());
        assert!(inv.models.iter().any(|m| m.id == DEFAULT_MODEL));
    }
}
