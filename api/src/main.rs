mod defs;
mod documents;
mod error;
mod ids;
mod redirects;
mod refs;
mod seed;
mod services;
mod site;
mod tree;
mod validate;

use axum::{
    http::{header, HeaderValue},
    routing::{get, patch, post},
    Router,
};
use sqlx::postgres::PgPoolOptions;
use std::{path::PathBuf, time::Duration};
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub defs_dir: PathBuf,
    pub options: std::sync::Arc<services::OptionsCache>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://webmaster:webmaster@localhost:5433/webmaster".into());
    let db = connect(&url).await?;
    sqlx::migrate!("./migrations").run(&db).await?;

    if let Ok(seed_file) = std::env::var("SEED_FILE") {
        seed::run(&db, &seed_file).await?;
    }

    let defs_dir = PathBuf::from(std::env::var("DEFINITIONS_DIR").unwrap_or_else(|_| "../definitions".into()));
    let static_dir = PathBuf::from(std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".into()));
    for p in defs::load(&defs_dir).problems {
        tracing::warn!("definitions/{}: {}", p.file, p.message);
    }
    let state = AppState { db, defs_dir, options: std::sync::Arc::new(services::OptionsCache::new()) };

    let api = Router::new()
        // editor
        .route("/definitions", get(defs::get_all))
        .route("/options/:service/:list", get(services::get_options))
        .route("/tree", get(tree::list).post(tree::create))
        .route("/tree/:id", patch(tree::update).delete(tree::remove))
        .route("/tree/:id/move", post(tree::move_node))
        .route("/documents", get(documents::list).post(documents::create))
        .route("/documents/:id", get(documents::get).put(documents::save).delete(documents::remove))
        .route("/documents/:id/publish", post(documents::publish))
        .route("/documents/:id/versions", get(documents::versions))
        .route("/documents/:id/restore/:version", post(documents::restore))
        .route("/documents/:id/used-by", get(documents::used_by))
        .route("/documents/:id/issues", get(validate::get_issues))
        .route("/redirects", get(redirects::list).put(redirects::replace))
        // sitet (kun konfiguration – sitet resolver selv datasources, schedules og access)
        .route("/site/page", get(site::page))
        .route("/site/tree", get(site::tree))
        .route("/site/redirects", get(redirects::list))
        .with_state(state);

    let index = static_dir.join("index.html");
    // index.html må ikke caches, ellers får redaktørerne en gammel editor efter en opdatering.
    // Filerne i assets/ har et hash i navnet og kan caches for evigt.
    let editor = Router::new()
        .nest_service("/assets", ServeDir::new(static_dir.join("assets")))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ))
        .fallback_service(
            Router::new()
                .fallback_service(ServeDir::new(&static_dir).fallback(ServeFile::new(index)))
                .layer(SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))),
        );
    let app = Router::new()
        .nest("/api", api)
        .merge(editor)
        .layer(TraceLayer::new_for_http());

    let addr = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("webmaster lytter på {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn connect(url: &str) -> anyhow::Result<sqlx::PgPool> {
    let mut tries = 0;
    loop {
        match PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
        {
            Ok(pool) => return Ok(pool),
            Err(e) if tries < 30 => {
                tries += 1;
                tracing::warn!("venter på databasen: {e}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
}
