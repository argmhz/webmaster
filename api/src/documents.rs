//! Dokumenter: sider, datasources, schedules, access og globals.
//! Kladden gemmes løbende; publicering laver en uforanderlig version.

use crate::{
    auth::{require, Role, User},
    defs,
    error::{AppError, R},
    ids, refs, validate,
    webhook::{Event, Webhooks},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};

/// Globals styrer hele sitet og må kun ændres af admins.
fn require_kind(u: &User, kind: &str) -> R<()> {
    if kind == "global" {
        require(u, Role::Admin)?;
    }
    Ok(())
}
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

/// Dokumentets slags. I databasen er det en tekst; typen er her for TypeScript.
#[allow(dead_code)]
#[derive(Serialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Kind {
    Page,
    Datasource,
    Schedule,
    Access,
    Global,
}

#[derive(Serialize, sqlx::FromRow, TS)]
#[ts(export)]
pub struct Doc {
    pub id: String,
    #[ts(as = "Kind")]
    pub kind: String,
    #[serde(rename = "type")]
    pub doc_type: Option<String>,
    pub name: String,
    pub updated_at: DateTime<Utc>,
    /// E-mail på den, der sidst gemte kladden.
    pub updated_by: String,
    #[ts(type = "number | null")]
    pub published_version: Option<i64>,
    pub published_at: Option<DateTime<Utc>>,
    pub changed: Option<bool>,
    #[ts(type = "number")]
    pub used_by: i64,
    /// Kun med, når ét dokument hentes. Formen afhænger af slags og type.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "any", optional)]
    pub draft: Option<Value>,
}

fn select(with_draft: bool) -> String {
    format!(
        "select d.id, d.kind, d.type as doc_type, d.name, d.updated_at, d.updated_by,
            p.version_id as published_version, v.published_at,
            (p.version_id is not null and (v.data is distinct from d.draft or v.name is distinct from d.name)) as changed,
            (select count(*) from refs r where r.to_id = d.id) as used_by,
            {} as draft
        from documents d
        left join publications p on p.document_id = d.id
        left join versions v on v.id = p.version_id",
        if with_draft { "d.draft" } else { "null::jsonb" }
    )
}

async fn one(db: &sqlx::PgPool, id: &str) -> R<Doc> {
    sqlx::query_as::<_, Doc>(&format!("{} where d.id = $1", select(true)))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::not_found("Dokumentet"))
}

#[derive(Deserialize)]
pub struct ListQ {
    kind: Option<String>,
}

pub async fn list(State(s): State<AppState>, Query(q): Query<ListQ>) -> R<Json<Vec<Doc>>> {
    let rows = sqlx::query_as::<_, Doc>(&format!(
        "{} where ($1::text is null or d.kind = $1) order by lower(d.name)",
        select(false)
    ))
    .bind(q.kind)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

pub async fn get(State(s): State<AppState>, Path(id): Path<String>) -> R<Json<Doc>> {
    Ok(Json(one(&s.db, &id).await?))
}

#[derive(Deserialize)]
pub struct CreateDoc {
    kind: String,
    #[serde(rename = "type")]
    doc_type: String,
    name: String,
    draft: Option<Value>,
}

pub async fn create(State(s): State<AppState>, Extension(u): Extension<User>, Json(b): Json<CreateDoc>) -> R<Json<Doc>> {
    require_kind(&u, &b.kind)?;
    if !matches!(b.kind.as_str(), "datasource" | "schedule" | "access" | "global") {
        return Err(AppError::bad("Sider oprettes i sidetræet"));
    }
    let defs = defs::load(&s.defs_dir);
    let Some(def) = defs.find_type(&b.kind, &b.doc_type) else {
        return Err(AppError::bad(format!("Typen «{}» findes ikke for {}", b.doc_type, b.kind)));
    };
    // En global uden «multiple» har ét dokument med et fast id, så sitet kan slå den op på typen.
    let id = if b.kind == "global" && !def.multiple {
        format!("gl_{}", b.doc_type)
    } else {
        ids::new(ids::prefix_for(&b.kind))
    };
    let exists: bool = sqlx::query_scalar("select exists(select 1 from documents where id = $1)")
        .bind(&id)
        .fetch_one(&s.db)
        .await?;
    if exists {
        return Err(AppError::conflict(format!("«{}» findes allerede", def.label)));
    }
    let draft = b.draft.unwrap_or_else(|| json!({ "fields": {} }));
    let mut tx = s.db.begin().await?;
    sqlx::query("insert into documents (id, kind, type, name, draft, updated_by) values ($1, $2, $3, $4, $5, $6)")
        .bind(&id)
        .bind(&b.kind)
        .bind(&b.doc_type)
        .bind(b.name.trim())
        .bind(&draft)
        .bind(&u.email)
        .execute(&mut *tx)
        .await?;
    refs::save(&mut tx, &id, &draft).await?;
    tx.commit().await?;
    Ok(Json(one(&s.db, &id).await?))
}

#[derive(Deserialize)]
pub struct SaveDoc {
    name: Option<String>,
    draft: Option<Value>,
}

pub async fn save(State(s): State<AppState>, Extension(u): Extension<User>, Path(id): Path<String>, Json(b): Json<SaveDoc>) -> R<Json<Doc>> {
    let mut tx = s.db.begin().await?;
    let kind: Option<String> = sqlx::query_scalar("select kind from documents where id = $1 for update")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?;
    let Some(kind) = kind else { return Err(AppError::not_found("Dokumentet")) };
    require_kind(&u, &kind)?;
    sqlx::query("update documents set updated_by = $2 where id = $1").bind(&id).bind(&u.email).execute(&mut *tx).await?;

    if let Some(name) = &b.name {
        if kind == "page" {
            return Err(AppError::bad("Sidens navn ændres i sidetræet"));
        }
        sqlx::query("update documents set name = $2, updated_at = now() where id = $1")
            .bind(&id)
            .bind(name.trim())
            .execute(&mut *tx)
            .await?;
    }
    if let Some(draft) = &b.draft {
        sqlx::query("update documents set draft = $2, updated_at = now() where id = $1")
            .bind(&id)
            .bind(draft)
            .execute(&mut *tx)
            .await?;
        refs::save(&mut tx, &id, draft).await?;
    }
    tx.commit().await?;
    Ok(Json(one(&s.db, &id).await?))
}

pub async fn remove(State(s): State<AppState>, Extension(u): Extension<User>, Path(id): Path<String>) -> R<StatusCode> {
    require(&u, Role::Publisher)?;
    let doc = one(&s.db, &id).await?;
    require_kind(&u, &doc.kind)?;
    if doc.kind == "page" {
        return Err(AppError::bad("Sider slettes i sidetræet"));
    }
    if doc.used_by > 0 {
        return Err(AppError::conflict(format!(
            "«{}» bruges stadig {} sted(er). Fjern den derfra først.",
            doc.name, doc.used_by
        )));
    }
    sqlx::query("delete from documents where id = $1").bind(&id).execute(&s.db).await?;
    tracing::info!(document = %id, by = u.email, "slettet");
    if doc.published_version.is_some() {
        s.webhooks.send(Webhooks::body(Event::Delete, Some(&id), Some(&doc.kind), doc.doc_type.as_deref(), vec![], &u.email));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn publish(State(s): State<AppState>, Extension(u): Extension<User>, Path(id): Path<String>) -> R<Json<Doc>> {
    require(&u, Role::Publisher)?;
    let doc = one(&s.db, &id).await?;
    require_kind(&u, &doc.kind)?;
    let issues = validate::stored(&s, &id).await?;
    let errors = issues.iter().filter(|i| i.level == validate::Level::Error).count();
    if errors > 0 {
        return Err(AppError(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Kan ikke publiceres: {errors} fejl skal rettes først"),
            Some(serde_json::to_value(&issues)?),
        ));
    }
    let mut tx = s.db.begin().await?;
    let row: Option<(String, Option<String>, Value)> =
        sqlx::query_as("select name, type, draft from documents where id = $1 for update")
            .bind(&id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((name, ty, draft)) = row else { return Err(AppError::not_found("Dokumentet")) };
    let version_id: i64 =
        sqlx::query_scalar("insert into versions (document_id, name, type, data, published_by) values ($1, $2, $3, $4, $5) returning id")
            .bind(&id)
            .bind(&name)
            .bind(&ty)
            .bind(&draft)
            .bind(&u.email)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query(
        "insert into publications (document_id, version_id) values ($1, $2)
         on conflict (document_id) do update set version_id = excluded.version_id",
    )
    .bind(&id)
    .bind(version_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    tracing::info!(document = %id, version = version_id, by = u.email, "publiceret");
    let paths = affected_paths(&s.db, &id, &doc.kind).await?;
    s.webhooks.send(Webhooks::body(Event::Publish, Some(&id), Some(&doc.kind), ty.as_deref(), paths, &u.email));
    Ok(Json(one(&s.db, &id).await?))
}

#[derive(Serialize, sqlx::FromRow, TS)]
#[ts(export)]
pub struct Version {
    #[ts(type = "number")]
    id: i64,
    name: String,
    published_at: DateTime<Utc>,
    published_by: String,
}

pub async fn versions(State(s): State<AppState>, Path(id): Path<String>) -> R<Json<Vec<Version>>> {
    let rows = sqlx::query_as::<_, Version>(
        "select id, name, published_at, published_by from versions where document_id = $1 order by id desc limit 50",
    )
    .bind(&id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

/// Lægger en tidligere version tilbage som kladde. Den skal publiceres igen for at gå live.
pub async fn restore(State(s): State<AppState>, Extension(u): Extension<User>, Path((id, version)): Path<(String, i64)>) -> R<Json<Doc>> {
    require_kind(&u, &one(&s.db, &id).await?.kind)?;
    let mut tx = s.db.begin().await?;
    let data: Option<Value> = sqlx::query_scalar(
        "update documents d
            set draft = v.data,
                name = case when d.kind = 'page' then d.name else v.name end,
                updated_at = now(),
                updated_by = $3
           from versions v
          where v.id = $2 and v.document_id = $1 and d.id = $1
      returning d.draft",
    )
    .bind(&id)
    .bind(version)
    .bind(&u.email)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(data) = data else { return Err(AppError::not_found("Versionen")) };
    refs::save(&mut tx, &id, &data).await?;
    tx.commit().await?;
    Ok(Json(one(&s.db, &id).await?))
}

#[derive(Serialize, sqlx::FromRow, TS)]
#[ts(export)]
pub struct UsedBy {
    id: String,
    #[ts(as = "Kind")]
    kind: String,
    name: String,
}

pub async fn used_by(State(s): State<AppState>, Path(id): Path<String>) -> R<Json<Vec<UsedBy>>> {
    let rows: Vec<UsedBy> = sqlx::query_as(
        "select d.id, d.kind, d.name from refs r join documents d on d.id = r.from_id
          where r.to_id = $1 order by d.kind, lower(d.name)",
    )
    .bind(&id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

/// Stier på sitet, der skal ryddes, når et dokument publiceres: siden selv, eller de
/// sider, der bruger datasourcen, schedulen eller access-reglen. Tom for globals.
async fn affected_paths(db: &sqlx::PgPool, id: &str, kind: &str) -> R<Vec<String>> {
    let ids: Vec<String> = match kind {
        "page" => vec![id.to_string()],
        "global" => vec![],
        _ => sqlx::query_scalar("select r.from_id from refs r join documents d on d.id = r.from_id where r.to_id = $1 and d.kind = 'page'")
            .bind(id)
            .fetch_all(db)
            .await?,
    };
    let paths = crate::site::all_paths(db).await?;
    let mut out: Vec<String> = ids.iter().filter_map(|i| paths.get(i).cloned()).collect();
    out.sort();
    Ok(out)
}
