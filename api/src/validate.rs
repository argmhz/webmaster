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
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
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

/// Tom værdi for netop dette felt. Formateret tekst uden tekst tæller som tom.
fn is_empty_for(fd: &FieldDef, v: Option<&Value>) -> bool {
    match (fd.field_type, v) {
        (FieldType::Richtext, Some(v @ Value::Object(_))) => crate::richtext::is_empty(v),
        _ => is_empty(v),
    }
}

fn is_empty(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.trim().is_empty(),
        Some(Value::Array(a)) => a.is_empty(),
        Some(Value::Object(m)) => m.is_empty(),
        _ => false,
    }
}

pub fn is_link(s: &str) -> bool {
    ["/", "#", "https://", "http://", "mailto:", "tel:"].iter().any(|p| s.starts_with(p))
}

pub fn is_color(s: &str) -> bool {
    let h = s.strip_prefix('#').unwrap_or("");
    matches!(h.len(), 3 | 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit())
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { n.to_string() }
}

/// Tjekker en udfyldt værdi mod feltets type. Bruges også til `default` i definitionerne.
/// Datasource, group og list tjekkes for sig, fordi de kræver opslag eller indeholder andre felter.
pub fn check_value(fd: &FieldDef, v: &Value) -> Option<String> {
    use FieldType::*;
    let want = |what: &str| Some(format!("skal være {what}"));
    match fd.field_type {
        Richtext => {
            if let Some(m) = crate::richtext::check(v) {
                return Some(m);
            }
            let n = crate::richtext::text_len(v) as u32;
            if let Some(m) = fd.min_length.filter(|m| n < *m) {
                return Some(format!("skal være mindst {m} tegn"));
            }
            if let Some(m) = fd.max_length.filter(|m| n > *m) {
                return Some(format!("må højst være {m} tegn (er {n})"));
            }
            None
        }
        Tags => {
            let Some(a) = v.as_array() else { return want("en liste af tekster") };
            let mut seen = vec![];
            for x in a {
                let Some(s) = x.as_str() else { return want("en liste af tekster") };
                if s.trim().is_empty() {
                    return Some("må ikke indeholde tomme punkter".into());
                }
                if seen.contains(&s) {
                    return Some(format!("«{s}» står der flere gange"));
                }
                if let Some(p) = &fd.pattern {
                    let ok = regex::Regex::new(&format!("^(?:{p})$")).map(|r| r.is_match(s)).unwrap_or(true);
                    if !ok {
                        return Some(match &fd.help {
                            Some(h) => format!("«{s}» har ikke det rigtige format. {h}"),
                            None => format!("«{s}» har ikke det rigtige format ({p})"),
                        });
                    }
                }
                seen.push(s);
            }
            check_count(fd, a.len())
        }
        Text | Textarea => {
            let Some(s) = v.as_str() else { return want("tekst") };
            let n = s.chars().count() as u32;
            if let Some(m) = fd.min_length.filter(|m| n < *m) {
                return Some(format!("skal være mindst {m} tegn"));
            }
            if let Some(m) = fd.max_length.filter(|m| n > *m) {
                return Some(format!("må højst være {m} tegn (er {n})"));
            }
            if let Some(p) = &fd.pattern {
                let ok = regex::Regex::new(&format!("^(?:{p})$")).map(|r| r.is_match(s)).unwrap_or(true);
                if !ok {
                    return Some(match &fd.help {
                        Some(h) => format!("har ikke det rigtige format. {h}"),
                        None => format!("har ikke det rigtige format ({p})"),
                    });
                }
            }
            None
        }
        Image => (!v.is_string()).then(|| "skal være tekst".into()),
        Link => match v {
            Value::String(s) if !s.is_empty() && !is_link(s) => {
                Some(format!("«{s}» er ikke et link. Brug /sti, #anker eller https://…"))
            }
            Value::String(_) => None,
            Value::Object(m) if m.len() == 1 && m.get("page").is_some_and(Value::is_string) => None,
            _ => want("et link eller en side"),
        },
        Number => {
            let Some(n) = v.as_f64() else { return want("et tal") };
            if let Some(m) = fd.min.filter(|m| n < *m) {
                return Some(format!("skal være mindst {}", fmt_num(m)));
            }
            if let Some(m) = fd.max.filter(|m| n > *m) {
                return Some(format!("må højst være {}", fmt_num(m)));
            }
            if let Some(step) = fd.step {
                let k = (n - fd.min.unwrap_or(0.0)) / step;
                if (k - k.round()).abs() > 1e-9 {
                    return Some(format!("skal gå op i {}", fmt_num(step)));
                }
            }
            None
        }
        Boolean => (!v.is_boolean()).then(|| "skal være sand eller falsk".into()),
        Select => match v.as_str() {
            None => want("en tekst"),
            Some(s) if !fd.allows_option(s) => Some(format!("«{s}» er ikke en af valgmulighederne")),
            _ => None,
        },
        Multiselect => {
            let Some(a) = v.as_array() else { return want("en liste") };
            let mut seen = vec![];
            for x in a {
                let Some(s) = x.as_str() else { return want("en liste af tekster") };
                if !fd.allows_option(s) {
                    return Some(format!("«{s}» er ikke en af valgmulighederne"));
                }
                if seen.contains(&s) {
                    return Some(format!("«{s}» er valgt flere gange"));
                }
                seen.push(s);
            }
            check_count(fd, a.len())
        }
        Date => match v.as_str() {
            Some(s) if NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() => None,
            _ => Some("skal være en dato som ÅÅÅÅ-MM-DD".into()),
        },
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
        Color => match v.as_str() {
            Some(s) if fd.options.as_ref().is_some_and(|o| !o.iter().any(|x| x.value().eq_ignore_ascii_case(s))) => {
                Some(format!("«{s}» er ikke en af farverne i paletten"))
            }
            Some(s) if is_color(s) => None,
            _ => Some("skal være en farve som #rrggbb".into()),
        },
        Group => (!v.is_object()).then(|| "skal være et objekt".into()),
        List => match v.as_array() {
            None => want("en liste"),
            Some(a) => check_count(fd, a.len()),
        },
        Datasource => None,
    }
}

fn check_count(fd: &FieldDef, n: usize) -> Option<String> {
    let n = n as u32;
    if let Some(m) = fd.min_items.filter(|m| n < *m) {
        return Some(format!("skal have mindst {m}"));
    }
    if let Some(m) = fd.max_items.filter(|m| n > *m) {
        return Some(format!("må højst have {m} (har {n})"));
    }
    None
}

/// Om et felt vises ud fra `showWhen` og værdierne ved siden af det.
pub fn is_shown(fd: &FieldDef, siblings: &[FieldDef], obj: &Map<String, Value>) -> bool {
    let Some(c) = &fd.show_when else { return true };
    let v = obj.get(&c.field).cloned().or_else(|| siblings.iter().find(|f| f.key == c.field).and_then(|f| f.default.clone()));
    if let Some(eq) = &c.equals {
        return v.as_ref() == Some(eq);
    }
    if let Some(list) = &c.one_of {
        return v.is_some_and(|v| list.contains(&v));
    }
    if let Some(ne) = c.not_empty {
        return ne != is_empty(v.as_ref());
    }
    true
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
    /// Hentede lister for `optionsFrom` ("service:liste" → værdier). Mangler en liste,
    /// kunne den ikke hentes, og så bruges `options` som reserve.
    lists: &'a HashMap<String, Vec<String>>,
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

    /// Validerer et objekt med felter. `prefix` er stien til objektet, fx "links.0.",
    /// så `Issue.field` bliver stien til det enkelte felt.
    fn fields(&mut self, loc: &Loc, prefix: &str, defs: &[FieldDef], obj: Option<&Value>) {
        let empty = Map::new();
        let obj = match obj {
            None | Some(Value::Null) => &empty,
            Some(Value::Object(m)) => m,
            Some(_) => {
                let f = prefix.strip_suffix('.').filter(|p| !p.is_empty());
                self.err(loc, f, "felterne skal være et objekt");
                return;
            }
        };
        for fd in defs {
            let path = format!("{prefix}{}", fd.key);
            let f = Some(path.as_str());
            if !is_shown(fd, defs, obj) {
                continue;
            }
            let v = obj.get(&fd.key);
            if is_empty_for(fd, v) {
                if fd.required {
                    self.err(loc, f, format!("{} skal udfyldes", fd.label()));
                }
                continue;
            }
            let v = v.unwrap();
            if let Some(m) = check_value(fd, v) {
                self.err(loc, f, format!("{} {m}", fd.label()));
                continue;
            }
            if let Some(of) = &fd.options_from {
                self.options_from(loc, &path, fd, of, v);
            }
            match fd.field_type {
                FieldType::Datasource => self.datasource(loc, &path, fd, v),
                FieldType::Link => {
                    if let Some(id) = v.get("page").and_then(Value::as_str) {
                        self.lookup(loc, f, id, "page", "siden");
                    }
                }
                FieldType::Group => {
                    self.fields(&loc.child(fd.label()), &format!("{path}."), fd.sub_fields(), Some(v));
                }
                FieldType::List => {
                    for (i, item) in v.as_array().into_iter().flatten().enumerate() {
                        let title = fd
                            .item_label
                            .as_ref()
                            .and_then(|k| item.get(k))
                            .and_then(Value::as_str)
                            .filter(|t| !t.is_empty())
                            .map(|t| format!(" «{t}»"))
                            .unwrap_or_default();
                        let iloc = loc.child(format!("{} nr. {}{title}", fd.label(), i + 1));
                        self.fields(&iloc, &format!("{path}.{i}."), fd.sub_fields(), Some(item));
                    }
                }
                _ => {}
            }
        }
        for k in obj.keys() {
            if !defs.iter().any(|d| &d.key == k) {
                let path = format!("{prefix}{k}");
                self.warn(loc, Some(&path), format!("feltet «{k}» findes ikke i definitionen"));
            }
        }
    }

    /// Tjekker værdier mod listen fra servicen. Kan den ikke hentes, tjekkes mod `options`,
    /// og en ukendt værdi er kun en advarsel, så en service, der er nede, ikke blokerer.
    fn options_from(&mut self, loc: &Loc, path: &str, fd: &FieldDef, of: &str, v: &Value) {
        let values: Vec<&str> = match v {
            Value::String(s) => vec![s.as_str()],
            Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
            _ => return,
        };
        let svc = of.split(':').next().unwrap_or(of);
        for x in values {
            match self.lists.get(of) {
                Some(list) if !list.iter().any(|y| y == x) => {
                    self.err(loc, Some(path), format!("{}: «{x}» findes ikke i {of}", fd.label()));
                }
                Some(_) => {}
                None if fd.options.as_ref().is_some_and(|o| o.iter().any(|o| o.value() == x)) => {}
                None => self.warn(
                    loc,
                    Some(path),
                    format!("{}: «{x}» kunne ikke tjekkes, fordi {svc} ikke svarer, og den står ikke i reservelisten", fd.label()),
                ),
            }
        }
    }

    fn datasource(&mut self, loc: &Loc, path: &str, fd: &FieldDef, v: &Value) {
        let f = Some(path);
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
            let fields = def.fields.clone();
            self.fields(&inner, &format!("{path}.inline.fields."), &fields, inline.get("fields"));
        } else {
            self.err(loc, f, format!("{} skal pege på en datasource", fd.label()));
        }
    }

    /// `schedules`, `access` og `accessMatch` på side, sektion og element.
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
        match obj.get("accessMatch") {
            None | Some(Value::Null) => {}
            Some(Value::String(m)) if m == "all" || m == "any" => {}
            Some(_) => self.err(loc, None, "«accessMatch» skal være \"all\" eller \"any\""),
        }
    }

    fn page(&mut self, draft: &Value) {
        let root = Loc { text: "Side".into(), ..Default::default() };
        self.fields(&root, "", &self.defs.page.fields.clone(), draft.get("fields"));
        self.visibility(&root, draft);
        let sections = match draft.get("sections") {
            None | Some(Value::Null) => return,
            Some(Value::Array(a)) => a,
            Some(_) => return self.err(&root, None, "«sections» skal være en liste"),
        };
        for (si, s) in sections.iter().enumerate() {
            let sid = s.get("id").and_then(Value::as_str).map(String::from);
            let sloc = Loc { text: format!("Sektion {}", si + 1), section: sid, element: None };
            self.fields(&sloc, "", &self.defs.section.fields.clone(), s.get("settings"));
            self.visibility(&sloc, s);
            self.columns(&sloc, s);
            match s.get("fallback") {
                None | Some(Value::Null) => {}
                Some(fb @ Value::Object(_)) => {
                    let has_access = s.get("access").and_then(Value::as_array).is_some_and(|a| !a.is_empty());
                    if !has_access {
                        self.warn(&sloc, None, "sektionen har indhold uden adgang, men ingen access-regler, så det bliver aldrig vist");
                    }
                    self.columns(&sloc.child("uden adgang"), fb);
                }
                Some(_) => self.err(&sloc, None, "«fallback» skal være et objekt med layout og kolonner"),
            }
        }
    }

    /// Layout og kolonner. Bruges både til sektionen og til dens indhold uden adgang.
    fn columns(&mut self, sloc: &Loc, s: &Value) {
        let layout = s.get("layout").and_then(Value::as_str).unwrap_or("");
        let width = match self.defs.layout(layout) {
            Some(l) => l.columns.len(),
            None => {
                self.err(sloc, None, format!("layoutet «{layout}» findes ikke"));
                usize::MAX
            }
        };
        let cols = s.get("columns").and_then(Value::as_array).cloned().unwrap_or_default();
        for (ci, col) in cols.iter().enumerate() {
            let els = col.as_array().cloned().unwrap_or_default();
            if ci >= width && !els.is_empty() {
                self.err(
                    sloc,
                    None,
                    format!("kolonne {} findes ikke i layoutet «{layout}», så {} element(er) bliver ikke vist", ci + 1, els.len()),
                );
            }
            for e in &els {
                self.element(sloc, ci, e);
            }
        }
    }

    fn element(&mut self, sloc: &Loc, ci: usize, e: &Value) {
        let ty = e.get("type").and_then(Value::as_str).unwrap_or("");
        let def = self.defs.element(ty).cloned();
        let label = def.as_ref().map(|d| d.label.clone()).unwrap_or_else(|| ty.into());
        let title = ["title", "headline", "headlineText"]
            .iter()
            .find_map(|k| e.get("fields").and_then(|f| f.get(*k)).and_then(Value::as_str).filter(|t| !t.is_empty()));
        let name = match title {
            Some(t) => format!("{label} «{t}»"),
            None => label,
        };
        let mut loc = sloc.child(format!("kolonne {} › {name}", ci + 1));
        loc.element = e.get("id").and_then(Value::as_str).map(String::from);
        let Some(def) = def else {
            return self.err(&loc, None, format!("elementtypen «{ty}» findes ikke længere i definitions/"));
        };
        self.fields(&loc, "", &def.fields, e.get("fields"));
        self.fields(&loc, "settings.", &self.defs.element.fields.clone(), e.get("settings"));
        self.visibility(&loc, e);
    }
}

/// Validerer en kladde. `refs` skal indeholde alle dokumenter, kladden peger på.
pub fn document(
    defs: &Defs,
    kind: &str,
    doc_type: Option<&str>,
    draft: &Value,
    refs: &HashMap<String, RefInfo>,
    lists: &HashMap<String, Vec<String>>,
) -> Vec<Issue> {
    let mut v = V { defs, refs, lists, out: vec![] };
    if kind == "page" {
        v.page(draft);
    } else {
        let ty = doc_type.unwrap_or("");
        match defs.find_type(kind, ty) {
            Some(d) => v.fields(&Loc::default(), "", &d.fields, draft.get("fields")),
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
    let lists = s.options.all_values(&defs).await;
    Ok(document(&defs, &kind, ty.as_deref(), &draft, &refs, &lists))
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
        for p in seed["pages"].as_array().unwrap() {
            let id = p["id"].as_str().unwrap();
            refs.insert(
                id.to_string(),
                RefInfo { kind: "page".into(), doc_type: None, name: p["title"].as_str().unwrap().into(), published: !unpublished.contains(&id) },
            );
        }
        let mut all = vec![];
        for d in seed["documents"].as_array().unwrap() {
            for i in document(&defs, d["kind"].as_str().unwrap(), d["type"].as_str(), &d["draft"], &refs, &HashMap::new()) {
                all.push(format!("{}: {} – {}", d["id"], i.location, i.message));
            }
        }
        for p in seed["pages"].as_array().unwrap() {
            for i in document(&defs, "page", None, &p["draft"], &refs, &HashMap::new()) {
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
        let issues = document(&defs, "page", None, &draft, &refs, &HashMap::new());
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
    fn nested_and_conditional_fields() {
        let defs = defs();
        let refs: HashMap<String, RefInfo> = [(
            "pg_a".to_string(),
            RefInfo { kind: "page".into(), doc_type: None, name: "Kladdeside".into(), published: false },
        )]
        .into();
        let draft = json!({
            "fields": { "title": "Side", "reviewBy": "17/11" },
            "sections": [{
                "id": "s1", "layout": "1-1",
                "settings": { "background": "custom", "backgroundColor": "rød", "anchor": "Mit Anker" },
                "columns": [
                    [
                        { "id": "e1", "type": "linkList", "fields": { "links": [
                            { "label": "Ok", "link": "/x" },
                            { "label": "", "link": { "page": "pg_a" } },
                            { "label": "Ukendt", "link": { "page": "pg_findes_ikke" } }
                        ] } },
                        { "id": "e2", "type": "linkList", "fields": { "links": [] } }
                    ],
                    [
                        { "id": "e3", "type": "countdown", "fields": { "title": "T", "until": "2026-11-18T20:00",
                          "cta": { "label": "", "link": "ikke et link" } } },
                        { "id": "e4", "type": "articleList", "fields": { "title": "x", "listType": "numbered", "amount": 5, "showMore": false, "moreLink": "ikke et link",
                          "articles": { "inline": { "type": "mostRead", "fields": { "limit": 0 } } } } }
                    ]
                ]
            }]
        });
        let issues = document(&defs, "page", None, &draft, &refs, &HashMap::new());
        let all = issues.iter().map(|i| format!("{} [{}] – {}", i.location, i.field.as_deref().unwrap_or(""), i.message)).collect::<Vec<_>>().join("\n");
        let has = |s: &str| assert!(all.contains(s), "mangler «{s}» i:\n{all}");
        let hasnt = |s: &str| assert!(!all.contains(s), "burde ikke have «{s}» i:\n{all}");
        has("[reviewBy] – Gennemgå senest skal være en dato");
        has("[backgroundColor] – Baggrundsfarve skal være en farve");
        has("[anchor] – Anker-id har ikke det rigtige format. Små bogstaver");
        has("Links nr. 2 [links.1.label] – Tekst skal udfyldes");
        has("[links.1.link] – siden «Kladdeside» er ikke publiceret");
        has("Links nr. 3 «Ukendt» [links.2.link] – siden «pg_findes_ikke» findes ikke");
        has("[links] – Links skal udfyldes");
        has("[articles.inline.fields.limit] – Antal skal være mindst 1");
        // Skjulte felter valideres ikke: link uden cta-tekst og «flere»-link, når det er slået fra
        hasnt("cta.link");
        hasnt("moreLink");
    }

    #[test]
    fn new_field_types_settings_and_access() {
        let defs = defs();
        let refs: HashMap<String, RefInfo> = [(
            "acc_a".to_string(),
            RefInfo { kind: "access".into(), doc_type: Some("hasTag".into()), name: "Tag".into(), published: true },
        )]
        .into();
        let doc = |text: &str| json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": text }] }] });
        let draft = json!({
            "fields": { "title": "Side", "keywords": ["a", "a"] }, "accessMatch": "some",
            "sections": [{
                "id": "s1", "layout": "1", "access": ["acc_a"], "accessMatch": "any",
                "columns": [[
                    { "id": "e1", "type": "text", "fields": { "body": { "type": "doc", "content": [{ "type": "paragraph" }] } } },
                    { "id": "e2", "type": "text", "fields": { "body": "ren tekst" } },
                    { "id": "e3", "type": "headline", "fields": { "kind": "custom", "style": { "color": "#123456" } },
                      "settings": { "classes": ["ok", "ikke ok"], "hideOn": ["tv"] } },
                    { "id": "e4", "type": "text", "fields": { "body": doc("Hej") }, "settings": { "elementId": "top-1", "classes": ["a-b"] } }
                ]],
                "fallback": { "layout": "1", "columns": [[ { "id": "e5", "type": "ad", "fields": {} } ]] }
            }, {
                "id": "s2", "layout": "1", "columns": [[]], "fallback": { "layout": "nope", "columns": [] }
            }]
        });
        let issues = document(&defs, "page", None, &draft, &refs, &HashMap::new());
        let all = issues.iter().map(|i| format!("{:?} {} [{}] – {}", i.level, i.location, i.field.as_deref().unwrap_or(""), i.message)).collect::<Vec<_>>().join("\n");
        let has = |s: &str| assert!(all.contains(s), "mangler «{s}» i:\n{all}");
        let hasnt = |s: &str| assert!(!all.contains(s), "burde ikke have «{s}» i:\n{all}");
        has("[keywords] – Meta-nøgleord «a» står der flere gange");
        has("Side [] – «accessMatch» skal være");
        has("kolonne 1 › Tekst [body] – Tekst skal udfyldes");
        has("[body] – Tekst skal være formateret tekst");
        has("[style.color] – Farve «#123456» er ikke en af farverne i paletten");
        has("[settings.classes] – CSS-klasser «ikke ok» har ikke det rigtige format");
        has("[settings.hideOn] – Skjul på «tv» er ikke en af valgmulighederne");
        has("Sektion 1 › uden adgang › kolonne 1 › Annonce [adType] – Annoncetype skal udfyldes");
        has("Warning Sektion 2 [] – sektionen har indhold uden adgang, men ingen access-regler");
        has("Sektion 2 › uden adgang [] – layoutet «nope» findes ikke");
        hasnt("top-1");
        hasnt("Sektion 1 [] – «accessMatch»");
    }

    #[test]
    fn globals() {
        let defs = defs();
        let refs = HashMap::new();
        let menu = json!({ "fields": { "items": [
            { "kind": "link", "label": "Forside", "link": "/" },
            { "kind": "folder", "label": "Mere", "children": [{ "label": "", "link": "/x" }] },
            { "kind": "divider" }
        ] } });
        let i = document(&defs, "global", Some("menu"), &menu, &refs, &HashMap::new());
        let all = i.iter().map(|i| format!("{} [{}] – {}", i.location, i.field.as_deref().unwrap_or(""), i.message)).collect::<Vec<_>>().join("\n");
        assert_eq!(i.len(), 1, "{all}");
        assert!(all.contains("[items.1.children.0.label] – Tekst skal udfyldes"), "{all}");
        let i = document(&defs, "global", Some("findesIkke"), &json!({}), &refs, &HashMap::new());
        assert!(i[0].message.contains("definitions/globals"), "{i:?}");
    }

    #[test]
    fn options_from_service() {
        let defs = defs();
        let refs = HashMap::new();
        let ds = |section: &str| json!({ "fields": { "section": section, "sort": "newest", "limit": 5 } });
        let run = |draft: Value, lists: &HashMap<String, Vec<String>>| document(&defs, "datasource", Some("articleQuery"), &draft, &refs, lists);
        let live: HashMap<String, Vec<String>> = [("editorial:sections".to_string(), vec!["sport".into(), "motor".into()])].into();
        let none = HashMap::new();

        // Listen er hentet: den gælder, også for værdier, der ikke står i reservelisten
        assert!(run(ds("motor"), &live).is_empty());
        let i = run(ds("kultur"), &live);
        assert_eq!((i.len(), i[0].level), (1, Level::Error), "{i:?}");
        assert!(i[0].message.contains("findes ikke i editorial:sections"));
        // Listen kunne ikke hentes: reservelisten bruges, og ukendte værdier er kun en advarsel
        assert!(run(ds("kultur"), &none).is_empty());
        let i = run(ds("motor"), &none);
        assert_eq!((i.len(), i[0].level), (1, Level::Warning), "{i:?}");
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
        assert!(check_value(&fd("select", json!({ "options": [{ "value": "a", "label": "A" }] })), &json!("a")).is_none());
        assert!(check_value(&fd("select", json!({ "options": [{ "value": "a", "label": "A" }] })), &json!("A")).is_some());
        assert!(check_value(&fd("number", json!({ "min": 0, "step": 5 })), &json!(15)).is_none());
        assert!(check_value(&fd("number", json!({ "min": 0, "step": 5 })), &json!(12)).is_some());
        assert!(check_value(&fd("number", json!({ "step": 0.1 })), &json!(0.3)).is_none());
        assert!(check_value(&fd("text", json!({ "maxLength": 3 })), &json!("æøå")).is_none());
        assert!(check_value(&fd("text", json!({ "maxLength": 3 })), &json!("abcd")).is_some());
        assert!(check_value(&fd("text", json!({ "pattern": "[0-9]+" })), &json!("12a")).is_some());
        assert!(check_value(&fd("color", json!({})), &json!("#E4002B")).is_none());
        assert!(check_value(&fd("color", json!({})), &json!("#e40")).is_none());
        assert!(check_value(&fd("color", json!({})), &json!("red")).is_some());
        assert!(check_value(&fd("date", json!({})), &json!("2026-02-30")).is_some());
        assert!(check_value(&fd("link", json!({})), &json!({ "page": "pg_x" })).is_none());
        assert!(check_value(&fd("link", json!({})), &json!({ "href": "/x" })).is_some());
        assert!(check_value(&fd("multiselect", json!({ "options": ["a", "b"], "maxItems": 1 })), &json!(["a", "b"])).is_some());
    }
}
