//! Picking a reachable una server out of several candidates.
//!
//! The same laptop is on the home LAN in the morning and on a hotel network in
//! the evening, and the server is only reachable by a different address in each
//! case. Rather than making the user switch settings, the client keeps an
//! ordered list of candidate URLs and, before each dictation, uses whichever one
//! actually answers.
//!
//! Candidates are probed **concurrently** and the first success wins, so a dead
//! address never costs more than the health timeout, and an unreachable LAN
//! address doesn't delay the remote path. The winner is cached — resolution runs
//! on the dictation hot path, so the steady state must be a lock read, not a
//! round trip.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::net::ApiClient;

/// How long a resolved endpoint is reused before being re-probed. Roaming is
/// normally caught by [`EndpointResolver::invalidate`] on upload failure; this
/// is the backstop for a network that changed without any request failing.
pub const CACHE_TTL: Duration = Duration::from_secs(300);

/// Per-candidate probe timeout. Longer than the 1.5s health check used by the
/// settings window: this one may be crossing a phone tether.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(2500);

/// How long an mDNS browse runs when nothing configured is reachable.
pub const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cached {
    url: String,
    at: Instant,
}

pub struct EndpointResolver {
    api: ApiClient,
    cached: Mutex<Option<Cached>>,
    ttl: Duration,
}

impl EndpointResolver {
    pub fn new(api: ApiClient) -> Self {
        Self {
            api,
            cached: Mutex::new(None),
            ttl: CACHE_TTL,
        }
    }

    #[cfg(test)]
    fn with_ttl(api: ApiClient, ttl: Duration) -> Self {
        Self {
            api,
            cached: Mutex::new(None),
            ttl,
        }
    }

    /// The endpoint chosen last time, if it hasn't gone stale.
    pub fn cached(&self) -> Option<String> {
        let guard = self.cached.lock().unwrap();
        guard
            .as_ref()
            .filter(|c| c.at.elapsed() < self.ttl)
            .map(|c| c.url.clone())
    }

    /// Forget the current endpoint. Called when a request against it failed, so
    /// the next dictation re-probes instead of retrying a dead address.
    pub fn invalidate(&self) {
        *self.cached.lock().unwrap() = None;
    }

    fn remember(&self, url: &str) {
        *self.cached.lock().unwrap() = Some(Cached {
            url: url.to_string(),
            at: Instant::now(),
        });
    }

    /// Resolve a usable base URL: cached value, else the first configured
    /// candidate that answers, else an mDNS-discovered one.
    pub async fn resolve(&self, candidates: &[String], autodiscover: bool) -> Option<String> {
        if let Some(url) = self.cached() {
            return Some(url);
        }
        if let Some(url) = self.race(candidates).await {
            self.remember(&url);
            return Some(url);
        }
        if autodiscover {
            let found = tokio::task::spawn_blocking(|| crate::discovery::discover(DISCOVERY_TIMEOUT))
                .await
                .unwrap_or_default();
            let urls: Vec<String> = found.into_iter().map(|s| s.url).collect();
            if let Some(url) = self.race(&urls).await {
                self.remember(&url);
                return Some(url);
            }
        }
        None
    }

    /// Probe every candidate at once; the first to answer wins.
    async fn race(&self, candidates: &[String]) -> Option<String> {
        let candidates: Vec<String> = candidates
            .iter()
            .map(|u| u.trim().trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty())
            .collect();
        match candidates.len() {
            0 => return None,
            // One candidate: probe inline rather than paying for a task.
            1 => {
                let url = candidates.into_iter().next()?;
                return self
                    .api
                    .reachable(&url, PROBE_TIMEOUT)
                    .await
                    .then_some(url);
            }
            _ => {}
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Option<String>>(candidates.len());
        for url in &candidates {
            let api = self.api.clone();
            let url = url.clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let ok = api.reachable(&url, PROBE_TIMEOUT).await;
                let _ = tx.send(ok.then_some(url)).await;
            });
        }
        drop(tx);

        let mut pending = candidates.len();
        while pending > 0 {
            match rx.recv().await {
                Some(Some(url)) => return Some(url),
                Some(None) => pending -= 1,
                None => break,
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Minimal stand-in for the server's `GET /v1/health`. Returns its base URL
    /// and a counter of how many requests it served.
    async fn fake_server(delay: Duration) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let counter = counter.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }
                    counter.fetch_add(1, Ordering::SeqCst);
                    let body = r#"{"status":"ok","asr_model_loaded":true}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                });
            }
        });
        (format!("http://{addr}"), hits)
    }

    /// A port nothing is listening on, to stand in for an unreachable endpoint.
    async fn dead_url() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{addr}")
    }

    fn resolver() -> EndpointResolver {
        EndpointResolver::new(ApiClient::new())
    }

    #[tokio::test]
    async fn resolves_the_only_reachable_candidate() {
        let (live, _) = fake_server(Duration::ZERO).await;
        let dead = dead_url().await;
        let r = resolver();
        let got = r.resolve(&[dead.clone(), live.clone()], false).await;
        assert_eq!(got, Some(live));
    }

    #[tokio::test]
    async fn a_dead_first_candidate_does_not_hide_a_live_one() {
        // The ordering that matters in practice: LAN address listed first,
        // stale while roaming, with the remote address behind it.
        let dead = dead_url().await;
        let (live, _) = fake_server(Duration::ZERO).await;
        let r = resolver();
        assert_eq!(r.resolve(&[dead, live.clone()], false).await, Some(live));
    }

    #[tokio::test]
    async fn fastest_candidate_wins_the_race() {
        let (slow, _) = fake_server(Duration::from_millis(400)).await;
        let (fast, _) = fake_server(Duration::ZERO).await;
        let r = resolver();
        assert_eq!(r.resolve(&[slow, fast.clone()], false).await, Some(fast));
    }

    #[tokio::test]
    async fn returns_none_when_nothing_answers() {
        let a = dead_url().await;
        let b = dead_url().await;
        let r = resolver();
        assert_eq!(r.resolve(&[a, b], false).await, None);
    }

    #[tokio::test]
    async fn empty_and_blank_candidates_are_ignored() {
        let r = resolver();
        assert_eq!(r.resolve(&[], false).await, None);
        assert_eq!(r.resolve(&["   ".into()], false).await, None);
    }

    #[tokio::test]
    async fn result_is_cached_so_the_hot_path_does_not_reprobe() {
        let (live, hits) = fake_server(Duration::ZERO).await;
        let r = resolver();
        assert_eq!(r.resolve(&[live.clone()], false).await, Some(live.clone()));
        let after_first = hits.load(Ordering::SeqCst);
        for _ in 0..5 {
            assert_eq!(r.resolve(&[live.clone()], false).await, Some(live.clone()));
        }
        assert_eq!(hits.load(Ordering::SeqCst), after_first, "cache should serve repeats");
    }

    #[tokio::test]
    async fn invalidate_forces_a_reprobe() {
        let (live, hits) = fake_server(Duration::ZERO).await;
        let r = resolver();
        r.resolve(&[live.clone()], false).await;
        r.invalidate();
        assert!(r.cached().is_none());
        r.resolve(&[live.clone()], false).await;
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cache_expires_after_the_ttl() {
        let (live, hits) = fake_server(Duration::ZERO).await;
        let r = EndpointResolver::with_ttl(ApiClient::new(), Duration::from_millis(60));
        r.resolve(&[live.clone()], false).await;
        tokio::time::sleep(Duration::from_millis(90)).await;
        assert!(r.cached().is_none(), "entry should have gone stale");
        r.resolve(&[live.clone()], false).await;
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn trailing_slashes_are_normalized_away() {
        let (live, _) = fake_server(Duration::ZERO).await;
        let r = resolver();
        let got = r.resolve(&[format!("{live}/")], false).await;
        assert_eq!(got, Some(live));
    }
}
