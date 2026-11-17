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
use crate::services::ServiceDef;
use std::collections::{BTreeMap, BTreeSet, HashSet};
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
    Date,
    Datetime,
    Time,
    Color,
    Link,
    Image,
    Datasource,
    Group,
    List,
}

impl FieldType {
    pub fn name(self) -> &'static str {
        use FieldType::*;
        match self {
            Text => "text",
            Richtext => "richtext",
            Number => "number",
            Boolean => "boolean",
            Select => "select",
            Multiselect => "multiselect",
            Date => "date",
            Datetime => "datetime",
            Time => "time",
            Color => "color",
            Link => "link",
            Image => "image",
            Datasource => "datasource",
            Group => "group",
            List => "list",
        }
    }
}

/// En valgmulighed: enten bare værdien, eller værdi og den tekst, redaktøren ser.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum OptionDef {
    Plain(String),
    Labeled {
        value: String,
        label: String,
    },
}

impl OptionDef {
    pub fn value(&self) -> &str {
        match self {
            OptionDef::Plain(v) | OptionDef::Labeled { value: v, .. } => v,
        }
    }
}

/// Betingelse for at vise et felt. Præcis én af `equals`, `in` og `notEmpty` skal være sat.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct Condition {
    /// Nøglen på et andet felt i samme liste af felter.
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equals: Option<Value>,
    #[serde(rename = "in", skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_empty: Option<bool>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_when: Option<Condition>,
    /// Tekst i et tomt felt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    // select / multiselect
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<OptionDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options_from: Option<String>,
    // number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    // text / richtext
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    /// Regulært udtryk, som hele værdien skal matche.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    // multiselect / list
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u32>,
    // datasource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepts: Option<Vec<String>>,
    // group / list
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<FieldDef>>,
    /// list: nøglen på det underfelt, hvis værdi bruges som overskrift på hvert punkt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_label: Option<String>,
}

impl FieldDef {
    pub fn label(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.key)
    }
    pub fn sub_fields(&self) -> &[FieldDef] {
        self.fields.as_deref().unwrap_or(&[])
    }
    /// Værdien er en af valgmulighederne (eller kommer fra `optionsFrom`, som ikke kan tjekkes her).
    pub fn allows_option(&self, v: &str) -> bool {
        self.options_from.is_some() || self.options.as_ref().is_some_and(|o| o.iter().any(|x| x.value() == v))
    }
}

/// Nøgler, som API'et selv bruger til at finde referencer i dokumenterne.
pub const RESERVED_KEYS: [&str; 5] = ["ref", "inline", "page", "schedules", "access"];

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
    /// Fra services.json. Kun skabelonerne: værdier fra miljøvariabler (fx tokens) er ikke med.
    pub services: BTreeMap<String, ServiceDef>,
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

    let services: BTreeMap<String, ServiceDef> = if dir.join("services.json").exists() {
        l.file(&dir.join("services.json")).unwrap_or_default()
    } else {
        BTreeMap::new()
    };
    for (name, svc) in &services {
        for m in crate::services::check(name, svc) {
            l.problem("services.json", m);
        }
    }
    let known = Known {
        ds_types: datasources.iter().map(|(_, d)| d.def_type.clone()).collect(),
        lists: services.iter().flat_map(|(n, s)| s.lists.keys().map(move |k| format!("{n}:{k}"))).collect(),
    };

    check_fields(&mut l, "page.json", &page.fields, &known);
    check_fields(&mut l, "section.json", &section.fields, &known);
    check_layouts(&mut l, &layouts);

    let elements = dedupe(&mut l, elements, |e| &e.def_type);
    for (f, e) in &elements {
        check_name(&mut l, f, &e.def_type);
        if e.version == 0 {
            l.problem(f, "version skal være 1 eller højere");
        }
        check_fields(&mut l, f, &e.fields, &known);
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
            if has_datasource(&d.fields) {
                l.problem(f, "felter af typen datasource kan kun bruges på elementer, side og sektion");
            }
            check_fields(&mut l, f, &d.fields, &known);
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
        services,
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

/// Det, felterne må henvise til.
struct Known {
    ds_types: HashSet<String>,
    /// "service:liste" fra services.json
    lists: HashSet<String>,
}

fn check_fields(l: &mut Loader, file: &str, fields: &[FieldDef], known: &Known) {
    check_fields_in(l, file, "", fields, known)
}

fn check_fields_in(l: &mut Loader, file: &str, parent: &str, fields: &[FieldDef], known: &Known) {
    use FieldType::*;
    let mut keys = BTreeSet::new();
    for fd in fields {
        let name = format!("{parent}{}", fd.key);
        let at = |m: String| format!("feltet «{name}»: {m}");
        let mut bad = |m: String| l.problems.push(DefProblem { file: file.into(), message: at(m) });
        let t = fd.field_type;
        let tn = t.name();
        if !is_ident(&fd.key) {
            bad("nøglen skal starte med et bogstav og kun indeholde bogstaver, tal og _".into());
        }
        if RESERVED_KEYS.contains(&fd.key.as_str()) {
            bad(format!("nøglen «{}» er reserveret. Reserverede nøgler: {}", fd.key, RESERVED_KEYS.join(", ")));
        }
        if !keys.insert(&fd.key) {
            bad("nøglen findes flere gange".into());
        }
        // Egenskaber, der kun hører til bestemte typer
        let only = |prop: &str, set: bool, ok: &[FieldType]| -> Option<String> {
            (set && !ok.contains(&t)).then(|| {
                let names: Vec<&str> = ok.iter().map(|x| x.name()).collect();
                format!("«{prop}» bruges kun på {}, ikke {tn}", names.join(" og "))
            })
        };
        let choice = [Select, Multiselect];
        let texts = [Text, Richtext];
        for m in [
            only("options", fd.options.is_some(), &choice),
            only("optionsFrom", fd.options_from.is_some(), &choice),
            only("min", fd.min.is_some(), &[Number]),
            only("max", fd.max.is_some(), &[Number]),
            only("step", fd.step.is_some(), &[Number]),
            only("minLength", fd.min_length.is_some(), &texts),
            only("maxLength", fd.max_length.is_some(), &texts),
            only("pattern", fd.pattern.is_some(), &texts),
            only("placeholder", fd.placeholder.is_some(), &[Text, Richtext, Number, Link, Image]),
            only("minItems", fd.min_items.is_some(), &[Multiselect, List]),
            only("maxItems", fd.max_items.is_some(), &[Multiselect, List]),
            only("accepts", fd.accepts.is_some(), &[Datasource]),
            only("fields", fd.fields.is_some(), &[Group, List]),
            only("itemLabel", fd.item_label.is_some(), &[List]),
        ]
        .into_iter()
        .flatten()
        {
            bad(m);
        }

        if choice.contains(&t) {
            match &fd.options {
                None => bad(format!("{tn} skal have «options»")),
                Some(o) if o.is_empty() => bad("«options» må ikke være tom".into()),
                Some(o) => {
                    let mut seen = HashSet::new();
                    for x in o {
                        if !seen.insert(x.value()) {
                            bad(format!("valgmuligheden «{}» findes flere gange", x.value()));
                        }
                    }
                }
            }
        }
        if let Some(of) = &fd.options_from {
            if !of.contains(':') {
                bad(format!("«optionsFrom» skal have formen \"service:liste\", ikke \"{of}\""));
            } else if !known.lists.contains(of) {
                bad(format!("«optionsFrom» peger på «{of}», som ikke står i services.json"));
            }
        }
        if let (Some(a), Some(b)) = (fd.min, fd.max) {
            if a > b {
                bad("«min» er større end «max»".into());
            }
        }
        if fd.step.is_some_and(|s| s <= 0.0) {
            bad("«step» skal være større end 0".into());
        }
        if let (Some(a), Some(b)) = (fd.min_length, fd.max_length) {
            if a > b {
                bad("«minLength» er større end «maxLength»".into());
            }
        }
        if let (Some(a), Some(b)) = (fd.min_items, fd.max_items) {
            if a > b {
                bad("«minItems» er større end «maxItems»".into());
            }
        }
        if let Some(p) = &fd.pattern {
            if let Err(e) = regex::Regex::new(p) {
                bad(format!("«pattern» er ikke et gyldigt regulært udtryk: {e}"));
            }
        }
        match (t, &fd.accepts) {
            (Datasource, None) => bad("datasource-felter skal have «accepts»".into()),
            (Datasource, Some(acc)) => {
                if acc.is_empty() {
                    bad("«accepts» må ikke være tom".into());
                }
                for a in acc {
                    if !known.ds_types.contains(a) {
                        bad(format!("«accepts» nævner «{a}», som ikke er en datasource-type"));
                    }
                }
            }
            _ => {}
        }
        if matches!(t, Group | List) {
            match &fd.fields {
                None => bad(format!("{tn} skal have «fields»")),
                Some(f) if f.is_empty() => bad("«fields» må ikke være tom".into()),
                Some(_) => {}
            }
        }
        if let Some(il) = &fd.item_label {
            if !fd.sub_fields().iter().any(|f| &f.key == il) {
                bad(format!("«itemLabel» peger på «{il}», som ikke er et af underfelterne"));
            }
        }
        if let Some(d) = &fd.default {
            if matches!(t, Datasource | Group | List) {
                bad(format!("{tn}-felter kan ikke have «default». Sæt den på underfelterne i stedet"));
            } else if let Some(m) = crate::validate::check_value(fd, d) {
                bad(format!("«default» er ugyldig: {m}"));
            }
        }
        if let Some(c) = &fd.show_when {
            let ops = [c.equals.is_some(), c.one_of.is_some(), c.not_empty.is_some()].iter().filter(|x| **x).count();
            if ops != 1 {
                bad("«showWhen» skal have præcis én af «equals», «in» og «notEmpty»".into());
            }
            if c.field == fd.key {
                bad("«showWhen» kan ikke afhænge af feltet selv".into());
            } else if !fields.iter().any(|f| f.key == c.field) {
                bad(format!("«showWhen» peger på «{}», som ikke er et felt ved siden af", c.field));
            }
        }
        if let Some(sub) = &fd.fields {
            check_fields_in(l, file, &format!("{name}."), sub, known);
        }
    }
}

/// Om der et sted i felterne (også inde i group og list) er et datasource-felt.
pub fn has_datasource(fields: &[FieldDef]) -> bool {
    fields.iter().any(|f| f.field_type == FieldType::Datasource || has_datasource(f.sub_fields()))
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
        std::fs::write(
            dir.join("elements/d.json"),
            r#"{"type":"d","version":1,"label":"D","fields":[
                {"key":"ref","type":"text"},
                {"key":"g","type":"group"},
                {"key":"l","type":"list","itemLabel":"nej","fields":[{"key":"x","type":"text","pattern":"("}]},
                {"key":"w","type":"text","showWhen":{"field":"mangler","equals":1}},
                {"key":"n","type":"number","min":5,"max":1,"maxLength":3},
                {"key":"s","type":"select","options":["a",{"value":"a","label":"A"}]}
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
        assert!(all.contains("«ref» er reserveret"), "{all}");
        assert!(all.contains("feltet «g»: group skal have «fields»"), "{all}");
        assert!(all.contains("«itemLabel» peger på «nej»"), "{all}");
        assert!(all.contains("feltet «l.x»: «pattern» er ikke et gyldigt"), "{all}");
        assert!(all.contains("«showWhen» peger på «mangler»"), "{all}");
        assert!(all.contains("«min» er større end «max»"), "{all}");
        assert!(all.contains("«maxLength» bruges kun på text og richtext, ikke number"), "{all}");
        assert!(all.contains("valgmuligheden «a» findes flere gange"), "{all}");
        assert_eq!(defs.elements.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
