//! Valgmuligheder fra andre services (`optionsFrom`).
//!
//! Webmasteren må ikke blive afhængig af servicene. Derfor:
//! - En service er kun "sat op", når alle dens miljøvariabler findes.
//! - Hentede lister caches i hukommelsen. Fejler et kald, bruges den seneste
//!   liste, der virkede (`stale`). Er der ingen, bruger editoren `options`
//!   fra definitionen (`fallback`).
//! - Et kald, der lige er fejlet, prøves først igen efter `RETRY_AFTER`,
//!   så en service, der er nede, ikke gør editoren langsom.

use crate::{defs::Defs, error::R, AppState};
use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use ts_rs::TS;

/* ---------- services.json ---------- */

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct ServiceDef {
    /// Fx "${EDITORIAL_URL}". `${NAVN}` erstattes med miljøvariablen.
    pub base_url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(type = "Record<string, string>", optional)]
    pub headers: BTreeMap<String, String>,
    /// Hvor længe en hentet liste er frisk, i sekunder. Standard 300.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(as = "Option<u32>")]
    pub cache_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<BTreeMap<String, ListDef>>", optional)]
    pub lists: BTreeMap<String, ListDef>,
    /// Udsnit af data, som datasources kan vise i editoren (`preview` på datasource-typen).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(as = "Option<BTreeMap<String, crate::previews::PreviewDef>>", optional)]
    pub previews: BTreeMap<String, crate::previews::PreviewDef>,
}

/// Hvordan en liste hentes og læses. `items`, `value` og `label` er JSON-pointere.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct ListDef {
    /// Stien efter `baseUrl`, fx "/sections".
    pub path: String,
    /// Hvor listen står i svaret, fx "/data". Tom eller udeladt: svaret er selv listen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<String>,
    /// Værdien i hvert punkt, fx "/slug". Tom: punktet er selv værdien.
    #[serde(default)]
    #[ts(as = "Option<String>", optional)]
    pub value: String,
    /// Teksten i hvert punkt, fx "/name". Udeladt: værdien bruges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

const DEFAULT_CACHE: u64 = 300;
pub(crate) const RETRY_AFTER: Duration = Duration::from_secs(30);
pub(crate) const TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) fn is_pointer(p: &str) -> bool {
    p.is_empty() || p.starts_with('/')
}

/// Problemer i én service i services.json.
pub fn check(name: &str, s: &ServiceDef) -> Vec<String> {
    let mut out = vec![];
    if s.lists.is_empty() && s.previews.is_empty() {
        out.push(format!("servicen «{name}» har hverken lister eller previews"));
    }
    for (k, l) in &s.lists {
        let at = format!("«{name}:{k}»");
        if !l.path.starts_with('/') {
            out.push(format!("{at}: «path» skal starte med /"));
        }
        for (prop, p) in [("items", l.items.as_deref()), ("value", Some(l.value.as_str())), ("label", l.label.as_deref())] {
            if let Some(p) = p {
                if !is_pointer(p) {
                    out.push(format!("{at}: «{prop}» skal være en JSON-pointer som \"/navn\", ikke \"{p}\""));
                }
            }
        }
    }
    for (k, p) in &s.previews {
        out.extend(crate::previews::check(&format!("«{name}:{k}»"), p));
    }
    out
}

/// Erstatter `${NAVN}` med miljøvariabler. Fejler med navnet på den første, der mangler.
pub(crate) fn interpolate(s: &str, env: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("${") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let Some(j) = after.find('}') else {
            out.push_str(&rest[i..]);
            return Ok(out);
        };
        let name = &after[..j];
        out.push_str(&env(name).filter(|v| !v.is_empty()).ok_or_else(|| name.to_string())?);
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Læser punkterne ud af et svar.
fn parse(l: &ListDef, body: &Value) -> Result<Vec<OptionItem>, String> {
    let items = body
        .pointer(l.items.as_deref().unwrap_or(""))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("svaret har ingen liste på «{}»", l.items.as_deref().unwrap_or("")))?;
    let text = |v: Option<&Value>| match v {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    };
    let out: Vec<OptionItem> = items
        .iter()
        .filter_map(|it| {
            let value = text(it.pointer(&l.value))?;
            let label = l.label.as_deref().and_then(|p| text(it.pointer(p))).unwrap_or_else(|| value.clone());
            Some(OptionItem { value, label })
        })
        .collect();
    if out.is_empty() {
        return Err(format!("fandt ingen værdier på «{}» i listen", l.value));
    }
    Ok(out)
}

/* ---------- cache ---------- */

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct OptionItem {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum OptionStatus {
    /// Hentet for nylig.
    Fresh,
    /// Servicen svarer ikke. Listen er den seneste, der virkede.
    Stale,
    /// Ingen liste. Brug `options` fra definitionen.
    Fallback,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OptionList {
    pub status: OptionStatus,
    /// `null` ved `fallback`.
    pub items: Option<Vec<OptionItem>>,
    pub fetched_at: Option<DateTime<Utc>>,
    /// Hvorfor listen ikke er frisk.
    pub error: Option<String>,
}

#[derive(Default)]
struct Entry {
    items: Option<Vec<OptionItem>>,
    fetched_at: Option<DateTime<Utc>>,
    fetched: Option<Instant>,
    error: Option<String>,
    failed: Option<Instant>,
}

pub struct OptionsCache {
    /// `Err` hvis klienten ikke kunne laves (fx ingen CA-certifikater). Så bruges reservelisterne.
    client: Result<reqwest::Client, String>,
    entries: Mutex<HashMap<String, Entry>>,
    /// Udsnit til editoren, se `previews.rs`.
    pub(crate) previews: Mutex<HashMap<String, crate::previews::Entry>>,
    env: Box<dyn Fn(&str) -> Option<String> + Send + Sync>,
}

impl OptionsCache {
    pub fn new() -> Self {
        Self::with_env(|k| std::env::var(k).ok())
    }

    pub fn with_env(env: impl Fn(&str) -> Option<String> + Send + Sync + 'static) -> Self {
        let client = reqwest::Client::builder().timeout(TIMEOUT).build().map_err(|e| {
            tracing::error!("kan ikke hente fra services, bruger reservelister: {e}");
            format!("http-klienten kunne ikke startes: {e}")
        });
        OptionsCache { client, entries: Mutex::new(HashMap::new()), previews: Mutex::new(HashMap::new()), env: Box::new(env) }
    }

    fn snapshot(&self, key: &str, status: OptionStatus, error: Option<String>) -> OptionList {
        let entries = self.entries.lock().unwrap();
        let e = entries.get(key);
        let items = e.and_then(|e| e.items.clone());
        let status = if items.is_none() { OptionStatus::Fallback } else { status };
        OptionList { status, items, fetched_at: e.and_then(|e| e.fetched_at), error }
    }

    /// Henter en liste ("service:liste") eller giver den bedste, der er.
    pub async fn get(&self, defs: &Defs, key: &str) -> OptionList {
        let fallback = |error: String| OptionList { status: OptionStatus::Fallback, items: None, fetched_at: None, error: Some(error) };
        let Some((svc_name, list_name)) = key.split_once(':') else { return fallback(format!("«{key}» er ikke på formen service:liste")) };
        let Some(svc) = defs.services.get(svc_name) else { return fallback(format!("servicen «{svc_name}» står ikke i services.json")) };
        let Some(list) = svc.lists.get(list_name) else { return fallback(format!("listen «{key}» står ikke i services.json")) };
        let ttl = Duration::from_secs(svc.cache_seconds.unwrap_or(DEFAULT_CACHE));

        let cached = {
            let entries = self.entries.lock().unwrap();
            entries.get(key).and_then(|e| {
                if e.fetched.is_some_and(|t| t.elapsed() < ttl) {
                    Some((OptionStatus::Fresh, None))
                } else if e.failed.is_some_and(|t| t.elapsed() < RETRY_AFTER) {
                    Some((OptionStatus::Stale, e.error.clone()))
                } else {
                    None
                }
            })
        };
        if let Some((status, err)) = cached {
            return self.snapshot(key, status, err);
        }

        let result = self.fetch(svc_name, svc, list).await;
        let mut entries = self.entries.lock().unwrap();
        let e = entries.entry(key.to_string()).or_default();
        match result {
            Ok(items) => {
                e.items = Some(items);
                e.fetched_at = Some(Utc::now());
                e.fetched = Some(Instant::now());
                e.error = None;
                e.failed = None;
                drop(entries);
                self.snapshot(key, OptionStatus::Fresh, None)
            }
            Err(err) => {
                tracing::warn!(list = key, "kunne ikke hente valgmuligheder: {err}");
                e.error = Some(err.clone());
                e.failed = Some(Instant::now());
                drop(entries);
                self.snapshot(key, OptionStatus::Stale, Some(err))
            }
        }
    }

    async fn fetch(&self, name: &str, svc: &ServiceDef, list: &ListDef) -> Result<Vec<OptionItem>, String> {
        let body = self.get_json(name, svc, &list.path, &[]).await?;
        parse(list, &body)
    }

    /// Henter JSON fra en service. Fejlen er en kort forklaring til redaktøren.
    pub(crate) async fn get_json(&self, name: &str, svc: &ServiceDef, path: &str, query: &[(String, String)]) -> Result<Value, String> {
        let not_set = |var: String| format!("«{name}» er ikke sat op ({var} mangler)");
        let base = interpolate(&svc.base_url, &*self.env).map_err(not_set)?;
        let client = self.client.as_ref().map_err(Clone::clone)?;
        let url = reqwest::Url::parse_with_params(&format!("{}{}", base.trim_end_matches('/'), path), query)
            .map_err(|_| format!("«{name}» har en ugyldig adresse"))?;
        let mut req = client.get(url);
        for (k, v) in &svc.headers {
            req = req.header(k, interpolate(v, &*self.env).map_err(not_set)?);
        }
        let res = req.send().await.map_err(|e| {
            if e.is_timeout() {
                format!("«{name}» svarede ikke inden for {} sekunder", TIMEOUT.as_secs())
            } else {
                format!("«{name}» kunne ikke kontaktes")
            }
        })?;
        if !res.status().is_success() {
            return Err(format!("«{name}» svarede {}", res.status()));
        }
        res.json().await.map_err(|_| format!("«{name}» svarede ikke med JSON"))
    }

    /// Alle lister, felterne i definitionerne henter fra, som værdier. Kun dem, der findes
    /// (frisk eller gammel). Bruges af valideringen.
    pub async fn all_values(&self, defs: &Defs) -> HashMap<String, Vec<String>> {
        let keys: Vec<String> = defs
            .services
            .iter()
            .flat_map(|(n, s)| s.lists.keys().map(move |k| format!("{n}:{k}")))
            .collect();
        let lists = futures::future::join_all(keys.iter().map(|k| self.get(defs, k))).await;
        keys.into_iter()
            .zip(lists)
            .filter_map(|(k, l)| Some((k, l.items?.into_iter().map(|i| i.value).collect())))
            .collect()
    }
}

pub async fn get_options(State(s): State<AppState>, Path((service, list)): Path<(String, String)>) -> R<Json<OptionList>> {
    let defs = crate::defs::load(&s.defs_dir);
    Ok(Json(s.options.get(&defs, &format!("{service}:{list}")).await))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn defs_with(base_url: &str, headers: &[(&str, &str)]) -> Defs {
        let mut d = crate::defs::load(std::path::Path::new("../definitions"));
        let svc: ServiceDef = serde_json::from_value(json!({
            "baseUrl": base_url,
            "headers": headers.iter().cloned().collect::<BTreeMap<_, _>>(),
            "cacheSeconds": 1,
            "lists": { "sections": { "path": "/sections", "items": "/data", "value": "/slug", "label": "/name" } }
        }))
        .unwrap();
        d.services = [("ed".to_string(), svc)].into();
        d
    }

    #[test]
    fn interpolates() {
        let env = |k: &str| (k == "A").then(|| "x".to_string());
        assert_eq!(interpolate("${A}/y", &env).unwrap(), "x/y");
        assert_eq!(interpolate("Bearer ${B}", &env).unwrap_err(), "B");
        assert_eq!(interpolate("intet", &env).unwrap(), "intet");
    }

    #[test]
    fn parses_lists() {
        let l: ListDef = serde_json::from_value(json!({ "path": "/x", "items": "/data", "value": "/id", "label": "/n" })).unwrap();
        let items = parse(&l, &json!({ "data": [{ "id": 1, "n": "En" }, { "id": "b" }, { "n": "uden id" }] })).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].value.as_str(), items[0].label.as_str()), ("1", "En"));
        assert_eq!(items[1].label, "b");
        let plain: ListDef = serde_json::from_value(json!({ "path": "/x" })).unwrap();
        assert_eq!(parse(&plain, &json!(["a", "b"])).unwrap().len(), 2);
        assert!(parse(&l, &json!({ "data": [] })).is_err());
        assert!(parse(&l, &json!({ "andet": [] })).is_err());
    }

    #[test]
    fn checks_services() {
        let s: ServiceDef = serde_json::from_value(json!({ "baseUrl": "x", "lists": { "a": { "path": "a", "value": "slug" } } })).unwrap();
        let p = check("ed", &s).join("\n");
        assert!(p.contains("«path» skal starte med /"), "{p}");
        assert!(p.contains("«value» skal være en JSON-pointer"), "{p}");
    }

    #[tokio::test]
    async fn not_configured_gives_fallback() {
        let cache = OptionsCache::with_env(|_| None);
        let l = cache.get(&defs_with("${ED_URL}", &[]), "ed:sections").await;
        assert_eq!(l.status, OptionStatus::Fallback);
        assert!(l.error.unwrap().contains("ED_URL mangler"));
        let l = cache.get(&defs_with("x", &[]), "ed:findes_ikke").await;
        assert_eq!(l.status, OptionStatus::Fallback);
    }

    /// Henter fra en rigtig server, og bruger den gamle liste, når serveren er væk.
    #[tokio::test]
    async fn fresh_then_stale_when_service_goes_down() {
        use axum::{http::{HeaderMap, StatusCode}, response::IntoResponse, routing::get, Router};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let down = Arc::new(AtomicBool::new(false));
        let d = down.clone();
        let app = Router::new().route(
            "/sections",
            get(move |h: HeaderMap| async move {
                if d.load(Ordering::SeqCst) {
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
                assert_eq!(h.get("authorization").unwrap(), "Bearer hemmelig");
                Json(json!({ "data": [{ "slug": "sport", "name": "Sport" }] })).into_response()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let cache = OptionsCache::with_env(move |k| match k {
            "ED_URL" => Some(format!("http://{addr}")),
            "ED_TOKEN" => Some("hemmelig".into()),
            _ => None,
        });
        let defs = defs_with("${ED_URL}", &[("Authorization", "Bearer ${ED_TOKEN}")]);
        let l = cache.get(&defs, "ed:sections").await;
        assert_eq!(l.status, OptionStatus::Fresh, "{:?}", l.error);
        assert_eq!(l.items.unwrap()[0].label, "Sport");

        down.store(true, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(1100)).await; // cacheSeconds: 1
        let l = cache.get(&defs, "ed:sections").await;
        assert_eq!(l.status, OptionStatus::Stale);
        assert_eq!(l.items.unwrap()[0].value, "sport");
        assert!(l.error.unwrap().contains("503"));

        let values = cache.all_values(&defs).await;
        assert_eq!(values["ed:sections"], vec!["sport".to_string()]);
    }
}
