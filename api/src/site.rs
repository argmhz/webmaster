//! Det API, sitet læser. Det leverer kun konfiguration: sitet resolver selv
//! datasources og afgør synlighed ud fra schedules og access.

use crate::{
    defs::{self, Defs},
    error::{AppError, R},
    refs, AppState,
};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

type NodeRow = (String, Option<String>, String);

fn compute_paths(nodes: &[NodeRow]) -> HashMap<String, String> {
    let by_id: HashMap<&str, &NodeRow> = nodes.iter().map(|n| (n.0.as_str(), n)).collect();
    nodes
        .iter()
        .map(|n| {
            let mut parts = vec![];
            let mut cur = Some(n);
            let mut guard = 0;
            while let Some(c) = cur {
                if !c.2.is_empty() {
                    parts.push(c.2.as_str());
                }
                cur = c.1.as_deref().and_then(|p| by_id.get(p).copied());
                guard += 1;
                if guard > 64 {
                    break;
                }
            }
            parts.reverse();
            (n.0.clone(), format!("/{}", parts.join("/")))
        })
        .collect()
}

fn normalize(path: &str) -> String {
    let p = path.trim().trim_end_matches('/');
    if p.is_empty() {
        "/".into()
    } else if p.starts_with('/') {
        p.into()
    } else {
        format!("/{p}")
    }
}

#[derive(Deserialize)]
pub struct PageQ {
    path: String,
    #[serde(default)]
    preview: bool,
}

pub async fn page(State(s): State<AppState>, Query(q): Query<PageQ>) -> R<Json<Value>> {
    let all_defs = defs::load(&s.defs_dir);
    let nodes: Vec<NodeRow> = sqlx::query_as("select id, parent_id, slug from nodes").fetch_all(&s.db).await?;
    let paths = compute_paths(&nodes);
    let want = normalize(&q.path);

    let Some(id) = paths.iter().find(|(_, p)| **p == want).map(|(id, _)| id.clone()) else {
        let r: Option<(String, i32)> = sqlx::query_as("select to_path, status from redirects where from_path = $1")
            .bind(&want)
            .fetch_optional(&s.db)
            .await?;
        if let Some((to, status)) = r {
            return Ok(Json(json!({ "redirect": { "to": to, "status": status } })));
        }
        return Err(AppError::not_found("Siden"));
    };

    let (data, version, published_at): (Value, Option<i64>, Option<DateTime<Utc>>) = if q.preview {
        let d: Value = sqlx::query_scalar("select draft from documents where id = $1")
            .bind(&id)
            .fetch_one(&s.db)
            .await?;
        (d, None, None)
    } else {
        let row: Option<(Value, i64, DateTime<Utc>)> = sqlx::query_as(
            "select v.data, v.id, v.published_at from publications p join versions v on v.id = p.version_id
              where p.document_id = $1",
        )
        .bind(&id)
        .fetch_optional(&s.db)
        .await?;
        let Some((d, v, at)) = row else {
            return Err(AppError(StatusCode::NOT_FOUND, "Siden er ikke publiceret endnu".into(), None));
        };
        (d, Some(v), Some(at))
    };

    // Referencer: kladder i preview, ellers kun publicerede versioner
    let ref_ids: Vec<String> = refs::collect(&data).into_iter().collect();
    let sql = if q.preview {
        "select id, kind, type, name, draft from documents where id = any($1)"
    } else {
        "select d.id, d.kind, v.type, v.name, v.data from documents d
           join publications p on p.document_id = d.id
           join versions v on v.id = p.version_id
          where d.id = any($1)"
    };
    let rows: Vec<(String, String, Option<String>, String, Value)> =
        sqlx::query_as(sql).bind(&ref_ids).fetch_all(&s.db).await?;

    let (mut ds, mut sch, mut acc) = (Map::new(), Map::new(), Map::new());
    for (rid, kind, ty, name, d) in rows {
        let fields = d.get("fields").cloned().unwrap_or_else(|| json!({}));
        match kind.as_str() {
            "datasource" => {
                let source = ty.as_deref().and_then(|t| all_defs.find_type("datasource", t)).and_then(|t| t.source.clone());
                ds.insert(rid, json!({ "type": ty, "source": source, "name": name, "fields": fields }));
            }
            "schedule" => {
                sch.insert(rid, json!({ "type": ty, "name": name, "fields": fields }));
            }
            "access" => {
                acc.insert(rid, json!({ "type": ty, "name": name, "fields": fields }));
            }
            _ => {}
        }
    }
    let missing: Vec<&String> = ref_ids
        .iter()
        .filter(|r| !ds.contains_key(*r) && !sch.contains_key(*r) && !acc.contains_key(*r))
        .collect();

    let mut sections = sections_out(&data, &all_defs);
    for s in sections.iter_mut() {
        enrich_inline(s, &all_defs);
    }

    Ok(Json(json!({
        "id": id,
        "path": want,
        "preview": q.preview,
        "version": version,
        "publishedAt": published_at,
        "visibility": { "schedules": "any", "access": "all" },
        "fields": data.get("fields").cloned().unwrap_or_else(|| json!({})),
        "schedules": data.get("schedules").cloned().unwrap_or_else(|| json!([])),
        "access": data.get("access").cloned().unwrap_or_else(|| json!([])),
        "sections": sections,
        "refs": { "datasources": ds, "schedules": sch, "access": acc },
        "missingRefs": missing,
    })))
}

/// Lokale datasources (`{ "inline": { type, fields } }`) får `source` fra
/// definitionen, så sitet kan resolve dem på samme måde som delte.
fn enrich_inline(v: &mut Value, all_defs: &Defs) {
    match v {
        Value::Object(m) => {
            if let Some(Value::Object(inline)) = m.get_mut("inline") {
                let source = inline
                    .get("type")
                    .and_then(|t| t.as_str())
                    .and_then(|t| all_defs.find_type("datasource", t))
                    .and_then(|d| d.source.clone());
                if let Some(source) = source {
                    inline.insert("source".into(), Value::String(source));
                }
            }
            for (_, x) in m.iter_mut() {
                enrich_inline(x, all_defs);
            }
        }
        Value::Array(a) => {
            for x in a {
                enrich_inline(x, all_defs);
            }
        }
        _ => {}
    }
}

/// Omdanner `columns: [[el, el], [el]]` til `columns: [{ width, elements }]`
/// og sætter elementtypens version på hvert element.
fn sections_out(data: &Value, all_defs: &Defs) -> Vec<Value> {
    data["sections"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.is_object())
        .map(|mut s| {
            let layout = s["layout"].as_str().unwrap_or("1").to_string();
            let widths: Vec<u32> = all_defs
                .layout(&layout)
                .map(|l| l.columns.iter().map(|c| c.width).collect())
                .unwrap_or_else(|| vec![12]);
            let cols = s["columns"].as_array().cloned().unwrap_or_default();
            let out: Vec<Value> = widths
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    let els: Vec<Value> = cols
                        .get(i)
                        .and_then(|c| c.as_array())
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|mut e| {
                            let v = e["type"].as_str().and_then(|t| all_defs.element(t)).map(|d| json!(d.version));
                            if let (Some(v), Some(obj)) = (v, e.as_object_mut()) {
                                obj.insert("version".into(), v);
                            }
                            e
                        })
                        .collect();
                    json!({ "width": w, "elements": els })
                })
                .collect();
            s["columns"] = Value::Array(out);
            s
        })
        .collect()
}

#[derive(Deserialize)]
pub struct TreeQ {
    #[serde(default)]
    preview: bool,
}

pub async fn tree(State(s): State<AppState>, Query(q): Query<TreeQ>) -> R<Json<Value>> {
    let rows: Vec<(String, Option<String>, String, String, bool)> = sqlx::query_as(
        "select n.id, n.parent_id, n.slug, n.title, (p.document_id is not null)
           from nodes n left join publications p on p.document_id = n.id
          order by n.sort, n.title",
    )
    .fetch_all(&s.db)
    .await?;
    let nodes: Vec<NodeRow> = rows.iter().map(|r| (r.0.clone(), r.1.clone(), r.2.clone())).collect();
    let paths = compute_paths(&nodes);
    Ok(Json(Value::Array(
        rows.iter()
            .filter(|r| q.preview || r.4)
            .map(|r| json!({ "id": r.0, "parentId": r.1, "path": paths[&r.0], "title": r.3 }))
            .collect(),
    )))
}
