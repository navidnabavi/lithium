# Multi-Backend Storage Design

## Goal

Abstract the storage layer so Lithium supports multiple backends (file, S3/MinIO) selected at runtime via config. Both backends offload actual file serving to nginx via `X-Accel-Redirect`. No 3xx redirects.

---

## Architecture

Two concepts are introduced as separate extensibility axes:

- **StorageBackend** — how data is stored, retrieved (for deletion), and how the nginx redirect path is constructed
- **Sweeper** (future) — eviction strategy; currently LRU, may vary per backend later

This spec covers **StorageBackend only**.

---

## Module Structure

```
src/
  backend/
    mod.rs      — StorageBackend trait + create_backend() factory
    file.rs     — FileBackend (current file logic extracted here)
    s3.rs       — S3Backend (new)
  cache_controller.rs  — Sweeper migrated from std threads to tokio tasks
  config.rs            — BackendConfig enum added
  download.rs          — store() called via backend instead of direct fs write
  main.rs              — wire backend from config into AppState
  error.rs             — add S3 error variant
```

---

## StorageBackend Trait

```rust
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store content at path. Returns bytes written.
    async fn store(&self, path: &str, data: bytes::Bytes) -> Result<usize>;

    /// Delete content at path. Called by Sweeper on eviction.
    async fn delete(&self, path: &str) -> Result<()>;

    /// Returns the nginx X-Accel-Redirect path for this backend.
    /// File: "/files/path/to/file"
    /// S3:   "/s3-internal/path/to/file"
    fn accel_redirect_path(&self, path: &str) -> String;
}
```

Uses `async_trait` crate (already an established pattern in the Rust async ecosystem).
`bytes::Bytes` added to `Cargo.toml` dependencies.

---

## Backends

### FileBackend (`src/backend/file.rs`)

Extracts current filesystem logic from `download.rs` and `Sweeper`.

- `store()`: creates parent dirs, writes `Bytes` to `base_dir/path`
- `delete()`: calls `tokio::fs::remove_file(base_dir/path)`
- `accel_redirect_path()`: returns `format!("/files{}", path)`

Config fields: `base_dir: PathBuf`

### S3Backend (`src/backend/s3.rs`)

Uses `aws-sdk-s3` crate.

- `store()`: `PutObject` to bucket at `path` key
- `delete()`: `DeleteObject` from bucket at `path` key
- `accel_redirect_path()`: returns `format!("{}{}", accel_prefix, path)` where `accel_prefix` is configured (e.g. `/s3-internal`)

nginx config for S3 backend (user responsibility, documented in README):
```nginx
location /s3-internal/ {
    internal;
    proxy_pass https://<bucket>.<endpoint>/;
}
```

Config fields: `bucket: String`, `endpoint: String`, `region: String`, `accel_prefix: String`

Credentials: via environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`) — standard AWS SDK credential chain. Not in `lithium.toml`.

---

## Config

### TOML format

File backend (default):
```toml
[backend]
type = "file"
base_dir = "/tmp/lithium-cache"
```

S3 backend:
```toml
[backend]
type = "s3"
bucket = "my-bucket"
endpoint = "https://s3.us-east-1.amazonaws.com"
region = "us-east-1"
accel_prefix = "/s3-internal"
```

### Rust types

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BackendConfig {
    File { base_dir: PathBuf },
    S3 {
        bucket: String,
        endpoint: String,
        region: String,
        accel_prefix: String,
    },
}
```

`Config` struct gains `backend: BackendConfig` field, replacing the top-level `base_dir`.

---

## Factory

```rust
// src/backend/mod.rs
pub async fn create_backend(config: &BackendConfig) -> Result<Arc<dyn StorageBackend>> {
    match config {
        BackendConfig::File { base_dir } => Ok(Arc::new(FileBackend::new(base_dir.clone()))),
        BackendConfig::S3 { bucket, endpoint, region, accel_prefix } => {
            // aws-config loads credentials async (env vars, instance metadata, etc.)
            let backend = S3Backend::new(bucket, endpoint, region, accel_prefix).await?;
            Ok(Arc::new(backend))
        }
    }
}
```

---

## AppState Change

```rust
#[derive(Clone)]
struct AppState {
    cache_controller: Arc<RwLock<CacheController>>,
    config: Config,
    client: reqwest::Client,
    backend: Arc<dyn StorageBackend>,  // replaces implicit file logic
}
```

---

## download.rs Change

`download_file` signature changes to accept backend:

```rust
pub async fn download_file(
    client: &reqwest::Client,
    backend: &dyn StorageBackend,
    url: &str,
    path: &str,
) -> Result<usize>
```

Removes direct `fs::File::create` / `write_all` logic. Calls `backend.store(path, bytes).await` instead.
Path traversal validation stays in `download_file` (backend-agnostic security layer).

---

## Sweeper Change

Current: two std threads + `mpsc::channel::<String>`.
New: two tokio tasks + `tokio::sync::mpsc::unbounded_channel::<String>`.

The file_deleter task becomes:
```rust
tokio::spawn(async move {
    while let Some(path) = rx.recv().await {
        if let Err(e) = backend.delete(&path).await {
            error!("Failed to delete {}: {}", path, e);
        }
    }
});
```

`Sweeper::new()` becomes `async fn new(...)` called from `main()` after tokio runtime is active.
The `Arc<AtomicBool>` stop flag and 100ms sleep loop remain for the sweep scheduler task.

---

## Error Handling

Add to `LithiumError`:
```rust
#[error("S3 error: {message}")]
S3 { message: String },
```

S3 SDK errors map to `LithiumError::S3` in the backend impl.

---

## New Dependencies (`Cargo.toml`)

```toml
async-trait = "0.1"
bytes = "1.0"
aws-sdk-s3 = "1.0"
aws-config = "1.0"
```

---

## Testing

- `FileBackend`: unit tests write/read/delete to a `tempdir`
- `S3Backend`: unit tests use `mockall` or a local MinIO instance (integration test, feature-gated)
- `StorageBackend` trait: a `MockBackend` in tests implements the trait for handler tests
- Existing `cache_controller` tests: unaffected (no backend dependency)

---

## What Does NOT Change

- `CacheController` in-memory metadata logic (LRU, timestamps, size accounting)
- `X-Accel-Redirect` serving contract with nginx
- Path traversal validation (stays in `download.rs`, before backend is called)
- Graceful shutdown logic (`AtomicBool` stop flag)
