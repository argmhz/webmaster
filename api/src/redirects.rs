use crate::{
    error::{AppError, R},
    AppState,
};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use ts_rs::TS;

#[derive(Serialize, Deserialize, sqlx::FromRow, TS)]
#[ts(export)]
pub struct Redirect {
    #[serde(rename = "from")]
    pub from_path: String,
    #[serde(rename = "to")]
    pub to_path: String,
    pub status: i32,
}

pub async fn list(State(s): State<AppState>) -> R<Json<Vec<Redirect>>> {
    let rows = sqlx::query_as::<_, Redirect>("select from_path, to_path, status from redirects order by sort, id")
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

/// Erstatter hele listen. Redirects går live med det samme.
pub async fn replace(State(s): State<AppState>, Json(list): Json<Vec<Redirect>>) -> R<Json<Vec<Redirect>>> {
    let mut seen = HashSet::new();
    for r in &list {
        if !r.from_path.starts_with('/') {
            return Err(AppError::bad(format!("«{}» skal starte med /", r.from_path)));
        }
        if r.to_path.trim().is_empty() {
            return Err(AppError::bad(format!("«{}» mangler et mål", r.from_path)));
        }
        if ![301, 302, 307, 308].contains(&r.status) {
            return Err(AppError::bad(format!("Status {} er ikke tilladt", r.status)));
        }
        if !seen.insert(r.from_path.clone()) {
            return Err(AppError::bad(format!("«{}» står der to gange", r.from_path)));
        }
    }
    let mut tx = s.db.begin().await?;
    sqlx::query("delete from redirects").execute(&mut *tx).await?;
    for (i, r) in list.iter().enumerate() {
        sqlx::query("insert into redirects (from_path, to_path, status, sort) values ($1, $2, $3, $4)")
            .bind(&r.from_path)
            .bind(r.to_path.trim())
            .bind(r.status)
            .bind(i as i32)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    list_inner(&s.db).await.map(Json)
}

async fn list_inner(db: &sqlx::PgPool) -> R<Vec<Redirect>> {
    Ok(sqlx::query_as::<_, Redirect>("select from_path, to_path, status from redirects order by sort, id")
        .fetch_all(db)
        .await?)
}
