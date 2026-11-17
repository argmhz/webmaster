//! Udsnit af data fra en datasource, så redaktøren kan se, hvad den giver.
//!
//! Udsnittet er kun til orientering. Det påvirker aldrig gem, publicering eller
//! validering, og endpointet svarer altid 200, også når servicen er nede.
//!
//! - `services.json` beskriver, hvordan et udsnit læses (`previews`).
//! - Datasource-typen peger på det og giver query-skabelonen:
//!   `"preview": { "from": "editorial:articles", "query": { "section": "{section}" } }`.
//!   `{felt}` erstattes med feltets værdi. Er feltet tomt, udelades parameteren;
//!   er det også påkrævet, hentes der intet, og editoren beder om at udfylde det.

use crate::{
    defs::{FieldDef, FieldType, RefTypeDef},
    error::{AppError, R},
    services::{is_pointer, OptionsCache, RETRY_AFTER},
    validate::is_shown,
    AppState,
};
use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use ts_rs::TS;

/// Hvor længe et hentet udsnit er frisk.
const TTL: Duration = Duration::from_secs(60);
/// Loft over antal udsnit i cachen.
const MAX_ENTRIES: usize = 200;
/// Højst så mange poster i et udsnit.
pub const MAX_ITEMS: usize = 5;

/* ---------- definitioner ---------- */

/// Et udsnit i services.json. Alle felter undtagen `path` er JSON-pointere.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct PreviewDef {
    /// Stien efter `baseUrl`, fx "/articles".
    pub path: String,
    /// Hvor listen står i svaret, fx "/data". Udeladt: svaret er selv listen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<String>,
    /// Overskriften i hver post, fx "/headline".
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Billedets adresse, fx "/image/url".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Postens adresse på sitet, fx "/url".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// `preview` på en datasource-type.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct PreviewRef {
    /// "service:preview" fra services.json.
    pub from: String,
    /// Query-parametre. `{felt}` erstattes med feltets værdi.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(type = "Record<string, string>", optional)]
    pub query: BTreeMap<String, String>,
}

/// Problemer i et udsnit i services.json.
pub fn check(at: &str, p: &PreviewDef) -> Vec<String> {
    let mut out = vec![];
    if !p.path.starts_with('/') {
        out.push(format!("{at}: «path» skal starte med /"));
    }
    if p.path.contains('{') || p.path.contains('?') {
        out.push(format!("{at}: «path» må ikke have skabeloner eller query. Læg dem i «preview.query» på datasource-typen"));
    }
    let ptrs = [("items", p.items.as_deref()), ("title", Some(p.title.as_str())), ("subtitle", p.subtitle.as_deref()), ("image", p.image.as_deref()), ("url", p.url.as_deref())];
    for (prop, v) in ptrs {
        if let Some(v) = v {
            if !is_pointer(v) {
                out.push(format!("{at}: «{prop}» skal være en JSON-pointer som \"/navn\", ikke \"{v}\""));
            }
        }
    }
    out
}

/// Navnene i `{…}` i en skabelon. `Err` ved en `{` uden `}`.
pub fn placeholders(s: &str) -> Result<Vec<&str>, String> {
    let mut out = vec![];
    let mut rest = s;
    while let Some(i) = rest.find('{') {
        let after = &rest[i + 1..];
        let j = after.find('}').ok_or_else(|| format!("«{s}» har en {{ uden }}"))?;
        out.push(&after[..j]);
        rest = &after[j + 1..];
    }
    Ok(out)
}

/// Felttyper, der kan stå i en skabelon: enkle værdier og lister af enkle værdier.
fn templatable(t: &FieldType) -> bool {
    !matches!(t, FieldType::Group | FieldType::List | FieldType::Datasource | FieldType::Richtext)
}

/// Problemer i `preview` på en datasource-type. `previews` er alle "service:preview".
pub fn check_ref(d: &RefTypeDef, previews: &[String]) -> Vec<String> {
    let mut out = vec![];
    if d.preview.is_some() && d.preview_note.is_some() {
        out.push("brug enten «preview» eller «previewNote», ikke begge".to_string());
    }
    let Some(p) = &d.preview else { return out };
    if !previews.contains(&p.from) {
        out.push(format!("«preview.from» peger på «{}», som ikke står under «previews» i services.json", p.from));
    }
    for (k, tmpl) in &p.query {
        match placeholders(tmpl) {
            Err(e) => out.push(format!("«preview.query.{k}»: {e}")),
            Ok(names) => {
                for n in names {
                    match d.fields.iter().find(|f| f.key == n) {
                        None => out.push(format!("«preview.query.{k}» bruger «{{{n}}}», men typen har ikke feltet «{n}»")),
                        Some(f) if !templatable(&f.field_type) => {
                            out.push(format!("«preview.query.{k}» bruger «{{{n}}}», men felter af den type kan ikke stå i en skabelon"))
                        }
                        Some(_) => {}
                    }
                }
            }
        }
    }
    out
}

/* ---------- skabeloner ---------- */

/// En værdi som tekst til en query-streng. Lister bliver kommasepareret.
fn as_text(v: &Value) -> Option<String> {
    let one = |v: &Value| match v {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    };
    match v {
        Value::Array(a) => Some(a.iter().filter_map(one).collect::<Vec<_>>().join(",")).filter(|s| !s.is_empty()),
        v => one(v),
    }
}

/// Fylder skabelonerne ud. `Err` med feltets navn, hvis et påkrævet felt er tomt.
fn fill(p: &PreviewRef, fields_def: &[FieldDef], values: &Map<String, Value>) -> Result<Vec<(String, String)>, String> {
    let mut out = vec![];
    'params: for (k, tmpl) in &p.query {
        let mut s = tmpl.clone();
        for n in placeholders(tmpl).unwrap_or_default() {
            let Some(fd) = fields_def.iter().find(|f| f.key == n) else { continue 'params };
            let v = is_shown(fd, fields_def, values)
                .then(|| values.get(n).filter(|v| !v.is_null()).or(fd.default.as_ref()))
                .flatten()
                .and_then(as_text);
            match v {
                Some(v) => s = s.replace(&format!("{{{n}}}"), &v),
                None if fd.required && is_shown(fd, fields_def, values) => return Err(fd.label.clone().unwrap_or_else(|| fd.key.clone())),
                None => continue 'params,
            }
        }
        out.push((k.clone(), s));
    }
    Ok(out)
}

/// Læser posterne ud af et svar.
fn parse(def: &PreviewDef, body: &Value) -> Result<Vec<PreviewItem>, String> {
    let items = body
        .pointer(def.items.as_deref().unwrap_or(""))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("svaret har ingen liste på «{}»", def.items.as_deref().unwrap_or("")))?;
    let text = |it: &Value, p: Option<&str>| match p.and_then(|p| it.pointer(p)) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    };
    Ok(items
        .iter()
        .filter_map(|it| {
            Some(PreviewItem {
                title: text(it, Some(&def.title))?,
                subtitle: text(it, def.subtitle.as_deref()),
                image: text(it, def.image.as_deref()),
                url: text(it, def.url.as_deref()),
            })
        })
        .take(MAX_ITEMS)
        .collect())
}

/* ---------- cache og endpoint ---------- */

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, optional_fields)]
pub struct PreviewItem {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum PreviewStatus {
    /// Hentet for nylig.
    Fresh,
    /// Servicen svarer ikke. Udsnittet er det seneste, der virkede.
    Stale,
    /// Servicen svarer ikke eller er ikke sat op, og der er intet gammelt udsnit.
    Unavailable,
    /// Et påkrævet felt er tomt.
    Incomplete,
    /// Typen har intet udsnit (se `message`).
    None,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Preview {
    pub status: PreviewStatus,
    pub items: Vec<PreviewItem>,
    pub fetched_at: Option<DateTime<Utc>>,
    /// Forklaring til redaktøren, når udsnittet ikke er friskt.
    pub message: Option<String>,
}

impl Preview {
    fn empty(status: PreviewStatus, message: impl Into<String>) -> Self {
        Preview { status, items: vec![], fetched_at: None, message: Some(message.into()) }
    }
}

#[derive(Default)]
pub struct Entry {
    items: Option<Vec<PreviewItem>>,
    fetched_at: Option<DateTime<Utc>>,
    fetched: Option<Instant>,
    error: Option<String>,
    failed: Option<Instant>,
}

impl Entry {
    fn live(&self) -> bool {
        self.fetched.is_some_and(|t| t.elapsed() < TTL) || self.failed.is_some_and(|t| t.elapsed() < RETRY_AFTER)
    }
    fn snapshot(&self, status: PreviewStatus) -> Preview {
        match &self.items {
            Some(items) => Preview { status, items: items.clone(), fetched_at: self.fetched_at, message: self.error.clone() },
            None => Preview::empty(PreviewStatus::Unavailable, self.error.clone().unwrap_or_default()),
        }
    }
}

impl OptionsCache {
    /// Udsnittet for en datasource-type med de givne felter.
    pub async fn preview(&self, defs: &crate::defs::Defs, d: &RefTypeDef, values: &Map<String, Value>) -> Preview {
        let Some(p) = &d.preview else {
            return Preview::empty(PreviewStatus::None, d.preview_note.clone().unwrap_or_else(|| "Der er intet udsnit for denne type".into()));
        };
        let Some((svc_name, def)) = p.from.split_once(':').and_then(|(s, k)| Some((s, defs.services.get(s)?.previews.get(k)?))) else {
            return Preview::empty(PreviewStatus::Unavailable, format!("«{}» står ikke i services.json", p.from));
        };
        let svc = &defs.services[svc_name];
        let query = match fill(p, &d.fields, values) {
            Ok(q) => q,
            Err(label) => return Preview::empty(PreviewStatus::Incomplete, format!("Udfyld «{label}» for at se et udsnit")),
        };
        let key = format!("{}?{}", p.from, serde_json::to_string(&query).unwrap_or_default());

        {
            let entries = self.previews.lock().unwrap();
            if let Some(e) = entries.get(&key) {
                if e.fetched.is_some_and(|t| t.elapsed() < TTL) {
                    return e.snapshot(PreviewStatus::Fresh);
                }
                if e.failed.is_some_and(|t| t.elapsed() < RETRY_AFTER) {
                    return e.snapshot(PreviewStatus::Stale);
                }
            }
        }

        let result = match self.get_json(svc_name, svc, &def.path, &query).await {
            Ok(body) => parse(def, &body),
            Err(e) => Err(e),
        };
        let mut entries = self.previews.lock().unwrap();
        if entries.len() >= MAX_ENTRIES && !entries.contains_key(&key) {
            entries.retain(|_, e| e.live());
            if entries.len() >= MAX_ENTRIES {
                entries.clear();
            }
        }
        let e = entries.entry(key).or_default();
        match result {
            Ok(items) => {
                *e = Entry { items: Some(items), fetched_at: Some(Utc::now()), fetched: Some(Instant::now()), error: None, failed: None };
                e.snapshot(PreviewStatus::Fresh)
            }
            Err(err) => {
                tracing::warn!(preview = p.from, "kunne ikke hente udsnit: {err}");
                e.error = Some(err);
                e.failed = Some(Instant::now());
                e.snapshot(PreviewStatus::Stale)
            }
        }
    }
}

#[derive(Deserialize)]
pub struct PreviewReq {
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    fields: Map<String, Value>,
}

/// `POST /api/datasources/preview` med `{ "type": "...", "fields": { ... } }`.
pub async fn post_preview(State(s): State<AppState>, Json(req): Json<PreviewReq>) -> R<Json<Preview>> {
    let defs = crate::defs::load(&s.defs_dir);
    let d = defs.find_type("datasource", &req.ty).ok_or_else(|| AppError::not_found(&format!("Datasource-typen «{}»", req.ty)))?;
    Ok(Json(s.options.preview(&defs, d, &req.fields).await))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ds(v: Value) -> RefTypeDef {
        serde_json::from_value(v).unwrap()
    }

    fn articles() -> RefTypeDef {
        ds(json!({
            "type": "q", "label": "Q", "source": "ed",
            "preview": { "from": "ed:articles", "query": { "section": "{section}", "tags": "{tags}", "ids": "id:{ids}", "limit": "5" } },
            "fields": [
                { "key": "section", "type": "select", "label": "Sektion", "options": ["a"], "default": "a" },
                { "key": "tags", "type": "tags", "label": "Tags" },
                { "key": "ids", "type": "text", "label": "Id'er", "required": true }
            ]
        }))
    }

    #[test]
    fn placeholders_and_checks() {
        assert_eq!(placeholders("{a}-{b}x").unwrap(), vec!["a", "b"]);
        assert!(placeholders("{a").is_err());
        let d = ds(json!({
            "type": "q", "label": "Q", "source": "ed", "previewNote": "x",
            "preview": { "from": "ed:nope", "query": { "a": "{missing}", "b": "{list}", "c": "{oops" } },
            "fields": [{ "key": "list", "type": "list", "label": "L", "fields": [] }]
        }));
        let p = check_ref(&d, &["ed:articles".into()]).join("\n");
        assert!(p.contains("enten «preview» eller «previewNote»"), "{p}");
        assert!(p.contains("«ed:nope»"), "{p}");
        assert!(p.contains("ikke feltet «missing»"), "{p}");
        assert!(p.contains("kan ikke stå i en skabelon"), "{p}");
        assert!(p.contains("{ uden }"), "{p}");
        assert!(check_ref(&articles(), &["ed:articles".into()]).is_empty());
        let svc: PreviewDef = serde_json::from_value(json!({ "path": "/a/{x}", "title": "headline" })).unwrap();
        let p = check("«ed:a»", &svc).join("\n");
        assert!(p.contains("skabeloner") && p.contains("JSON-pointer"), "{p}");
    }

    #[test]
    fn fills_templates() {
        let d = articles();
        let p = d.preview.as_ref().unwrap();
        let v = |j: Value| j.as_object().unwrap().clone();
        // Påkrævet felt mangler.
        assert_eq!(fill(p, &d.fields, &v(json!({}))).unwrap_err(), "Id'er");
        // Standardværdi bruges, tomme felter udelades, lister bliver kommasepareret.
        let q = fill(p, &d.fields, &v(json!({ "ids": " 12 ", "tags": [] }))).unwrap();
        assert_eq!(q, vec![("ids".into(), "id:12".into()), ("limit".into(), "5".into()), ("section".into(), "a".into())]);
        let q = fill(p, &d.fields, &v(json!({ "ids": "1", "tags": ["ø", "b&c"], "section": "x y" }))).unwrap();
        assert!(q.contains(&("tags".into(), "ø,b&c".into())));
        assert!(q.contains(&("section".into(), "x y".into())));
    }

    #[test]
    fn parses_items() {
        let def: PreviewDef = serde_json::from_value(json!({ "path": "/a", "items": "/data", "title": "/h", "image": "/img/url", "url": "/u" })).unwrap();
        let body = json!({ "data": (0..8).map(|i| json!({ "h": format!("T{i}"), "img": { "url": "x.jpg" } })).chain([json!({ "u": "uden titel" })]).collect::<Vec<_>>() });
        let items = parse(&def, &body).unwrap();
        assert_eq!(items.len(), MAX_ITEMS);
        assert_eq!(items[0].image.as_deref(), Some("x.jpg"));
        assert!(items[0].url.is_none());
        assert!(parse(&def, &json!({})).is_err());
    }

    fn defs_with(base_url: &str) -> crate::defs::Defs {
        let mut d = crate::defs::load(std::path::Path::new("../definitions"));
        let svc: crate::services::ServiceDef = serde_json::from_value(json!({
            "baseUrl": base_url,
            "previews": { "articles": { "path": "/articles", "items": "/data", "title": "/headline" } }
        }))
        .unwrap();
        d.services = [("ed".to_string(), svc)].into();
        d
    }

    #[tokio::test]
    async fn statuses() {
        let cache = OptionsCache::with_env(|_| None);
        let none = ds(json!({ "type": "u", "label": "U", "source": "ed", "previewNote": "Afhænger af den besøgende", "fields": [] }));
        let defs = defs_with("${ED_URL}");
        let p = cache.preview(&defs, &none, &Map::new()).await;
        assert_eq!((p.status, p.message.as_deref()), (PreviewStatus::None, Some("Afhænger af den besøgende")));
        let p = cache.preview(&defs, &articles(), &Map::new()).await;
        assert_eq!(p.status, PreviewStatus::Incomplete);
        let vals = json!({ "ids": "1" }).as_object().unwrap().clone();
        let p = cache.preview(&defs, &articles(), &vals).await;
        assert_eq!(p.status, PreviewStatus::Unavailable);
        assert!(p.message.unwrap().contains("ED_URL mangler"));
    }

    /// Henter fra en rigtig server med query-parametre, og bruger det gamle udsnit, når den er væk.
    #[tokio::test]
    async fn fresh_then_stale_then_unavailable() {
        use axum::{extract::Query, http::StatusCode, response::IntoResponse, routing::get, Router};
        use std::collections::HashMap;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let down = Arc::new(AtomicBool::new(false));
        let d = down.clone();
        let app = Router::new().route(
            "/articles",
            get(move |Query(q): Query<HashMap<String, String>>| async move {
                if d.load(Ordering::SeqCst) {
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }
                let title = format!("{}|{}", q.get("ids").cloned().unwrap_or_default(), q.get("tags").cloned().unwrap_or_default());
                Json(json!({ "data": [{ "headline": title }] })).into_response()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let cache = OptionsCache::with_env(move |k| (k == "ED_URL").then(|| format!("http://{addr}")));
        let defs = defs_with("${ED_URL}");
        let vals = |j: Value| j.as_object().unwrap().clone();
        let p = cache.preview(&defs, &articles(), &vals(json!({ "ids": "7", "tags": ["a&b", "ø"] }))).await;
        assert_eq!(p.status, PreviewStatus::Fresh, "{:?}", p.message);
        assert_eq!(p.items[0].title, "id:7|a&b,ø", "værdierne skal URL-kodes og komme rigtigt frem");

        down.store(true, Ordering::SeqCst);
        cache.previews.lock().unwrap().values_mut().for_each(|e| e.fetched = Some(Instant::now() - TTL));
        let p = cache.preview(&defs, &articles(), &vals(json!({ "ids": "7", "tags": ["a&b", "ø"] }))).await;
        assert_eq!(p.status, PreviewStatus::Stale);
        assert_eq!(p.items.len(), 1);
        assert!(p.message.unwrap().contains("503"));

        // Andre felter giver et andet udsnit, som aldrig er hentet.
        let p = cache.preview(&defs, &articles(), &vals(json!({ "ids": "8" }))).await;
        assert_eq!(p.status, PreviewStatus::Unavailable);
        assert!(p.items.is_empty());
    }
}
