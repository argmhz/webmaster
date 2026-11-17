//! Definitionerne er JSON-filer, som udviklerne ejer i git. De læses ved hvert
//! kald, så en ændret fil slår igennem uden genstart.
//!
//! Hver fil parses til typede strukturer og tjekkes. En fil med fejl springes
//! over, og fejlen kommer med i `problems`, så editoren og loggen kan vise den.
//! Manualen ligger i `definitions/README.md`.

use crate::{error::R, AppState};
use axum::{extract::State, Json};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Felttyperne. Editoren har én renderer pr. type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum FieldType {
    Text,
    Richtext,
    Number,
    Boolean,
    Select,
    Multiselect,
    Datetime,
    Time,
    Link,
    Image,
    Datasource,
}

impl FieldType {
    pub fn name(self) -> &'static str {
        match self {
            FieldType::Text => "text",
            FieldType::Richtext => "richtext",
            FieldType::Number => "number",
            FieldType::Boolean => "boolean",
            FieldType::Select => "select",
            FieldType::Multiselect => "multiselect",
            FieldType::Datetime => "datetime",
            FieldType::Time => "time",
            FieldType::Link => "link",
            FieldType::Image => "image",
            FieldType::Datasource => "datasource",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct FieldDef {
    pub key: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepts: Option<Vec<String>>,
}

impl FieldDef {
    pub fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.key)
    }
}

/// `page.json` og `section.json`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct FieldsDef {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub def_type: Option<String>,
    pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct Column {
    pub width: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct LayoutDef {
    pub layout: String,
    pub label: String,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct ElementDef {
    #[serde(rename = "type")]
    pub def_type: String,
    pub version: u32,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub fields: Vec<FieldDef>,
}

/// Datasource-, schedule- og access-typer.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, optional_fields)]
pub struct RefTypeDef {
    #[serde(rename = "type")]
    pub def_type: String,
    pub label: String,
    /// Kun datasources: hvilken service sitet skal hente data fra.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub fields: Vec<FieldDef>,
}

/// Et problem i en definitionsfil.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DefProblem {
    pub file: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Defs {
    pub page: FieldsDef,
    pub section: FieldsDef,
    pub layouts: Vec<LayoutDef>,
    pub elements: Vec<ElementDef>,
    pub datasources: Vec<RefTypeDef>,
    pub schedules: Vec<RefTypeDef>,
    pub access: Vec<RefTypeDef>,
    pub problems: Vec<DefProblem>,
}

/// Mappen i definitions/ for en dokument-slags.
pub fn list_key(kind: &str) -> &'static str {
    match kind {
        "datasource" => "datasources",
        "schedule" => "schedules",
        _ => "access",
    }
}

impl Defs {
    pub fn ref_types(&self, kind: &str) -> &[RefTypeDef] {
        match kind {
            "datasource" => &self.datasources,
            "schedule" => &self.schedules,
            "access" => &self.access,
            _ => &[],
        }
    }

    pub fn find_type(&self, kind: &str, ty: &str) -> Option<&RefTypeDef> {
        self.ref_types(kind).iter().find(|t| t.def_type == ty)
    }

    pub fn element(&self, ty: &str) -> Option<&ElementDef> {
        self.elements.iter().find(|e| e.def_type == ty)
    }

    pub fn layout(&self, key: &str) -> Option<&LayoutDef> {
        self.layouts.iter().find(|l| l.layout == key)
    }
}

/* ---------- indlæsning ---------- */

struct Loader {
    dir: PathBuf,
    problems: Vec<DefProblem>,
}

impl Loader {
    fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.dir).unwrap_or(p).display().to_string()
    }

    fn problem(&mut self, file: impl Into<String>, message: impl Into<String>) {
        self.problems.push(DefProblem { file: file.into(), message: message.into() });
    }

    fn file<T: DeserializeOwned>(&mut self, p: &Path) -> Option<T> {
        let file = self.rel(p);
        let raw = match std::fs::read_to_string(p) {
            Ok(s) => s,
            Err(e) => {
                self.problem(file, format!("kunne ikke læses: {e}"));
                return None;
            }
        };
        match serde_json::from_str(&raw) {
            Ok(v) => Some(v),
            Err(e) => {
                self.problem(file, e.to_string());
                None
            }
        }
    }

    /// Alle `*.json` i en mappe, sorteret. Returnerer (filnavn, indhold).
    fn dir<T: DeserializeOwned>(&mut self, sub: &str) -> Vec<(String, T)> {
        let dir = self.dir.join(sub);
        let Ok(entries) = std::fs::read_dir(&dir) else { return vec![] };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        files.iter().filter_map(|p| Some((self.rel(p), self.file(p)?))).collect()
    }
}

pub fn load(dir: &Path) -> Defs {
    let mut l = Loader { dir: dir.to_path_buf(), problems: vec![] };
    let empty = || FieldsDef { def_type: None, fields: vec![] };
    let page = l.file(&dir.join("page.json")).unwrap_or_else(empty);
    let section = l.file(&dir.join("section.json")).unwrap_or_else(empty);
    let layouts: Vec<LayoutDef> = l.file(&dir.join("layouts.json")).unwrap_or_default();
    let elements = l.dir::<ElementDef>("elements");
    let datasources = l.dir::<RefTypeDef>("datasources");
    let schedules = l.dir::<RefTypeDef>("schedules");
    let access = l.dir::<RefTypeDef>("access");

    let ds_types: HashSet<String> = datasources.iter().map(|(_, d)| d.def_type.clone()).collect();

    check_fields(&mut l, "page.json", &page.fields, &ds_types);
    check_fields(&mut l, "section.json", &section.fields, &ds_types);
    check_layouts(&mut l, &layouts);

    let elements = dedupe(&mut l, elements, |e| &e.def_type);
    for (f, e) in &elements {
        check_name(&mut l, f, &e.def_type);
        if e.version == 0 {
            l.problem(f, "version skal være 1 eller højere");
        }
        check_fields(&mut l, f, &e.fields, &ds_types);
    }
    let mut lists = vec![];
    for (kind, list) in [("datasource", datasources), ("schedule", schedules), ("access", access)] {
        let list = dedupe(&mut l, list, |d| &d.def_type);
        for (f, d) in &list {
            check_name(&mut l, f, &d.def_type);
            match (kind, &d.source) {
                ("datasource", None) => l.problem(f, "en datasource-type skal have «source», fx \"editorial\""),
                ("datasource", Some(_)) | (_, None) => {}
                (_, Some(_)) => l.problem(f, "«source» bruges kun på datasource-typer"),
            }
            if d.fields.iter().any(|x| x.field_type == FieldType::Datasource) {
                l.problem(f, "felter af typen datasource kan kun bruges på elementer, side og sektion");
            }
            check_fields(&mut l, f, &d.fields, &ds_types);
        }
        lists.push(list.into_iter().map(|(_, d)| d).collect::<Vec<_>>());
    }
    let access = lists.pop().unwrap();
    let schedules = lists.pop().unwrap();
    let datasources = lists.pop().unwrap();

    Defs {
        page,
        section,
        layouts,
        elements: elements.into_iter().map(|(_, e)| e).collect(),
        datasources,
        schedules,
        access,
        problems: l.problems,
    }
}

/// Fjerner typer, der findes i flere filer. Den første fil vinder.
fn dedupe<T>(l: &mut Loader, list: Vec<(String, T)>, key: impl Fn(&T) -> &String) -> Vec<(String, T)> {
    let mut seen: Vec<(String, String)> = vec![];
    let mut out = vec![];
    for (f, d) in list {
        let k = key(&d).clone();
        if let Some((first, _)) = seen.iter().find(|(_, t)| *t == k) {
            l.problem(&f, format!("typen «{k}» findes allerede i {first} og bliver ignoreret her"));
            continue;
        }
        seen.push((f.clone(), k));
        out.push((f, d));
    }
    out
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|c| c.is_ascii_alphabetic()) && c.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn check_name(l: &mut Loader, file: &str, ty: &str) {
    if !is_ident(ty) {
        l.problem(file, format!("«{ty}» er ikke et gyldigt typenavn (brug fx camelCase uden mellemrum)"));
    }
}

fn check_layouts(l: &mut Loader, layouts: &[LayoutDef]) {
    let f = "layouts.json";
    if layouts.is_empty() {
        l.problem(f, "der skal være mindst ét layout");
    }
    let mut seen = HashSet::new();
    for lay in layouts {
        if !seen.insert(&lay.layout) {
            l.problem(f, format!("layoutet «{}» findes flere gange", lay.layout));
        }
        let sum: u32 = lay.columns.iter().map(|c| c.width).sum();
        if lay.columns.is_empty() {
            l.problem(f, format!("layoutet «{}» har ingen kolonner", lay.layout));
        } else if sum != 12 {
            l.problem(f, format!("kolonnerne i «{}» fylder {sum}, ikke 12", lay.layout));
        }
    }
}

fn check_fields(l: &mut Loader, file: &str, fields: &[FieldDef], ds_types: &HashSet<String>) {
    let mut keys = BTreeSet::new();
    for fd in fields {
        let at = |m: String| format!("feltet «{}»: {m}", fd.key);
        if !is_ident(&fd.key) {
            l.problem(file, at("nøglen skal starte med et bogstav og kun indeholde bogstaver, tal og _".into()));
        }
        if !keys.insert(&fd.key) {
            l.problem(file, at("nøglen findes flere gange".into()));
        }
        let t = fd.field_type;
        let choice = matches!(t, FieldType::Select | FieldType::Multiselect);
        if choice && fd.options.as_ref().is_none_or(|o| o.is_empty()) {
            l.problem(file, at(format!("{} skal have «options»", t.name())));
        }
        if !choice && fd.options.is_some() {
            l.problem(file, at(format!("«options» bruges ikke på {}", t.name())));
        }
        if !choice && fd.options_from.is_some() {
            l.problem(file, at(format!("«optionsFrom» bruges ikke på {}", t.name())));
        }
        if let Some(of) = &fd.options_from {
            if !of.contains(':') {
                l.problem(file, at(format!("«optionsFrom» skal have formen \"service:liste\", ikke \"{of}\"")));
            }
        }
        if fd.max.is_some() && t != FieldType::Number {
            l.problem(file, at("«max» bruges kun på number".into()));
        }
        match (t, &fd.accepts) {
            (FieldType::Datasource, None) => l.problem(file, at("datasource-felter skal have «accepts»".into())),
            (FieldType::Datasource, Some(acc)) => {
                if acc.is_empty() {
                    l.problem(file, at("«accepts» må ikke være tom".into()));
                }
                for a in acc {
                    if !ds_types.contains(a) {
                        l.problem(file, at(format!("«accepts» nævner «{a}», som ikke er en datasource-type")));
                    }
                }
            }
            (_, Some(_)) => l.problem(file, at("«accepts» bruges kun på datasource".into())),
            (_, None) => {}
        }
        if let Some(d) = &fd.default {
            if t == FieldType::Datasource {
                l.problem(file, at("datasource-felter kan ikke have «default»".into()));
            } else if let Some(m) = crate::validate::check_value(fd, d) {
                l.problem(file, at(format!("«default» er ugyldig: {m}")));
            }
        }
    }
}

pub async fn get_all(State(s): State<AppState>) -> R<Json<Defs>> {
    Ok(Json(load(&s.defs_dir)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definitions_in_repo_are_valid() {
        let defs = load(Path::new("../definitions"));
        let msgs: Vec<String> = defs.problems.iter().map(|p| format!("{}: {}", p.file, p.message)).collect();
        assert!(msgs.is_empty(), "problemer i definitions/:\n{}", msgs.join("\n"));
        assert!(!defs.elements.is_empty());
    }

    #[test]
    fn reports_problems() {
        let dir = std::env::temp_dir().join(format!("wm-defs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("elements")).unwrap();
        std::fs::create_dir_all(dir.join("datasources")).unwrap();
        std::fs::write(dir.join("page.json"), r#"{"fields":[]}"#).unwrap();
        std::fs::write(dir.join("section.json"), r#"{"fields":[]}"#).unwrap();
        std::fs::write(dir.join("layouts.json"), r#"[{"layout":"1","label":"x","columns":[{"width":10}]}]"#).unwrap();
        std::fs::write(
            dir.join("elements/a.json"),
            r#"{"type":"a","version":1,"label":"A","fields":[
                {"key":"s","type":"select"},
                {"key":"d","type":"datasource","accepts":["nope"]},
                {"key":"n","type":"number","default":"x"}
            ]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("elements/b.json"), r#"{"type":"b","version":1,"label":"B","fields":[{"key":"x","type":"txt"}]}"#).unwrap();
        std::fs::write(dir.join("elements/c.json"), r#"{"type":"c","version":1,"label":"C","feilds":[]}"#).unwrap();
        let defs = load(&dir);
        let all = defs.problems.iter().map(|p| format!("{}: {}", p.file, p.message)).collect::<Vec<_>>().join("\n");
        assert!(all.contains("fylder 10"), "{all}");
        assert!(all.contains("select skal have «options»"), "{all}");
        assert!(all.contains("«nope»"), "{all}");
        assert!(all.contains("«default» er ugyldig"), "{all}");
        assert!(all.contains("elements/b.json") && all.contains("txt"), "{all}");
        assert!(all.contains("elements/c.json") && all.contains("feilds"), "{all}");
        assert_eq!(defs.elements.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
