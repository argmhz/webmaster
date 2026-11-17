//! Validering af kladder mod definitionerne.
//!
//! Fejl (`error`) forhindrer publicering. Advarsler (`warning`) vises i editoren,
//! men blokerer ikke. Kladder kan altid gemmes, også med fejl.

use crate::{
    defs::{self, Defs, FieldDef, FieldType},
    error::{AppError, R},
    refs, AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{NaiveDateTime, NaiveTime};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Level {
    Error,
    Warning,
}

/// Et problem i en kladde. `section`, `element` og `field` peger på stedet,
/// så editoren kan vælge det.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, optional_fields)]
pub struct Issue {
    pub level: Level,
    /// Hvor i dokumentet, i klar tekst, fx "Sektion 2 › Artikelliste".
    pub location: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

/// Det, valideringen skal vide om et dokument, der refereres til.
#[derive(Debug, Clone)]
pub struct RefInfo {
    pub kind: String,
    pub doc_type: Option<String>,
    pub name: String,
    pub published: bool,
}

/* ---------- enkelte værdier ---------- */

fn is_empty(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.trim().is_empty(),
        Some(Value::Array(a)) => a.is_empty(),
        _ => false,
    }
}

fn in_options(fd: &FieldDef, s: &str) -> bool {
    // Med optionsFrom er `options` kun en reserve, så andre værdier er tilladt.
    fd.options_from.is_some() || fd.options.as_ref().is_some_and(|o| o.iter().any(|x| x == s))
}

fn is_link(s: &str) -> bool {
    ["/", "#", "https://", "http://", "mailto:", "tel:"].iter().any(|p| s.starts_with(p))
}

/// Tjekker en udfyldt værdi mod feltets type. Bruges også til `default` i definitionerne.
/// Datasource-felter tjekkes for sig, fordi de kræver opslag.
pub fn check_value(fd: &FieldDef, v: &Value) -> Option<String> {
    use FieldType::*;
    let want = |what: &str| Some(format!("skal være {what}"));
    match fd.field_type {
        Text | Richtext | Image => (!v.is_string()).then(|| "skal være tekst".into()),
        Link => match v.as_str() {
            None => want("tekst"),
            Some(s) if !s.is_empty() && !is_link(s) => {
                Some(format!("«{s}» er ikke et link. Brug /sti, #anker eller https://…"))
            }
            _ => None,
        },
        Number => match v.as_f64() {
            None => want("et tal"),
            Some(n) if fd.max.is_some_and(|m| n > m) => Some(format!("må højst være {}", fd.max.unwrap())),
            _ => None,
        },
        Boolean => (!v.is_boolean()).then(|| "skal være sand eller falsk".into()),
        Select => match v.as_str() {
            None => want("en tekst"),
            Some(s) if !in_options(fd, s) => Some(format!("«{s}» er ikke en af valgmulighederne")),
            _ => None,
        },
        Multiselect => {
            let Some(a) = v.as_array() else { return want("en liste") };
            let mut seen = vec![];
            for x in a {
                let Some(s) = x.as_str() else { return want("en liste af tekster") };
                if !in_options(fd, s) {
                    return Some(format!("«{s}» er ikke en af valgmulighederne"));
                }
                if seen.contains(&s) {
                    return Some(format!("«{s}» er valgt flere gange"));
                }
                seen.push(s);
            }
            None
        }
        Datetime => match v.as_str() {
            Some(s)
                if NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").is_ok()
                    || NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok() =>
            {
                None
            }
            _ => Some("skal være dato og tid som ÅÅÅÅ-MM-DDTTT:MM".into()),
        },
        Time => match v.as_str() {
            Some(s) if NaiveTime::parse_from_str(s, "%H:%M").is_ok() => None,
            _ => Some("skal være et klokkeslæt som TT:MM".into()),
        },
        Datasource => None,
    }
}

/* ---------- dokumenter ---------- */

#[derive(Clone, Default)]
struct Loc {
    text: String,
    section: Option<String>,
    element: Option<String>,
}

impl Loc {
    fn child(&self, text: impl AsRef<str>) -> Loc {
        let text = if self.text.is_empty() { text.as_ref().to_string() } else { format!("{} › {}", self.text, text.as_ref()) };
        Loc { text, ..self.clone() }
    }
}

struct V<'a> {
    defs: &'a Defs,
    refs: &'a HashMap<String, RefInfo>,
    out: Vec<Issue>,
}

impl V<'_> {
    fn push(&mut self, level: Level, loc: &Loc, field: Option<&str>, message: impl Into<String>) {
        self.out.push(Issue {
            level,
            location: if loc.text.is_empty() { "Dokumentet".into() } else { loc.text.clone() },
            message: message.into(),
            section: loc.section.clone(),
            element: loc.element.clone(),
            field: field.map(String::from),
        });
    }
    fn err(&mut self, loc: &Loc, field: Option<&str>, m: impl Into<String>) {
        self.push(Level::Error, loc, field, m)
    }
    fn warn(&mut self, loc: &Loc, field: Option<&str>, m: impl Into<String>) {
        self.push(Level::Warning, loc, field, m)
    }

    /// Slår et refereret dokument op og tjekker, at det har den rigtige slags og er publiceret.
    fn lookup(&mut self, loc: &Loc, field: Option<&str>, id: &str, kind: &str, what: &str) -> Option<RefInfo> {
        let Some(r) = self.refs.get(id) else {
            self.err(loc, field, format!("{what} «{id}» findes ikke"));
            return None;
        };
        if r.kind != kind {
            self.err(loc, field, format!("«{}» er en {}, ikke en {what}", r.name, r.kind));
            return None;
        }
        if !r.published {
            self.err(loc, field, format!("{what} «{}» er ikke publiceret", r.name));
        }
        Some(r.clone())
    }

    fn fields(&mut self, loc: &Loc, defs: &[FieldDef], obj: Option<&Value>) {
        let empty = Map::new();
        let obj = match obj {
            None | Some(Value::Null) => &empty,
            Some(Value::Object(m)) => m,
            Some(_) => {
                self.err(loc, None, "felterne skal være et objekt");
                return;
            }
        };
        for fd in defs {
            let v = obj.get(&fd.key);
            let f = Some(fd.key.as_str());
            if is_empty(v) {
                if fd.required {
                    self.err(loc, f, format!("{} skal udfyldes", fd.label()));
                }
                continue;
            }
            let v = v.unwrap();
            if fd.field_type == FieldType::Datasource {
                self.datasource(loc, fd, v);
            } else if let Some(m) = check_value(fd, v) {
                self.err(loc, f, format!("{} {m}", fd.label()));
            }
        }
        for k in obj.keys() {
            if !defs.iter().any(|d| &d.key == k) {
                self.warn(loc, Some(k), format!("feltet «{k}» findes ikke i definitionen"));
            }
        }
    }

    fn datasource(&mut self, loc: &Loc, fd: &FieldDef, v: &Value) {
        let f = Some(fd.key.as_str());
        let accepts = fd.accepts.clone().unwrap_or_default();
        let accepted = |ty: &str| accepts.iter().any(|a| a == ty);
        let label = |ty: &str| self.defs.find_type("datasource", ty).map(|d| d.label.clone()).unwrap_or_else(|| ty.into());
        if let Some(id) = v.get("ref").and_then(Value::as_str) {
            let Some(r) = self.lookup(loc, f, id, "datasource", "datasourcen") else { return };
            let ty = r.doc_type.unwrap_or_default();
            if !accepted(&ty) {
                let ok = accepts.iter().map(|a| label(a)).collect::<Vec<_>>().join(", ");
                self.err(loc, f, format!("{}: «{}» er en {}, men feltet tager kun {ok}", fd.label(), r.name, label(&ty)));
            }
        } else if let Some(inline) = v.get("inline").filter(|x| x.is_object()) {
            let ty = inline.get("type").and_then(Value::as_str).unwrap_or("");
            let Some(def) = self.defs.find_type("datasource", ty) else {
                self.err(loc, f, format!("{}: datasource-typen «{ty}» findes ikke", fd.label()));
                return;
            };
            if !accepted(ty) {
                self.err(loc, f, format!("{}: feltet tager ikke {}", fd.label(), def.label));
            }
            let inner = loc.child(format!("{} (lokal {})", fd.label(), def.label.to_lowercase()));
            self.fields(&inner, &def.fields, inline.get("fields"));
        } else {
            self.err(loc, f, format!("{} skal pege på en datasource", fd.label()));
        }
    }

    /// `schedules` og `access` på side, sektion og element.
    fn visibility(&mut self, loc: &Loc, obj: &Value) {
        for (key, kind, what) in [("schedules", "schedule", "schedulen"), ("access", "access", "access-reglen")] {
            match obj.get(key) {
                None | Some(Value::Null) => {}
                Some(Value::Array(a)) => {
                    for x in a {
                        match x.as_str() {
                            Some(id) => {
                                self.lookup(loc, None, id, kind, what);
                            }
                            None => self.err(loc, None, format!("«{key}» må kun indeholde id'er")),
                        }
                    }
                }
                Some(_) => self.err(loc, None, format!("«{key}» skal være en liste")),
            }
        }
    }

    fn page(&mut self, draft: &Value) {
        let root = Loc { text: "Side".into(), ..Default::default() };
        self.fields(&root, &self.defs.page.fields.clone(), draft.get("fields"));
        self.visibility(&root, draft);
        let sections = match draft.get("sections") {
            None | Some(Value::Null) => return,
            Some(Value::Array(a)) => a,
            Some(_) => return self.err(&root, None, "«sections» skal være en liste"),
        };
        for (si, s) in sections.iter().enumerate() {
            let sid = s.get("id").and_then(Value::as_str).map(String::from);
            let sloc = Loc { text: format!("Sektion {}", si + 1), section: sid, element: None };
            let layout = s.get("layout").and_then(Value::as_str).unwrap_or("");
            let width = match self.defs.layout(layout) {
                Some(l) => l.columns.len(),
                None => {
                    self.err(&sloc, None, format!("layoutet «{layout}» findes ikke"));
                    usize::MAX
                }
            };
            self.fields(&sloc, &self.defs.section.fields.clone(), s.get("settings"));
            self.visibility(&sloc, s);
            let cols = s.get("columns").and_then(Value::as_array).cloned().unwrap_or_default();
            for (ci, col) in cols.iter().enumerate() {
                let els = col.as_array().cloned().unwrap_or_default();
                if ci >= width && !els.is_empty() {
                    self.err(
                        &sloc,
                        None,
                        format!("kolonne {} findes ikke i layoutet «{layout}», så {} element(er) bliver ikke vist", ci + 1, els.len()),
                    );
                }
                for e in &els {
                    self.element(&sloc, ci, e);
                }
            }
        }
    }

    fn element(&mut self, sloc: &Loc, ci: usize, e: &Value) {
        let ty = e.get("type").and_then(Value::as_str).unwrap_or("");
        let def = self.defs.element(ty).cloned();
        let label = def.as_ref().map(|d| d.label.clone()).unwrap_or_else(|| ty.into());
        let title = e.pointer("/fields/title").and_then(Value::as_str).filter(|t| !t.is_empty());
        let name = match title {
            Some(t) => format!("{label} «{t}»"),
            None => label,
        };
        let mut loc = sloc.child(format!("kolonne {} › {name}", ci + 1));
        loc.element = e.get("id").and_then(Value::as_str).map(String::from);
        let Some(def) = def else {
            return self.err(&loc, None, format!("elementtypen «{ty}» findes ikke længere i definitions/"));
        };
        self.fields(&loc, &def.fields, e.get("fields"));
        self.visibility(&loc, e);
    }
}

/// Validerer en kladde. `refs` skal indeholde alle dokumenter, kladden peger på.
pub fn document(defs: &Defs, kind: &str, doc_type: Option<&str>, draft: &Value, refs: &HashMap<String, RefInfo>) -> Vec<Issue> {
    let mut v = V { defs, refs, out: vec![] };
    if kind == "page" {
        v.page(draft);
    } else {
        let ty = doc_type.unwrap_or("");
        match defs.find_type(kind, ty) {
            Some(d) => v.fields(&Loc::default(), &d.fields, draft.get("fields")),
            None => v.err(&Loc::default(), None, format!("typen «{ty}» findes ikke længere i definitions/{}", defs::list_key(kind))),
        }
    }
    v.out
}

/* ---------- med databasen ---------- */

pub async fn load_refs(db: &sqlx::PgPool, draft: &Value) -> R<HashMap<String, RefInfo>> {
    let ids: Vec<String> = refs::collect(draft).into_iter().collect();
    let rows: Vec<(String, String, Option<String>, String, bool)> = sqlx::query_as(
        "select d.id, d.kind, d.type, d.name, p.document_id is not null
           from documents d left join publications p on p.document_id = d.id
          where d.id = any($1)",
    )
    .bind(&ids)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, kind, doc_type, name, published)| (id, RefInfo { kind, doc_type, name, published }))
        .collect())
}

/// Validerer den gemte kladde for et dokument.
pub async fn stored(s: &AppState, id: &str) -> R<Vec<Issue>> {
    let row: Option<(String, Option<String>, Value)> = sqlx::query_as("select kind, type, draft from documents where id = $1")
        .bind(id)
        .fetch_optional(&s.db)
        .await?;
    let Some((kind, ty, draft)) = row else { return Err(AppError::not_found("Dokumentet")) };
    let defs = defs::load(&s.defs_dir);
    let refs = load_refs(&s.db, &draft).await?;
    Ok(document(&defs, &kind, ty.as_deref(), &draft, &refs))
}

pub async fn get_issues(State(s): State<AppState>, Path(id): Path<String>) -> R<Json<Vec<Issue>>> {
    Ok(Json(stored(&s, &id).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn defs() -> Defs {
        let d = defs::load(std::path::Path::new("../definitions"));
        assert!(d.problems.is_empty(), "{:?}", d.problems);
        d
    }

    /// Alle dokumenter og sider i seed.json skal kunne publiceres med de nuværende definitioner.
    #[test]
    fn seed_is_valid() {
        let defs = defs();
        let seed: Value = serde_json::from_str(&std::fs::read_to_string("../seed.json").unwrap()).unwrap();
        let unpublished: Vec<&str> = seed["unpublished"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        let mut refs = HashMap::new();
        for d in seed["documents"].as_array().unwrap() {
            let id = d["id"].as_str().unwrap();
            refs.insert(
                id.to_string(),
                RefInfo {
                    kind: d["kind"].as_str().unwrap().into(),
                    doc_type: d["type"].as_str().map(String::from),
                    name: d["name"].as_str().unwrap().into(),
                    published: !unpublished.contains(&id),
                },
            );
        }
        let mut all = vec![];
        for d in seed["documents"].as_array().unwrap() {
            for i in document(&defs, d["kind"].as_str().unwrap(), d["type"].as_str(), &d["draft"], &refs) {
                all.push(format!("{}: {} – {}", d["id"], i.location, i.message));
            }
        }
        for p in seed["pages"].as_array().unwrap() {
            for i in document(&defs, "page", None, &p["draft"], &refs) {
                all.push(format!("{}: {} – {}", p["id"], i.location, i.message));
            }
        }
        assert!(all.is_empty(), "seed.json passer ikke til definitions/:\n{}", all.join("\n"));
    }

    #[test]
    fn finds_errors_in_page() {
        let defs = defs();
        let refs: HashMap<String, RefInfo> = [
            ("ds_a".to_string(), RefInfo { kind: "datasource".into(), doc_type: Some("userProfile".into()), name: "Bruger".into(), published: true }),
            ("sch_x".to_string(), RefInfo { kind: "schedule".into(), doc_type: Some("window".into()), name: "X".into(), published: false }),
        ]
        .into();
        let draft = json!({
            "fields": { "title": "" , "gammel": 1 },
            "sections": [{
                "id": "s1", "layout": "1", "settings": { "spacing": "huge" }, "schedules": ["sch_x"], "access": ["nope"],
                "columns": [
                    [
                        { "id": "e1", "type": "articleList", "fields": { "title": "Hej", "articles": { "ref": "ds_a" } } },
                        { "id": "e2", "type": "articleList", "fields": { "title": "Lokal", "articles": { "inline": { "type": "mostRead", "fields": { "limit": 99 } } } } },
                        { "id": "e3", "type": "hero", "fields": { "title": "x", "link": "www.dr.dk" } },
                        { "id": "e4", "type": "findesIkke", "fields": {} }
                    ],
                    [ { "id": "e5", "type": "text", "fields": { "body": "x" } } ]
                ]
            }]
        });
        let issues = document(&defs, "page", None, &draft, &refs);
        let msgs: Vec<String> = issues.iter().map(|i| format!("{:?} {} – {}", i.level, i.location, i.message)).collect();
        let all = msgs.join("\n");
        let has = |s: &str| assert!(all.contains(s), "mangler «{s}» i:\n{all}");
        has("Sidetitel skal udfyldes");
        has("Warning Side – feltet «gammel» findes ikke");
        has("«huge» er ikke en af valgmulighederne");
        has("schedulen «X» er ikke publiceret");
        has("access-reglen «nope» findes ikke");
        has("men feltet tager kun");
        has("lokal mest læste");
        has("Antal må højst være 20");
        has("«www.dr.dk» er ikke et link");
        has("elementtypen «findesIkke»");
        has("kolonne 2 findes ikke");
        let e1 = issues.iter().find(|i| i.message.contains("men feltet tager kun")).unwrap();
        assert_eq!(e1.element.as_deref(), Some("e1"));
        assert_eq!(e1.section.as_deref(), Some("s1"));
        assert_eq!(e1.field.as_deref(), Some("articles"));
    }

    #[test]
    fn values() {
        let fd = |t: &str, extra: Value| -> FieldDef {
            let mut v = json!({ "key": "k", "type": t });
            v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            serde_json::from_value(v).unwrap()
        };
        assert!(check_value(&fd("time", json!({})), &json!("06:00")).is_none());
        assert!(check_value(&fd("time", json!({})), &json!("6")).is_some());
        assert!(check_value(&fd("datetime", json!({})), &json!("2026-11-18T18:00")).is_none());
        assert!(check_value(&fd("datetime", json!({})), &json!("i morgen")).is_some());
        assert!(check_value(&fd("select", json!({ "options": ["a"], "optionsFrom": "x:y" })), &json!("b")).is_none());
        assert!(check_value(&fd("multiselect", json!({ "options": ["a", "b"] })), &json!(["a", "a"])).is_some());
        assert!(check_value(&fd("boolean", json!({})), &json!("true")).is_some());
    }
}
