use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::RwLock;
use tower_http::trace::TraceLayer;

mod backend;
mod cache_controller;
mod config;
mod download;
mod error;

use backend::{create_backend, StorageBackend};
use cache_controller::*;
use config::*;
use download::*;
use error::*;

#[derive(Clone)]
struct AppState {
    cache_controller: Arc<RwLock<CacheController>>,
    config: Config,
    client: reqwest::Client,
    backend: Arc<dyn StorageBackend>,
}

async fn handler(
    Path(path): Path<String>,
    State(state): State<AppState>,
) -> Result<axum::response::Response> {
    let path = format!("/{}", path);

    // Check cache
    let cache_result = {
        let mut cache = state
            .cache_controller
            .write()
            .map_err(|_| LithiumError::Cache {
                message: "Failed to acquire cache lock".to_string(),
            })?;
        cache.access(&path)
    };

    match cache_result {
        HitMiss::Hit => {
            tracing::info!("Cache hit for {}", path);
            let xaccel = state.backend.accel_redirect_path(&path);
            return Ok(xaccel_redirect(&xaccel));
        }
        HitMiss::Downloading => {
            tracing::info!("File still downloading for {}", path);
            return Ok((StatusCode::SERVICE_UNAVAILABLE, [("Retry-After", "1")]).into_response());
        }
        HitMiss::Miss => {
            tracing::info!("Cache miss for {}", path);
        }
    }

    // Download and store via backend
    let download_url = format!("{}{}", state.config.base_url, path);

    match download_file(&state.client, state.backend.as_ref(), &download_url, &path).await {
        Ok(size) => {
            let mut cache = state
                .cache_controller
                .write()
                .map_err(|_| LithiumError::Cache {
                    message: "Failed to acquire cache lock".to_string(),
                })?;
            if let Err(e) = cache.download_done(&path, size) {
                tracing::error!("Failed to update cache: {}", e);
                cache.download_failed(&path);
                return Err(e);
            }
            cache.dump();
        }
        Err(e) => {
            tracing::error!("Download failed for {}: {}", path, e);
            let mut cache = state
                .cache_controller
                .write()
                .map_err(|_| LithiumError::Cache {
                    message: "Failed to acquire cache lock".to_string(),
                })?;
            cache.download_failed(&path);
            return Err(e);
        }
    }

    let xaccel = state.backend.accel_redirect_path(&path);
    Ok(xaccel_redirect(&xaccel))
}

fn xaccel_redirect(internal_url: &str) -> axum::response::Response {
    (
        StatusCode::OK,
        [("X-Accel-Redirect", internal_url.to_string())],
    )
        .into_response()
}

impl IntoResponse for LithiumError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            LithiumError::Download { message } => (StatusCode::BAD_GATEWAY, message),
            LithiumError::Http(_) => (StatusCode::BAD_GATEWAY, "HTTP error".to_string()),
            LithiumError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "IO error".to_string()),
            LithiumError::S3 { message } => {
                (StatusCode::BAD_GATEWAY, format!("S3 error: {}", message))
            }
            LithiumError::PathTraversal { path } => (
                StatusCode::BAD_REQUEST,
                format!("Path traversal detected: {}", path),
            ),
            LithiumError::InvalidPath { path } => {
                (StatusCode::BAD_REQUEST, format!("Invalid path: {}", path))
            }
            LithiumError::Cache { message } => (StatusCode::INTERNAL_SERVER_ERROR, message),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            ),
        };
        (status, message).into_response()
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let config = Config::load()?;
    tracing::info!("Loaded configuration: {:?}", config);

    // Create storage backend from config
    let backend = create_backend(&config.backend).await?;

    let cache_controller = Arc::new(RwLock::new(CacheController::new(
        config.cache.max_file_size,
    )));

    let stop = Arc::new(AtomicBool::new(false));
    let sweeper = if config.sweeper.enabled {
        Some(Sweeper::new(
            cache_controller.clone(),
            backend.clone(),
            stop.clone(),
            config.sweeper.clone(),
        ))
    } else {
        tracing::info!("Sweeper disabled via config");
        None
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let state = AppState {
        cache_controller,
        config: config.clone(),
        client,
        backend,
    };

    let app = Router::new()
        .route("/*path", get(handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", config.server.host, config.server.port))
            .await?;
    tracing::info!(
        "Server listening on {}:{}",
        config.server.host,
        config.server.port
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("Shutdown signal received");
            stop.store(true, Ordering::Relaxed);
        })
        .await?;

    if let Some(s) = sweeper {
        s.join().await;
    }

    Ok(())
}
