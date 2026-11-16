//! Definitionerne er JSON-filer, som udviklerne ejer i git. De læses ved hvert
//! kald, så en ændret fil slår igennem uden genstart.

use crate::{error::R, AppState};
use anyhow::Context;
use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn read(p: &Path) -> anyhow::Result<Value> {
    let s = std::fs::read_to_string(p).with_context(|| format!("kunne ikke læse {}", p.display()))?;
    serde_json::from_str(&s).with_context(|| format!("ugyldig JSON i {}", p.display()))
}

fn read_dir(dir: &Path) -> anyhow::Result<Vec<Value>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files.iter().map(|p| read(p)).collect()
}

pub fn load(dir: &Path) -> anyhow::Result<Value> {
    Ok(json!({
        "page": read(&dir.join("page.json"))?,
        "section": read(&dir.join("section.json"))?,
        "layouts": read(&dir.join("layouts.json"))?,
        "elements": read_dir(&dir.join("elements"))?,
        "datasources": read_dir(&dir.join("datasources"))?,
        "schedules": read_dir(&dir.join("schedules"))?,
        "access": read_dir(&dir.join("access"))?,
    }))
}

/// Navnet på listen i definitionerne for en dokument-type.
pub fn list_key(kind: &str) -> &'static str {
    match kind {
        "datasource" => "datasources",
        "schedule" => "schedules",
        _ => "access",
    }
}

pub fn find_type<'a>(defs: &'a Value, kind: &str, ty: &str) -> Option<&'a Value> {
    defs[list_key(kind)].as_array()?.iter().find(|t| t["type"].as_str() == Some(ty))
}

pub async fn get_all(State(s): State<AppState>) -> R<Json<Value>> {
    Ok(Json(load(&s.defs_dir)?))
}
