//! Finder referencer i et dokument:
//!   `{ "ref": "ds_..." }`            – datasource-felter
//!   `"schedules": ["sch_..."]`       – på side, sektion og element
//!   `"access": ["acc_..."]`          – på side, sektion og element

use serde_json::Value;
use std::collections::BTreeSet;

pub fn collect(v: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    walk(v, &mut out);
    out
}

fn walk(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                match (k.as_str(), x) {
                    ("ref", Value::String(s)) => {
                        out.insert(s.clone());
                    }
                    ("schedules" | "access", Value::Array(a)) => {
                        for i in a {
                            if let Value::String(s) = i {
                                out.insert(s.clone());
                            }
                        }
                    }
                    _ => walk(x, out),
                }
            }
        }
        Value::Array(a) => {
            for x in a {
                walk(x, out)
            }
        }
        _ => {}
    }
}

pub async fn save(conn: &mut sqlx::PgConnection, from: &str, draft: &Value) -> sqlx::Result<()> {
    sqlx::query("delete from refs where from_id = $1").bind(from).execute(&mut *conn).await?;
    let ids: Vec<String> = collect(draft).into_iter().collect();
    if !ids.is_empty() {
        sqlx::query("insert into refs (from_id, to_id) select $1, unnest($2::text[]) on conflict do nothing")
            .bind(from)
            .bind(ids)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}
