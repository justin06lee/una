//! HTTP client for the una server API.
//!
//! Contract: `POST {base}/v1/dictations` as multipart/form-data with an
//! `audio` file part (16 kHz mono s16le WAV) plus text fields, and
//! `GET {base}/v1/health`. Connect timeout 3s, total timeout 60s (health:
//! 1.5s). Exactly one automatic retry on connect/reset errors with the same
//! `utterance_id`; never retried on HTTP status errors.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::state::ErrKind;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(60);
pub const HEALTH_TIMEOUT: Duration = Duration::from_millis(1500);

/// Client identification string sent with each dictation.
pub fn client_string() -> String {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    };
    let arch = std::env::consts::ARCH;
    // Shape: "una-desktop/0.1.0 macos-aarch64"
    format!("una-desktop/{} {os}-{arch}", env!("CARGO_PKG_VERSION"))
}

#[derive(Debug, Clone, Deserialize)]
pub struct Timings {
    pub transcribe_ms: Option<f64>,
    pub cleanup_ms: Option<f64>,
    pub total_ms: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DictationResponse {
    pub id: Option<String>,
    /// The text to insert.
    pub text: String,
    pub raw_text: Option<String>,
    pub cleaned_text: Option<String>,
    pub cleanup_applied: Option<bool>,
    pub duration_ms: Option<f64>,
    pub timings: Option<Timings>,
    pub asr_model: Option<String>,
    pub llm_model: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Health {
    pub status: Option<String>,
    pub asr_model_loaded: Option<bool>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiErrorEnvelope {
    error: ApiErrorBody,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiErrorBody {
    code: Option<String>,
    message: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("could not connect to the server")]
    Connect(#[source] reqwest::Error),
    #[error("the request timed out")]
    Timeout(#[source] reqwest::Error),
    #[error("server error {status}: {message}")]
    Api { status: u16, code: String, message: String },
    #[error("could not decode the server response: {0}")]
    Decode(String),
    #[error("request failed: {0}")]
    Other(#[source] reqwest::Error),
}

impl NetError {
    pub fn kind(&self) -> ErrKind {
        match self {
            NetError::Connect(_) => ErrKind::Connect,
            NetError::Timeout(_) => ErrKind::Timeout,
            NetError::Api { .. } => ErrKind::Server,
            NetError::Decode(_) => ErrKind::Decode,
            NetError::Other(_) => ErrKind::Other,
        }
    }

    /// Whether "Retry Last Dictation" is likely to help.
    pub fn retryable(&self) -> bool {
        matches!(self, NetError::Connect(_) | NetError::Timeout(_) | NetError::Other(_))
    }
}

#[derive(Debug, Clone)]
pub struct DictationRequest {
    pub wav: Vec<u8>,
    /// Client-generated UUID v4; the server dedupes retries on it.
    pub utterance_id: String,
    /// Frontmost application name; omitted when unknown.
    pub app_name: Option<String>,
    pub clean: bool,
}

impl DictationRequest {
    pub fn new(wav: Vec<u8>, app_name: Option<String>) -> Self {
        Self { wav, utterance_id: uuid::Uuid::new_v4().to_string(), app_name, clean: true }
    }
}

#[derive(Clone)]
pub struct ApiClient {
    http: reqwest::Client,
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TOTAL_TIMEOUT)
            .build()
            .expect("build http client");
        Self { http }
    }

    /// POST a dictation. Performs exactly one automatic retry (same
    /// utterance_id) when the transport failed to connect or the connection
    /// was reset. HTTP status errors are never retried.
    pub async fn dictate(
        &self,
        base: &str,
        req: &DictationRequest,
    ) -> Result<DictationResponse, NetError> {
        let mut attempts = 0u8;
        loop {
            attempts += 1;
            match self.dictate_once(base, req).await {
                Ok(resp) => return Ok(resp),
                Err(err) => {
                    let transport_retryable = matches!(&err, NetError::Connect(_))
                        || matches!(&err, NetError::Other(e) if is_reset(e));
                    if attempts == 1 && transport_retryable {
                        tracing::info!("retrying dictation upload once (transport error): {err}");
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }

    async fn dictate_once(
        &self,
        base: &str,
        req: &DictationRequest,
    ) -> Result<DictationResponse, NetError> {
        let url = format!("{}/v1/dictations", base.trim_end_matches('/'));
        let file = reqwest::multipart::Part::bytes(req.wav.clone())
            .file_name("utterance.wav")
            .mime_str("audio/wav")
            .map_err(NetError::Other)?;
        let mut form = reqwest::multipart::Form::new()
            .part("audio", file)
            .text("utterance_id", req.utterance_id.clone())
            .text("clean", if req.clean { "true" } else { "false" })
            .text("client", client_string());
        if let Some(app) = &req.app_name {
            form = form.text("app_name", app.clone());
        }

        let resp = self.http.post(&url).multipart(form).send().await.map_err(classify)?;
        let status = resp.status();
        let body = resp.bytes().await.map_err(classify)?;
        if status.is_success() {
            serde_json::from_slice::<DictationResponse>(&body)
                .map_err(|e| NetError::Decode(e.to_string()))
        } else {
            Err(api_error(status.as_u16(), &body))
        }
    }

    /// GET /v1/health with a short (1.5s) timeout.
    pub async fn health(&self, base: &str) -> Result<Health, NetError> {
        let url = format!("{}/v1/health", base.trim_end_matches('/'));
        let resp = self
            .http
            .get(&url)
            .timeout(HEALTH_TIMEOUT)
            .send()
            .await
            .map_err(classify)?;
        let status = resp.status();
        let body = resp.bytes().await.map_err(classify)?;
        if status.is_success() {
            serde_json::from_slice::<Health>(&body).map_err(|e| NetError::Decode(e.to_string()))
        } else {
            Err(api_error(status.as_u16(), &body))
        }
    }
}

fn api_error(status: u16, body: &[u8]) -> NetError {
    match serde_json::from_slice::<ApiErrorEnvelope>(body) {
        Ok(env) => NetError::Api {
            status,
            code: env.error.code.unwrap_or_else(|| "unknown".into()),
            message: env.error.message.unwrap_or_else(|| "unknown server error".into()),
        },
        Err(_) => NetError::Api {
            status,
            code: "unknown".into(),
            message: format!("HTTP {status}"),
        },
    }
}

fn classify(e: reqwest::Error) -> NetError {
    if e.is_timeout() {
        NetError::Timeout(e)
    } else if e.is_connect() {
        NetError::Connect(e)
    } else {
        NetError::Other(e)
    }
}

/// Walk the error chain looking for a connection-reset io error.
fn is_reset(e: &reqwest::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(e);
    while let Some(err) = source {
        if let Some(io) = err.downcast_ref::<std::io::Error>() {
            if matches!(
                io.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
            ) {
                return true;
            }
        }
        source = err.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_string_shape() {
        let s = client_string();
        assert!(s.starts_with("una-desktop/"), "{s}");
        assert!(s.contains(' '), "{s}");
        #[cfg(target_os = "macos")]
        assert!(s.ends_with(&format!("macos-{}", std::env::consts::ARCH)), "{s}");
    }

    #[test]
    fn api_error_parses_contract_envelope() {
        let body = br#"{"error":{"code":"asr_failed","message":"model not loaded"}}"#;
        match api_error(503, body) {
            NetError::Api { status, code, message } => {
                assert_eq!(status, 503);
                assert_eq!(code, "asr_failed");
                assert_eq!(message, "model not loaded");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn api_error_tolerates_garbage() {
        match api_error(500, b"<html>oops</html>") {
            NetError::Api { status, code, .. } => {
                assert_eq!(status, 500);
                assert_eq!(code, "unknown");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn dictation_response_parses() {
        let body = br#"{
            "id":"d1","text":"hello","raw_text":"uh hello","cleaned_text":"hello",
            "cleanup_applied":true,"duration_ms":1200.5,
            "timings":{"transcribe_ms":300,"cleanup_ms":90,"total_ms":400},
            "asr_model":"m","llm_model":"l"
        }"#;
        let resp: DictationResponse = serde_json::from_slice(body).unwrap();
        assert_eq!(resp.text, "hello");
        assert_eq!(resp.timings.unwrap().transcribe_ms, Some(300.0));
    }
}
