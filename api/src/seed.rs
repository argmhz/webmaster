//! Fylder en tom database med eksempeldata fra seed.json.

use crate::refs;
use serde_json::Value;

pub async fn run(db: &sqlx::PgPool, file: &str) -> anyhow::Result<()> {
    let count: i64 = sqlx::query_scalar("select count(*) from documents").fetch_one(db).await?;
    if count > 0 {
        return Ok(());
    }
    let Ok(raw) = std::fs::read_to_string(file) else {
        tracing::warn!("ingen seed-fil på {file}");
        return Ok(());
    };
    let seed: Value = serde_json::from_str(&raw)?;
    let mut tx = db.begin().await?;

    for d in seed["documents"].as_array().into_iter().flatten() {
        sqlx::query("insert into documents (id, kind, type, name, draft) values ($1, $2, $3, $4, $5)")
            .bind(d["id"].as_str())
            .bind(d["kind"].as_str())
            .bind(d["type"].as_str())
            .bind(d["name"].as_str())
            .bind(&d["draft"])
            .execute(&mut *tx)
            .await?;
    }
    for (i, p) in seed["pages"].as_array().into_iter().flatten().enumerate() {
        sqlx::query("insert into documents (id, kind, name, draft) values ($1, 'page', $2, $3)")
            .bind(p["id"].as_str())
            .bind(p["title"].as_str())
            .bind(&p["draft"])
            .execute(&mut *tx)
            .await?;
        sqlx::query("insert into nodes (id, parent_id, slug, title, sort) values ($1, $2, $3, $4, $5)")
            .bind(p["id"].as_str())
            .bind(p["parent"].as_str())
            .bind(p["slug"].as_str().unwrap_or(""))
            .bind(p["title"].as_str())
            .bind(i as i32)
            .execute(&mut *tx)
            .await?;
    }

    let docs: Vec<(String, Value)> = sqlx::query_as("select id, draft from documents").fetch_all(&mut *tx).await?;
    for (id, draft) in &docs {
        refs::save(&mut tx, id, draft).await?;
    }

    let unpublished: Vec<String> = seed["unpublished"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    sqlx::query(
        "with v as (
            insert into versions (document_id, name, type, data, published_by)
            select id, name, type, draft, 'seed' from documents where not (id = any($1))
            returning id, document_id
         )
         insert into publications (document_id, version_id) select document_id, id from v",
    )
    .bind(&unpublished)
    .execute(&mut *tx)
    .await?;

    for (i, r) in seed["redirects"].as_array().into_iter().flatten().enumerate() {
        sqlx::query("insert into redirects (from_path, to_path, status, sort) values ($1, $2, $3, $4)")
            .bind(r["from"].as_str())
            .bind(r["to"].as_str())
            .bind(r["status"].as_i64().unwrap_or(301) as i32)
            .bind(i as i32)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    tracing::info!("database seedet med {} dokumenter", docs.len());
    Ok(())
}
