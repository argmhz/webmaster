//! Sidetræet. Hver node har et side-dokument med samme id.

use crate::{
    error::{AppError, R},
    ids, AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

#[derive(Serialize, sqlx::FromRow, TS)]
#[ts(export, rename = "TreeNode")]
pub struct Node {
    pub id: String,
    pub parent_id: Option<String>,
    pub slug: String,
    pub title: String,
    pub sort: i32,
    #[ts(type = "number | null")]
    pub published_version: Option<i64>,
    pub changed: Option<bool>,
}

const SELECT: &str = "select n.id, n.parent_id, n.slug, n.title, n.sort,
        p.version_id as published_version,
        (p.version_id is not null and v.data is distinct from d.draft) as changed
    from nodes n
    join documents d on d.id = n.id
    left join publications p on p.document_id = n.id
    left join versions v on v.id = p.version_id";

pub async fn list(State(s): State<AppState>) -> R<Json<Vec<Node>>> {
    let rows = sqlx::query_as::<_, Node>(&format!("{SELECT} order by n.sort, n.title"))
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

async fn one(db: &sqlx::PgPool, id: &str) -> R<Node> {
    sqlx::query_as::<_, Node>(&format!("{SELECT} where n.id = $1"))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::not_found("Siden"))
}

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        match c {
            'æ' => out.push_str("ae"),
            'ø' => out.push_str("oe"),
            'å' => out.push_str("aa"),
            c if c.is_ascii_alphanumeric() => out.push(c),
            _ => out.push('-'),
        }
    }
    out.split('-').filter(|x| !x.is_empty()).collect::<Vec<_>>().join("-")
}

async fn slug_taken(conn: &mut sqlx::PgConnection, parent: &str, slug: &str, except: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("select exists(select 1 from nodes where parent_id = $1 and slug = $2 and id <> $3)")
        .bind(parent)
        .bind(slug)
        .bind(except)
        .fetch_one(conn)
        .await
}

#[derive(Deserialize)]
pub struct CreateNode {
    parent_id: String,
    title: String,
    slug: Option<String>,
}

pub async fn create(State(s): State<AppState>, Json(b): Json<CreateNode>) -> R<Json<Node>> {
    let id = ids::new("pg");
    let base = slugify(b.slug.as_deref().unwrap_or(&b.title));
    let base = if base.is_empty() { "side".to_string() } else { base };

    let mut tx = s.db.begin().await?;
    let mut slug = base.clone();
    let mut n = 2;
    while slug_taken(&mut tx, &b.parent_id, &slug, &id).await? {
        slug = format!("{base}-{n}");
        n += 1;
    }
    let sort: i32 = sqlx::query_scalar("select coalesce(max(sort) + 1, 0) from nodes where parent_id = $1")
        .bind(&b.parent_id)
        .fetch_one(&mut *tx)
        .await?;
    let draft = json!({ "fields": { "title": b.title }, "schedules": [], "access": [], "sections": [] });
    sqlx::query("insert into documents (id, kind, name, draft) values ($1, 'page', $2, $3)")
        .bind(&id)
        .bind(&b.title)
        .bind(&draft)
        .execute(&mut *tx)
        .await?;
    sqlx::query("insert into nodes (id, parent_id, slug, title, sort) values ($1, $2, $3, $4, $5)")
        .bind(&id)
        .bind(&b.parent_id)
        .bind(&slug)
        .bind(&b.title)
        .bind(sort)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(one(&s.db, &id).await?))
}

#[derive(Deserialize)]
pub struct UpdateNode {
    title: Option<String>,
    slug: Option<String>,
}

pub async fn update(State(s): State<AppState>, Path(id): Path<String>, Json(b): Json<UpdateNode>) -> R<Json<Node>> {
    let mut tx = s.db.begin().await?;
    let parent: Option<Option<String>> = sqlx::query_scalar("select parent_id from nodes where id = $1")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(parent) = parent else { return Err(AppError::not_found("Siden")) };

    if let Some(title) = &b.title {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::bad("Navnet må ikke være tomt"));
        }
        sqlx::query("update nodes set title = $2 where id = $1").bind(&id).bind(title).execute(&mut *tx).await?;
        sqlx::query("update documents set name = $2, updated_at = now() where id = $1")
            .bind(&id)
            .bind(title)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(slug) = &b.slug {
        let Some(parent) = parent else { return Err(AppError::bad("Forsiden har ingen slug")) };
        let slug = slugify(slug);
        if slug.is_empty() {
            return Err(AppError::bad("Slug må ikke være tom"));
        }
        if slug_taken(&mut tx, &parent, &slug, &id).await? {
            return Err(AppError::conflict(format!("Der findes allerede en side med slug «{slug}» samme sted")));
        }
        sqlx::query("update nodes set slug = $2 where id = $1").bind(&id).bind(&slug).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(one(&s.db, &id).await?))
}

#[derive(Deserialize)]
pub struct MoveNode {
    parent_id: String,
    index: usize,
}

pub async fn move_node(State(s): State<AppState>, Path(id): Path<String>, Json(b): Json<MoveNode>) -> R<StatusCode> {
    let mut tx = s.db.begin().await?;
    let all: Vec<(String, Option<String>, String)> = sqlx::query_as("select id, parent_id, slug from nodes")
        .fetch_all(&mut *tx)
        .await?;
    let me = all.iter().find(|n| n.0 == id).ok_or_else(|| AppError::not_found("Siden"))?;
    if me.1.is_none() {
        return Err(AppError::bad("Forsiden kan ikke flyttes"));
    }
    if !all.iter().any(|n| n.0 == b.parent_id) {
        return Err(AppError::not_found("Den nye forælder"));
    }
    // Må ikke flyttes ind under sig selv
    let mut cur = Some(b.parent_id.clone());
    while let Some(c) = cur {
        if c == id {
            return Err(AppError::bad("En side kan ikke flyttes ind under sig selv"));
        }
        cur = all.iter().find(|n| n.0 == c).and_then(|n| n.1.clone());
    }
    let slug = me.2.clone();
    if slug_taken(&mut tx, &b.parent_id, &slug, &id).await? {
        return Err(AppError::conflict(format!("Der findes allerede en side med slug «{slug}» der")));
    }

    let mut siblings: Vec<String> =
        sqlx::query_scalar("select id from nodes where parent_id = $1 and id <> $2 order by sort, title")
            .bind(&b.parent_id)
            .bind(&id)
            .fetch_all(&mut *tx)
            .await?;
    siblings.insert(b.index.min(siblings.len()), id.clone());

    sqlx::query("update nodes set parent_id = $2 where id = $1")
        .bind(&id)
        .bind(&b.parent_id)
        .execute(&mut *tx)
        .await?;
    for (i, sid) in siblings.iter().enumerate() {
        sqlx::query("update nodes set sort = $2 where id = $1")
            .bind(sid)
            .bind(i as i32)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove(State(s): State<AppState>, Path(id): Path<String>) -> R<StatusCode> {
    let all: Vec<(String, Option<String>)> = sqlx::query_as("select id, parent_id from nodes").fetch_all(&s.db).await?;
    let me = all.iter().find(|n| n.0 == id).ok_or_else(|| AppError::not_found("Siden"))?;
    if me.1.is_none() {
        return Err(AppError::bad("Forsiden kan ikke slettes"));
    }
    // Saml hele undertræet
    let mut subtree = vec![id.clone()];
    let mut i = 0;
    while i < subtree.len() {
        let cur = subtree[i].clone();
        subtree.extend(all.iter().filter(|n| n.1.as_deref() == Some(cur.as_str())).map(|n| n.0.clone()));
        i += 1;
    }
    sqlx::query("delete from documents where id = any($1)").bind(&subtree).execute(&s.db).await?;
    Ok(StatusCode::NO_CONTENT)
}
