//! Brugere, login, sessioner og roller.
//!
//! - Adgangskoder gemmes som argon2-hash. Sessioner som sha256 af et tilfældigt token,
//!   der ligger i cookien `wm_session` (HttpOnly, SameSite=Lax).
//! - Alle ændringer (alt andet end GET og HEAD) kræver headeren `X-Webmaster: 1`.
//!   En formular eller et billede på et andet site kan ikke sætte den, og et
//!   `fetch` fra et andet site med den header kræver CORS, som API'et ikke tillader.
//! - Rollerne er ordnet: editor < publisher < admin.

use crate::{
    error::{AppError, R},
    ids, AppState,
};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Path, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use ts_rs::TS;

pub const COOKIE: &str = "wm_session";
pub const CSRF_HEADER: &str = "x-webmaster";
const SESSION_DAYS: i64 = 14;
pub const MIN_PASSWORD: usize = 10;
/// Antal forkerte forsøg pr. e-mail, før login spærres i `LOCKOUT`.
const MAX_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(15 * 60);

/* ---------- roller og brugere ---------- */

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Role {
    Editor,
    Publisher,
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Editor => "editor",
            Role::Publisher => "publisher",
            Role::Admin => "admin",
        }
    }
    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "editor" => Some(Role::Editor),
            "publisher" => Some(Role::Publisher),
            "admin" => Some(Role::Admin),
            _ => None,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Role::Editor => "redaktør",
            Role::Publisher => "udgiver",
            Role::Admin => "admin",
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: Role,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

type UserRow = (String, String, String, String, bool, DateTime<Utc>, Option<DateTime<Utc>>);
const USER_COLS: &str = "u.id, u.email, u.name, u.role, u.active, u.created_at, u.last_login_at";

fn user_of(r: UserRow) -> User {
    User { id: r.0, email: r.1, name: r.2, role: Role::parse(&r.3).unwrap_or(Role::Editor), active: r.4, created_at: r.5, last_login_at: r.6 }
}

/// 403, hvis brugeren ikke har mindst rollen `r`.
pub fn require(u: &User, r: Role) -> R<()> {
    if u.role >= r {
        Ok(())
    } else {
        Err(AppError(StatusCode::FORBIDDEN, format!("Det kræver rollen {} eller højere", r.label()), None))
    }
}

/* ---------- adgangskoder og tokens ---------- */

pub fn hash_password(pw: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default().hash_password(pw.as_bytes(), &salt).map_err(|e| anyhow::anyhow!("{e}"))?.to_string())
}

fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
}

/// Argon2 er bevidst langsom, så den køres uden for async-tråden.
async fn verify_blocking(pw: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || verify_password(&pw, &hash)).await.unwrap_or(false)
}

pub fn check_password(pw: &str) -> R<()> {
    if pw.chars().count() < MIN_PASSWORD {
        return Err(AppError::bad(format!("Adgangskoden skal have mindst {MIN_PASSWORD} tegn")));
    }
    Ok(())
}

fn new_token() -> String {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    hex::encode(b)
}

fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Sammenligner i konstant tid, så et token ikke kan gættes tegn for tegn.
pub fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE)
        .map(|(_, v)| v.to_string())
}

fn cookie(value: &str, max_age: i64) -> HeaderValue {
    let secure = std::env::var("COOKIE_SECURE").is_ok_and(|v| v == "true");
    let s = format!(
        "{COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{}",
        if secure { "; Secure" } else { "" }
    );
    HeaderValue::from_str(&s).expect("cookie er ASCII")
}

/* ---------- sessioner ---------- */

/// Brugeren bag sessionen i cookien, hvis den er gyldig og brugeren er aktiv.
pub async fn current_user(db: &sqlx::PgPool, headers: &HeaderMap) -> R<Option<User>> {
    let Some(token) = cookie_token(headers) else { return Ok(None) };
    let row: Option<UserRow> = sqlx::query_as(&format!(
        "select {USER_COLS} from sessions s join users u on u.id = s.user_id
          where s.token_hash = $1 and s.expires_at > now() and u.active"
    ))
    .bind(sha256_hex(&token))
    .fetch_optional(db)
    .await?;
    Ok(row.map(user_of))
}

fn unauthorized() -> Response {
    AppError(StatusCode::UNAUTHORIZED, "Du er ikke logget ind".into(), None).into_response()
}

/// Kræver `X-Webmaster` på alt andet end GET og HEAD.
pub async fn csrf(req: Request, next: Next) -> Response {
    if !matches!(*req.method(), Method::GET | Method::HEAD) && !req.headers().contains_key(CSRF_HEADER) {
        return AppError(StatusCode::FORBIDDEN, "Mangler headeren X-Webmaster".into(), None).into_response();
    }
    next.run(req).await
}

/// Kræver login og lægger `User` i requestens extensions.
pub async fn require_login(State(s): State<AppState>, mut req: Request, next: Next) -> Response {
    match current_user(&s.db, req.headers()).await {
        Ok(Some(u)) => {
            req.extensions_mut().insert(u);
            next.run(req).await
        }
        Ok(None) => unauthorized(),
        Err(e) => e.into_response(),
    }
}

/// Må requesten se kladder? Ja med `X-Preview-Token` = `SITE_PREVIEW_TOKEN`, eller med en session.
pub async fn preview_allowed(db: &sqlx::PgPool, headers: &HeaderMap) -> R<()> {
    let expected = std::env::var("SITE_PREVIEW_TOKEN").ok().filter(|t| !t.is_empty());
    let given = headers.get("x-preview-token").and_then(|v| v.to_str().ok());
    if let (Some(e), Some(g)) = (&expected, given) {
        if same(e, g) {
            return Ok(());
        }
    }
    if current_user(db, headers).await?.is_some() {
        return Ok(());
    }
    Err(AppError(StatusCode::UNAUTHORIZED, "Kladder kræver X-Preview-Token eller login".into(), None))
}

/* ---------- login ---------- */

fn failures() -> &'static Mutex<HashMap<String, (u32, Instant)>> {
    static F: OnceLock<Mutex<HashMap<String, (u32, Instant)>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(HashMap::new()))
}

fn locked(email: &str) -> bool {
    let mut f = failures().lock().unwrap();
    f.retain(|_, (_, t)| t.elapsed() < LOCKOUT);
    f.get(email).is_some_and(|(n, _)| *n >= MAX_FAILURES)
}

fn record_failure(email: &str) {
    let mut f = failures().lock().unwrap();
    let e = f.entry(email.to_string()).or_insert((0, Instant::now()));
    e.0 += 1;
    e.1 = Instant::now();
}

#[derive(Deserialize)]
pub struct Login {
    email: String,
    password: String,
}

/// En gyldig hash, så et login med en ukendt e-mail tager lige så lang tid som med en kendt.
fn dummy_hash() -> &'static str {
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| hash_password("ingen-bruger-har-denne").unwrap_or_default())
}

pub async fn login(State(s): State<AppState>, Json(b): Json<Login>) -> R<Response> {
    let email = b.email.trim().to_lowercase();
    if locked(&email) {
        return Err(AppError(StatusCode::TOO_MANY_REQUESTS, "For mange forkerte forsøg. Prøv igen om 15 minutter.".into(), None));
    }
    let row: Option<(String, String, bool)> =
        sqlx::query_as("select id, password_hash, active from users where lower(email) = $1")
            .bind(&email)
            .fetch_optional(&s.db)
            .await?;
    let hash = row.as_ref().map(|r| r.1.clone()).unwrap_or_else(|| dummy_hash().to_string());
    let ok = verify_blocking(b.password, hash).await;
    let Some((id, _, true)) = row.filter(|_| ok) else {
        record_failure(&email);
        tracing::warn!(email, "forkert login");
        return Err(AppError(StatusCode::UNAUTHORIZED, "Forkert e-mail eller adgangskode".into(), None));
    };
    failures().lock().unwrap().remove(&email);

    let token = new_token();
    sqlx::query("delete from sessions where expires_at < now()").execute(&s.db).await?;
    sqlx::query("insert into sessions (token_hash, user_id, expires_at) values ($1, $2, now() + make_interval(days => $3))")
        .bind(sha256_hex(&token))
        .bind(&id)
        .bind(SESSION_DAYS as i32)
        .execute(&s.db)
        .await?;
    sqlx::query("update users set last_login_at = now() where id = $1").bind(&id).execute(&s.db).await?;
    let user = get_user(&s.db, &id).await?;
    tracing::info!(email = user.email, "logget ind");
    let mut res = Json(user).into_response();
    res.headers_mut().insert(header::SET_COOKIE, cookie(&token, SESSION_DAYS * 86400));
    Ok(res)
}

pub async fn logout(State(s): State<AppState>, headers: HeaderMap) -> R<Response> {
    if let Some(t) = cookie_token(&headers) {
        sqlx::query("delete from sessions where token_hash = $1").bind(sha256_hex(&t)).execute(&s.db).await?;
    }
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut().insert(header::SET_COOKIE, cookie("", 0));
    Ok(res)
}

pub async fn me(Extension(u): Extension<User>) -> Json<User> {
    Json(u)
}

#[derive(Deserialize)]
pub struct ChangePassword {
    current: String,
    new: String,
}

/// Skift egen adgangskode. Alle andre sessioner logges ud.
pub async fn change_password(State(s): State<AppState>, Extension(u): Extension<User>, headers: HeaderMap, Json(b): Json<ChangePassword>) -> R<StatusCode> {
    let hash: String = sqlx::query_scalar("select password_hash from users where id = $1").bind(&u.id).fetch_one(&s.db).await?;
    if !verify_blocking(b.current, hash).await {
        return Err(AppError::bad("Den nuværende adgangskode er forkert"));
    }
    check_password(&b.new)?;
    sqlx::query("update users set password_hash = $2 where id = $1").bind(&u.id).bind(hash_password(&b.new)?).execute(&s.db).await?;
    let keep = cookie_token(&headers).map(|t| sha256_hex(&t)).unwrap_or_default();
    sqlx::query("delete from sessions where user_id = $1 and token_hash <> $2").bind(&u.id).bind(keep).execute(&s.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

/* ---------- brugeradministration (admin) ---------- */

async fn get_user(db: &sqlx::PgPool, id: &str) -> R<User> {
    let row: Option<UserRow> = sqlx::query_as(&format!("select {USER_COLS} from users u where u.id = $1")).bind(id).fetch_optional(db).await?;
    row.map(user_of).ok_or_else(|| AppError::not_found("Brugeren"))
}

pub async fn list_users(State(s): State<AppState>, Extension(u): Extension<User>) -> R<Json<Vec<User>>> {
    require(&u, Role::Admin)?;
    let rows: Vec<UserRow> = sqlx::query_as(&format!("select {USER_COLS} from users u order by lower(u.name)")).fetch_all(&s.db).await?;
    Ok(Json(rows.into_iter().map(user_of).collect()))
}

fn check_email(email: &str) -> R<String> {
    let e = email.trim().to_lowercase();
    let ok = e.split_once('@').is_some_and(|(a, b)| !a.is_empty() && b.contains('.') && !b.starts_with('.') && !e.contains(' '));
    if !ok {
        return Err(AppError::bad(format!("«{email}» er ikke en e-mailadresse")));
    }
    Ok(e)
}

#[derive(Deserialize)]
pub struct CreateUser {
    email: String,
    name: String,
    role: Role,
    password: String,
}

pub async fn create_user(State(s): State<AppState>, Extension(u): Extension<User>, Json(b): Json<CreateUser>) -> R<Json<User>> {
    require(&u, Role::Admin)?;
    let email = check_email(&b.email)?;
    if b.name.trim().is_empty() {
        return Err(AppError::bad("Navnet må ikke være tomt"));
    }
    check_password(&b.password)?;
    let id = ids::new("us");
    let res = sqlx::query("insert into users (id, email, name, role, password_hash) values ($1, $2, $3, $4, $5)")
        .bind(&id)
        .bind(&email)
        .bind(b.name.trim())
        .bind(b.role.as_str())
        .bind(hash_password(&b.password)?)
        .execute(&s.db)
        .await;
    if let Err(sqlx::Error::Database(e)) = &res {
        if e.is_unique_violation() {
            return Err(AppError::conflict(format!("Der er allerede en bruger med {email}")));
        }
    }
    res?;
    tracing::info!(by = u.email, email, role = b.role.as_str(), "bruger oprettet");
    Ok(Json(get_user(&s.db, &id).await?))
}

#[derive(Deserialize)]
pub struct UpdateUser {
    name: Option<String>,
    role: Option<Role>,
    active: Option<bool>,
    password: Option<String>,
}

pub async fn update_user(State(s): State<AppState>, Extension(u): Extension<User>, Path(id): Path<String>, Json(b): Json<UpdateUser>) -> R<Json<User>> {
    require(&u, Role::Admin)?;
    let target = get_user(&s.db, &id).await?;
    if id == u.id && (b.role.is_some_and(|r| r != u.role) || b.active == Some(false)) {
        return Err(AppError::bad("Du kan ikke ændre din egen rolle eller deaktivere dig selv"));
    }
    let mut tx = s.db.begin().await?;
    if let Some(name) = &b.name {
        if name.trim().is_empty() {
            return Err(AppError::bad("Navnet må ikke være tomt"));
        }
        sqlx::query("update users set name = $2 where id = $1").bind(&id).bind(name.trim()).execute(&mut *tx).await?;
    }
    if let Some(r) = b.role {
        sqlx::query("update users set role = $2 where id = $1").bind(&id).bind(r.as_str()).execute(&mut *tx).await?;
    }
    if let Some(a) = b.active {
        sqlx::query("update users set active = $2 where id = $1").bind(&id).bind(a).execute(&mut *tx).await?;
    }
    if let Some(pw) = &b.password {
        check_password(pw)?;
        sqlx::query("update users set password_hash = $2 where id = $1").bind(&id).bind(hash_password(pw)?).execute(&mut *tx).await?;
    }
    // Ny adgangskode eller deaktiveret: log brugeren ud overalt.
    if b.password.is_some() || b.active == Some(false) {
        sqlx::query("delete from sessions where user_id = $1").bind(&id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    tracing::info!(by = u.email, email = target.email, "bruger ændret");
    Ok(Json(get_user(&s.db, &id).await?))
}

/// Opretter den første admin fra `ADMIN_EMAIL` og `ADMIN_PASSWORD`, hvis der ingen brugere er.
pub async fn bootstrap(db: &sqlx::PgPool) -> anyhow::Result<()> {
    let any: bool = sqlx::query_scalar("select exists(select 1 from users)").fetch_one(db).await?;
    if any {
        return Ok(());
    }
    let (Ok(email), Ok(pw)) = (std::env::var("ADMIN_EMAIL"), std::env::var("ADMIN_PASSWORD")) else {
        tracing::warn!("der er ingen brugere. Sæt ADMIN_EMAIL og ADMIN_PASSWORD for at oprette den første admin");
        return Ok(());
    };
    let email = check_email(&email).map_err(|e| anyhow::anyhow!(e.1))?;
    check_password(&pw).map_err(|e| anyhow::anyhow!(e.1))?;
    sqlx::query("insert into users (id, email, name, role, password_hash) values ($1, $2, 'Admin', 'admin', $3)")
        .bind(ids::new("us"))
        .bind(&email)
        .bind(hash_password(&pw)?)
        .execute(db)
        .await?;
    tracing::info!(email, "første admin oprettet");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_and_tokens() {
        let h = hash_password("korrekt-hest").unwrap();
        assert!(verify_password("korrekt-hest", &h));
        assert!(!verify_password("forkert-hest", &h));
        assert!(!verify_password("x", "ikke en hash"));
        assert_ne!(new_token(), new_token());
        assert_eq!(new_token().len(), 64);
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
        assert!(check_password("kort").is_err() && check_password("lang-nok-1").is_ok());
    }

    #[test]
    fn roles_and_cookies() {
        assert!(Role::Admin > Role::Publisher && Role::Publisher > Role::Editor);
        let u = User { id: "us_1".into(), email: "a@b.dk".into(), name: "A".into(), role: Role::Publisher, active: true, created_at: Utc::now(), last_login_at: None };
        assert!(require(&u, Role::Editor).is_ok() && require(&u, Role::Publisher).is_ok());
        let e = require(&u, Role::Admin).unwrap_err();
        assert_eq!(e.0, StatusCode::FORBIDDEN);
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_static("a=1; wm_session=tok; b=2"));
        assert_eq!(cookie_token(&h).as_deref(), Some("tok"));
        assert!(check_email(" Navn@Firma.DK ").unwrap() == "navn@firma.dk");
        assert!(check_email("ikke-en-mail").is_err() && check_email("a@b").is_err());
    }

    #[test]
    fn lockout_after_failures() {
        let e = "laas@test.dk";
        for _ in 0..MAX_FAILURES {
            assert!(!locked(e));
            record_failure(e);
        }
        assert!(locked(e));
    }
}
