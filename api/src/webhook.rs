//! Webhook til sitet, når noget går live, så det kan rydde sin cache.
//!
//! - `PUBLISH_WEBHOOK_URLS`: kommaseparerede adresser. Tom eller udeladt: ingen webhooks.
//! - `PUBLISH_WEBHOOK_SECRET`: nøgle til `X-Webmaster-Signature: sha256=<hex>`, en
//!   HMAC-SHA256 af selve body'en. Uden nøgle sendes der ingen signatur.
//!
//! Kaldene sendes i baggrunden og prøves op til `ATTEMPTS` gange. Publicering venter
//! aldrig på dem, og en fejl logges kun.

use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::Sha256;
use std::time::Duration;

const ATTEMPTS: u32 = 3;
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Event {
    /// Et dokument er publiceret.
    Publish,
    /// Et dokument er slettet.
    Delete,
    /// Sidetræet er ændret (ny sti, flyttet eller slettet side).
    Tree,
    /// Redirects er ændret.
    Redirects,
}

pub struct Webhooks {
    urls: Vec<String>,
    secret: Option<String>,
    client: Option<reqwest::Client>,
    /// Pause før forsøg nr. 2 og 3. Kort i tests.
    backoff: Duration,
}

impl Webhooks {
    pub fn from_env() -> Self {
        let urls = std::env::var("PUBLISH_WEBHOOK_URLS").unwrap_or_default();
        let secret = std::env::var("PUBLISH_WEBHOOK_SECRET").ok();
        let w = Self::new(urls.split(',').map(str::trim).filter(|u| !u.is_empty()).map(String::from).collect(), secret, Duration::from_secs(2));
        if !w.urls.is_empty() {
            tracing::info!(urls = w.urls.len(), signed = w.secret.is_some(), "webhooks ved publicering er slået til");
        }
        w
    }

    pub fn new(urls: Vec<String>, secret: Option<String>, backoff: Duration) -> Self {
        let client = reqwest::Client::builder().timeout(TIMEOUT).build().ok();
        Webhooks { urls, secret: secret.filter(|s| !s.is_empty()), client, backoff }
    }

    pub fn body(event: Event, id: Option<&str>, kind: Option<&str>, ty: Option<&str>, paths: Vec<String>, by: &str) -> Value {
        json!({ "event": event, "id": id, "kind": kind, "type": ty, "paths": paths, "by": by, "at": Utc::now() })
    }

    pub fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC tager nøgler af alle længder");
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    /// Sender `body` til alle adresser i baggrunden.
    pub fn send(&self, body: Value) -> Vec<tokio::task::JoinHandle<bool>> {
        let Some(client) = &self.client else { return vec![] };
        let bytes = serde_json::to_vec(&body).unwrap_or_default();
        let sig = self.secret.as_deref().map(|s| Self::sign(s, &bytes));
        self.urls
            .iter()
            .map(|url| {
                let (client, url, bytes, sig, backoff) = (client.clone(), url.clone(), bytes.clone(), sig.clone(), self.backoff);
                tokio::spawn(async move {
                    for attempt in 1..=ATTEMPTS {
                        let mut req = client.post(&url).header("content-type", "application/json").body(bytes.clone());
                        if let Some(s) = &sig {
                            req = req.header("x-webmaster-signature", s);
                        }
                        match req.send().await {
                            Ok(r) if r.status().is_success() => return true,
                            Ok(r) => tracing::warn!(url, attempt, status = %r.status(), "webhook fejlede"),
                            Err(e) => tracing::warn!(url, attempt, "webhook fejlede: {e}"),
                        }
                        if attempt < ATTEMPTS {
                            tokio::time::sleep(backoff * attempt).await;
                        }
                    }
                    tracing::error!(url, "webhook opgivet efter {ATTEMPTS} forsøg");
                    false
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Bytes, http::{HeaderMap, StatusCode}, routing::post, Router};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn signs_like_github() {
        // Kendt testvektor for HMAC-SHA256.
        assert_eq!(
            Webhooks::sign("key", b"The quick brown fox jumps over the lazy dog"),
            "sha256=f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[tokio::test]
    async fn retries_and_signs() {
        let calls = Arc::new(AtomicU32::new(0));
        let got: Arc<Mutex<Option<(String, Bytes)>>> = Arc::default();
        let (c, g) = (calls.clone(), got.clone());
        let app = Router::new().route(
            "/hook",
            post(move |h: HeaderMap, body: Bytes| async move {
                // Første kald fejler, andet lykkes.
                if c.fetch_add(1, Ordering::SeqCst) == 0 {
                    return StatusCode::BAD_GATEWAY;
                }
                *g.lock().unwrap() = Some((h["x-webmaster-signature"].to_str().unwrap().to_string(), body));
                StatusCode::NO_CONTENT
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let w = Webhooks::new(vec![format!("http://{addr}/hook"), "http://127.0.0.1:1/nede".into()], Some("hemmelig".into()), Duration::from_millis(10));
        let body = Webhooks::body(Event::Publish, Some("pg_1"), Some("page"), None, vec!["/nyheder".into()], "a@b.dk");
        let results: Vec<bool> = futures::future::join_all(w.send(body)).await.into_iter().map(Result::unwrap).collect();
        assert_eq!(results, vec![true, false]);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let (sig, bytes) = got.lock().unwrap().clone().unwrap();
        assert_eq!(sig, Webhooks::sign("hemmelig", &bytes));
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!((v["event"].as_str(), v["paths"][0].as_str()), (Some("publish"), Some("/nyheder")));
    }

    #[test]
    fn off_without_urls() {
        let w = Webhooks::new(vec![], None, Duration::ZERO);
        assert!(w.urls.is_empty());
    }
}
