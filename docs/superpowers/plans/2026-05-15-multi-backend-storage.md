# Multi-Backend Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Abstract the storage layer behind a `StorageBackend` trait so Lithium supports file and S3 backends selected at runtime via `lithium.toml`.

**Architecture:** A `StorageBackend` trait with `store`, `delete`, and `accel_redirect_path` methods lives in `src/backend/mod.rs`. `FileBackend` and `S3Backend` implement it. Config selects the backend via a `BackendConfig` enum. `AppState` holds `Arc<dyn StorageBackend>`. The `Sweeper` migrates from std threads to tokio tasks so it can call `backend.delete()` asynchronously.

**Tech Stack:** Rust, axum 0.7, tokio 1.0, async-trait 0.1, bytes 1.0, aws-sdk-s3 1.0, aws-config 1.0, tempfile (dev)

---

### Task 1: Add dependencies to Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add new dependencies**

Replace the contents of `Cargo.toml` with:

```toml
[package]
name = "lithium"
version = "0.2.0"
authors = ["Navid <navid92@gmail.com>"]
edition = "2021"
description = "A modern cache-based file CDN written in Rust"
license = "MIT"

[dependencies]
tokio = { version = "1.0", features = ["full"] }
axum = "0.7"
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
tracing = "0.1"
tracing-subscriber = "0.3"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
thiserror = "1.0"
uuid = { version = "1.0", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
url = "2.4"
path-clean = "0.1"
toml = "0.8"
reqwest = { version = "0.11", features = ["json"] }
futures-util = "0.3"
async-trait = "0.1"
bytes = "1.0"
aws-sdk-s3 = "1.0"
aws-config = "1.0"

[dev-dependencies]
tempfile = "3.0"
```

- [ ] **Step 2: Verify build**

```bash
cd /Users/sotoon/personal/lithium && cargo build
```

Expected: compiles (may take a while fetching aws crates)

- [ ] **Step 3: Commit**

```bash
cd /Users/sotoon/personal/lithium && git add Cargo.toml Cargo.lock && git commit -m "chore: add async-trait, bytes, aws-sdk-s3 dependencies

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 2: Add S3 error variant and BackendConfig

**Files:**
- Modify: `src/error.rs`
- Modify: `src/config.rs`
- Modify: `lithium.toml`

- [ ] **Step 1: Add S3 error variant to src/error.rs**

Replace the full contents of `src/error.rs`:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LithiumError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("URL parsing error: {0}")]
    Url(#[from] url::ParseError),

    #[error("Configuration error: {0}")]
    Config(#[from] toml::de::Error),

    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("Path traversal detected: {path}")]
    PathTraversal { path: String },

    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Cache error: {message}")]
    Cache { message: String },

    #[error("Download error: {message}")]
    Download { message: String },

    #[error("Invalid path: {path}")]
    InvalidPath { path: String },

    #[error("S3 error: {message}")]
    S3 { message: String },
}

pub type Result<T> = std::result::Result<T, LithiumError>;
```

- [ ] **Step 2: Write failing test for BackendConfig deserialization**

Add to `src/config.rs` test block (we'll add it after updating the struct):

First, replace the full contents of `src/config.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BackendConfig {
    File {
        base_dir: PathBuf,
    },
    S3 {
        bucket: String,
        endpoint: String,
        region: String,
        accel_prefix: String,
    },
}

impl Default for BackendConfig {
    fn default() -> Self {
        BackendConfig::File {
            base_dir: PathBuf::from("/tmp/lithium-cache"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub cache: CacheConfig,
    pub base_url: String,
    pub backend: BackendConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub size_limit: usize,
    pub soft_limit_ratio: f64,
    pub sweep_interval_secs: u64,
    pub max_delete_per_iteration: usize,
    pub max_file_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 9999,
            },
            cache: CacheConfig {
                size_limit: 100_000_000,
                soft_limit_ratio: 0.85,
                sweep_interval_secs: 10,
                max_delete_per_iteration: 100,
                max_file_size: 10_000_000,
            },
            base_url: "https://divar.ir".to_string(),
            backend: BackendConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config = if let Ok(config_str) = std::fs::read_to_string("lithium.toml") {
            toml::from_str(&config_str)?
        } else {
            Self::default()
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.cache.size_limit == 0 {
            return Err(anyhow::anyhow!("Cache size limit must be greater than 0"));
        }
        if self.cache.soft_limit_ratio <= 0.0 || self.cache.soft_limit_ratio >= 1.0 {
            return Err(anyhow::anyhow!("Soft limit ratio must be between 0 and 1"));
        }
        if self.cache.sweep_interval_secs == 0 {
            return Err(anyhow::anyhow!("Sweep interval must be greater than 0"));
        }
        if self.cache.max_delete_per_iteration == 0 {
            return Err(anyhow::anyhow!("Max delete per iteration must be greater than 0"));
        }
        if self.cache.max_file_size == 0 {
            return Err(anyhow::anyhow!("Max file size must be greater than 0"));
        }
        if self.cache.max_file_size > self.cache.size_limit {
            return Err(anyhow::anyhow!("Max file size cannot be larger than cache size limit"));
        }
        if self.server.port == 0 {
            return Err(anyhow::anyhow!("Port must be greater than 0"));
        }
        if self.base_url.is_empty() {
            return Err(anyhow::anyhow!("Base URL cannot be empty"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9999);
        assert_eq!(config.cache.size_limit, 100_000_000);
        assert_eq!(config.base_url, "https://divar.ir");
        matches!(config.backend, BackendConfig::File { .. });
    }

    #[test]
    fn test_backend_config_file_deserialization() {
        let toml_str = r#"
            type = "file"
            base_dir = "/tmp/cache"
        "#;
        let backend: BackendConfig = toml::from_str(toml_str).unwrap();
        match backend {
            BackendConfig::File { base_dir } => {
                assert_eq!(base_dir, std::path::PathBuf::from("/tmp/cache"));
            }
            _ => panic!("Expected File backend"),
        }
    }

    #[test]
    fn test_backend_config_s3_deserialization() {
        let toml_str = r#"
            type = "s3"
            bucket = "my-bucket"
            endpoint = "https://s3.example.com"
            region = "us-east-1"
            accel_prefix = "/s3-internal"
        "#;
        let backend: BackendConfig = toml::from_str(toml_str).unwrap();
        match backend {
            BackendConfig::S3 { bucket, endpoint, region, accel_prefix } => {
                assert_eq!(bucket, "my-bucket");
                assert_eq!(endpoint, "https://s3.example.com");
                assert_eq!(region, "us-east-1");
                assert_eq!(accel_prefix, "/s3-internal");
            }
            _ => panic!("Expected S3 backend"),
        }
    }
}
```

- [ ] **Step 3: Update lithium.toml**

Replace the full contents of `lithium.toml`:

```toml
[server]
host = "0.0.0.0"
port = 9999

[cache]
size_limit = 100000000  # 100MB
soft_limit_ratio = 0.85
sweep_interval_secs = 10
max_delete_per_iteration = 100
max_file_size = 10000000  # 10MB per file

base_url = "https://divar.ir"

[backend]
type = "file"
base_dir = "/tmp/lithium-cache"
```

- [ ] **Step 4: Run tests**

```bash
cd /Users/sotoon/personal/lithium && cargo test
```

Note: `main.rs` will fail to compile because it still uses `config.base_dir`. That's expected — we'll fix it in Task 8. Run just the config tests:

```bash
cd /Users/sotoon/personal/lithium && cargo test --lib config
```

Expected: 3 config tests pass

- [ ] **Step 5: Commit**

```bash
cd /Users/sotoon/personal/lithium && git add src/error.rs src/config.rs lithium.toml && git commit -m "feat(config): add BackendConfig enum and S3 error variant

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 3: Create StorageBackend trait and factory

**Files:**
- Create: `src/backend/mod.rs`

- [ ] **Step 1: Create src/backend/mod.rs**

```rust
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;

use crate::config::BackendConfig;
use crate::error::Result;

pub mod file;
pub mod s3;

pub use file::FileBackend;
pub use s3::S3Backend;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Store content at the given logical path. Returns number of bytes stored.
    async fn store(&self, path: &str, data: Bytes) -> Result<usize>;

    /// Delete content at the given logical path. Called by Sweeper on eviction.
    async fn delete(&self, path: &str) -> Result<()>;

    /// Returns the nginx X-Accel-Redirect path for a cache hit on this backend.
    /// FileBackend returns "/files<path>", S3Backend returns "<accel_prefix><path>".
    fn accel_redirect_path(&self, path: &str) -> String;
}

/// Create a backend from config. Must be called inside a tokio runtime (for S3 credential loading).
pub async fn create_backend(config: &BackendConfig) -> Result<Arc<dyn StorageBackend>> {
    match config {
        BackendConfig::File { base_dir } => {
            Ok(Arc::new(FileBackend::new(base_dir.clone())))
        }
        BackendConfig::S3 { bucket, endpoint, region, accel_prefix } => {
            let backend = S3Backend::new(bucket, endpoint, region, accel_prefix).await?;
            Ok(Arc::new(backend))
        }
    }
}
```

- [ ] **Step 2: Add stub files so it compiles**

Create `src/backend/file.rs` with a stub (will be replaced in Task 4):

```rust
use async_trait::async_trait;
use bytes::Bytes;
use std::path::PathBuf;
use crate::error::Result;
use super::StorageBackend;

pub struct FileBackend {
    pub base_dir: PathBuf,
}

impl FileBackend {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

#[async_trait]
impl StorageBackend for FileBackend {
    async fn store(&self, _path: &str, _data: Bytes) -> Result<usize> {
        unimplemented!()
    }

    async fn delete(&self, _path: &str) -> Result<()> {
        unimplemented!()
    }

    fn accel_redirect_path(&self, path: &str) -> String {
        format!("/files{}", path)
    }
}
```

Create `src/backend/s3.rs` with a stub (will be replaced in Task 5):

```rust
use async_trait::async_trait;
use bytes::Bytes;
use crate::error::Result;
use super::StorageBackend;

pub struct S3Backend {
    pub bucket: String,
    pub accel_prefix: String,
}

impl S3Backend {
    pub async fn new(
        _bucket: &str,
        _endpoint: &str,
        _region: &str,
        _accel_prefix: &str,
    ) -> Result<Self> {
        unimplemented!()
    }
}

#[async_trait]
impl StorageBackend for S3Backend {
    async fn store(&self, _path: &str, _data: Bytes) -> Result<usize> {
        unimplemented!()
    }

    async fn delete(&self, _path: &str) -> Result<()> {
        unimplemented!()
    }

    fn accel_redirect_path(&self, path: &str) -> String {
        format!("{}{}", self.accel_prefix, path)
    }
}
```

- [ ] **Step 3: Declare backend module in src/main.rs**

Add `mod backend;` to the module declarations at the top of `src/main.rs` (after the existing `mod` lines):

```rust
mod cache_controller;
mod download;
mod config;
mod error;
mod backend;
```

Also add to the use statements:
```rust
use backend::StorageBackend;
use backend::create_backend;
```

- [ ] **Step 4: Build to check trait compiles**

```bash
cd /Users/sotoon/personal/lithium && cargo build 2>&1 | head -40
```

Expected: trait and stubs compile. `main.rs` still has errors about `config.base_dir` — that's fine for now.

- [ ] **Step 5: Write trait test in src/backend/mod.rs**

Add to `src/backend/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LithiumError;

    struct MockBackend;

    #[async_trait]
    impl StorageBackend for MockBackend {
        async fn store(&self, _path: &str, data: Bytes) -> Result<usize> {
            Ok(data.len())
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            Ok(())
        }

        fn accel_redirect_path(&self, path: &str) -> String {
            format!("/mock{}", path)
        }
    }

    #[tokio::test]
    async fn test_mock_backend_store() {
        let backend = MockBackend;
        let data = Bytes::from("hello world");
        let size = backend.store("/test/file.jpg", data).await.unwrap();
        assert_eq!(size, 11);
    }

    #[tokio::test]
    async fn test_mock_backend_accel_redirect() {
        let backend = MockBackend;
        assert_eq!(backend.accel_redirect_path("/foo/bar.jpg"), "/mock/foo/bar.jpg");
    }
}
```

- [ ] **Step 6: Run backend tests**

```bash
cd /Users/sotoon/personal/lithium && cargo test --lib backend 2>&1
```

Expected: 2 mock backend tests pass

- [ ] **Step 7: Commit**

```bash
cd /Users/sotoon/personal/lithium && git add src/backend/ src/main.rs && git commit -m "feat(backend): add StorageBackend trait and module structure

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 4: Implement FileBackend

**Files:**
- Modify: `src/backend/file.rs`

FileBackend stores files under `base_dir/path`. `store()` creates parent directories and writes bytes. `delete()` removes the file using `tokio::fs`. `accel_redirect_path()` returns `/files<path>` for nginx.

- [ ] **Step 1: Write failing tests**

Replace the full contents of `src/backend/file.rs`:

```rust
use async_trait::async_trait;
use bytes::Bytes;
use std::path::PathBuf;
use tracing::{info, error};

use crate::error::{LithiumError, Result};
use super::StorageBackend;

pub struct FileBackend {
    pub base_dir: PathBuf,
}

impl FileBackend {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn full_path(&self, path: &str) -> PathBuf {
        // path is like "/some/file.jpg" — strip leading slash before joining
        let stripped = path.trim_start_matches('/');
        self.base_dir.join(stripped)
    }
}

#[async_trait]
impl StorageBackend for FileBackend {
    async fn store(&self, path: &str, data: Bytes) -> Result<usize> {
        let full_path = self.full_path(path);

        if let Some(parent) = full_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let size = data.len();
        tokio::fs::write(&full_path, data).await?;
        info!("FileBackend: stored {} bytes at {}", size, full_path.display());
        Ok(size)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let full_path = self.full_path(path);
        tokio::fs::remove_file(&full_path).await.map_err(|e| {
            error!("FileBackend: failed to delete {}: {}", full_path.display(), e);
            LithiumError::Io(e)
        })?;
        info!("FileBackend: deleted {}", full_path.display());
        Ok(())
    }

    fn accel_redirect_path(&self, path: &str) -> String {
        format!("/files{}", path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_store_creates_file() {
        let dir = tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());
        let data = Bytes::from("hello lithium");

        let size = backend.store("/subdir/test.txt", data).await.unwrap();

        assert_eq!(size, 13);
        let stored = tokio::fs::read(dir.path().join("subdir/test.txt")).await.unwrap();
        assert_eq!(stored, b"hello lithium");
    }

    #[tokio::test]
    async fn test_store_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());

        backend.store("/a/b/c/file.bin", Bytes::from("x")).await.unwrap();

        assert!(dir.path().join("a/b/c/file.bin").exists());
    }

    #[tokio::test]
    async fn test_delete_removes_file() {
        let dir = tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());

        backend.store("/to_delete.txt", Bytes::from("data")).await.unwrap();
        assert!(dir.path().join("to_delete.txt").exists());

        backend.delete("/to_delete.txt").await.unwrap();
        assert!(!dir.path().join("to_delete.txt").exists());
    }

    #[tokio::test]
    async fn test_delete_missing_file_returns_error() {
        let dir = tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());

        let result = backend.delete("/nonexistent.txt").await;
        assert!(result.is_err());
        matches!(result.unwrap_err(), LithiumError::Io(_));
    }

    #[test]
    fn test_accel_redirect_path() {
        let backend = FileBackend::new(PathBuf::from("/tmp/cache"));
        assert_eq!(backend.accel_redirect_path("/images/cat.jpg"), "/files/images/cat.jpg");
    }
}
```

- [ ] **Step 2: Run tests (should now pass since implementation is complete)**

```bash
cd /Users/sotoon/personal/lithium && cargo test --lib backend::file
```

Expected: 5 FileBackend tests pass

- [ ] **Step 3: Commit**

```bash
cd /Users/sotoon/personal/lithium && git add src/backend/file.rs && git commit -m "feat(backend): implement FileBackend with store/delete/accel_redirect

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 5: Implement S3Backend

**Files:**
- Modify: `src/backend/s3.rs`

S3Backend uses `aws-sdk-s3`. Credentials come from the standard AWS SDK chain (env vars `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, or instance metadata — NOT from `lithium.toml`). `accel_redirect_path()` prepends the configured `accel_prefix` so nginx can proxy the S3 path.

- [ ] **Step 1: Write tests for accel_redirect_path (no network needed)**

Replace the full contents of `src/backend/s3.rs`:

```rust
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::Builder as S3ConfigBuilder;
use bytes::Bytes;
use tracing::{info, error};

use crate::error::{LithiumError, Result};
use super::StorageBackend;

pub struct S3Backend {
    client: Client,
    bucket: String,
    accel_prefix: String,
}

impl S3Backend {
    pub async fn new(
        bucket: &str,
        endpoint: &str,
        region: &str,
        accel_prefix: &str,
    ) -> Result<Self> {
        let sdk_config = aws_config::defaults(BehaviorVersion::latest())
            .region(aws_config::Region::new(region.to_string()))
            .endpoint_url(endpoint)
            .load()
            .await;

        let s3_config = S3ConfigBuilder::from(&sdk_config)
            .force_path_style(true) // required for MinIO
            .build();

        let client = Client::from_conf(s3_config);

        Ok(Self {
            client,
            bucket: bucket.to_string(),
            accel_prefix: accel_prefix.to_string(),
        })
    }
}

#[async_trait]
impl StorageBackend for S3Backend {
    async fn store(&self, path: &str, data: Bytes) -> Result<usize> {
        let key = path.trim_start_matches('/');
        let size = data.len();

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(data.into())
            .send()
            .await
            .map_err(|e| {
                error!("S3Backend: PutObject failed for {}: {}", key, e);
                LithiumError::S3 { message: e.to_string() }
            })?;

        info!("S3Backend: stored {} bytes at s3://{}/{}", size, self.bucket, key);
        Ok(size)
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let key = path.trim_start_matches('/');

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                error!("S3Backend: DeleteObject failed for {}: {}", key, e);
                LithiumError::S3 { message: e.to_string() }
            })?;

        info!("S3Backend: deleted s3://{}/{}", self.bucket, key);
        Ok(())
    }

    fn accel_redirect_path(&self, path: &str) -> String {
        format!("{}{}", self.accel_prefix, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // S3Backend::new requires live AWS/MinIO credentials.
    // Integration tests go here when a MinIO instance is available.
    // Unit-testable logic lives below.

    #[test]
    fn test_accel_redirect_path() {
        // Build a minimal S3Backend without calling new() (which needs AWS)
        let backend = S3Backend {
            client: {
                // Build a dummy client — we only test accel_redirect_path, no network calls
                let conf = aws_sdk_s3::Config::builder()
                    .region(aws_config::Region::new("us-east-1"))
                    .build();
                Client::from_conf(conf)
            },
            bucket: "test-bucket".to_string(),
            accel_prefix: "/s3-internal".to_string(),
        };

        assert_eq!(
            backend.accel_redirect_path("/images/cat.jpg"),
            "/s3-internal/images/cat.jpg"
        );
        assert_eq!(
            backend.accel_redirect_path("/"),
            "/s3-internal/"
        );
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cd /Users/sotoon/personal/lithium && cargo test --lib backend::s3
```

Expected: 1 test passes (`test_accel_redirect_path`)

- [ ] **Step 3: Commit**

```bash
cd /Users/sotoon/personal/lithium && git add src/backend/s3.rs && git commit -m "feat(backend): implement S3Backend using aws-sdk-s3

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 6: Update download.rs to use StorageBackend

**Files:**
- Modify: `src/download.rs`

Remove direct filesystem write logic. Accept `backend: &dyn StorageBackend`. Path traversal validation stays here (security layer, backend-agnostic). The `path` parameter is now a logical path (e.g., `/images/cat.jpg`), not a full filesystem path.

- [ ] **Step 1: Write failing test**

The existing `test_download_file_rejects_traversal` test will need a mock backend. Add a `MockBackend` in the test module and update the test. First update the full file:

Replace the contents of `src/download.rs`:

```rust
use async_trait::async_trait;
use bytes::Bytes;
use tracing::info;
use url::Url;

use crate::backend::StorageBackend;
use crate::error::{LithiumError, Result};

pub async fn download_file(
    client: &reqwest::Client,
    backend: &dyn StorageBackend,
    url: &str,
    path: &str,
) -> Result<usize> {
    // Validate URL scheme
    let parsed_url = Url::parse(url)?;
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return Err(LithiumError::InvalidPath {
            path: format!("Invalid URL scheme: {}", parsed_url.scheme()),
        });
    }

    // Reject paths containing parent directory components
    // This is backend-agnostic security: must happen before any backend call
    if std::path::Path::new(path)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(LithiumError::PathTraversal {
            path: path.to_string(),
        });
    }

    info!("Downloading {} → backend path {}", url, path);

    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        return Err(LithiumError::Download {
            message: format!("HTTP error: {}", response.status()),
        });
    }

    let bytes: Bytes = response.bytes().await?;
    let size = backend.store(path, bytes).await?;

    info!("Downloaded and stored {} bytes at {}", size, path);
    Ok(size)
}

// validate_path remains for any callers that need base-dir-scoped path validation
pub fn validate_path(path: &str, base_dir: &std::path::Path) -> Result<std::path::PathBuf> {
    let clean_path = std::path::PathBuf::from(path_clean::clean(path));

    if clean_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(LithiumError::PathTraversal {
            path: path.to_string(),
        });
    }

    let full_path = base_dir.join(clean_path);

    if !full_path.starts_with(base_dir) {
        return Err(LithiumError::PathTraversal {
            path: path.to_string(),
        });
    }

    Ok(full_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ---- validate_path tests (unchanged) ----

    #[test]
    fn test_validate_path_safe() {
        let base_dir = Path::new("/tmp/cache");
        let result = validate_path("safe/path/file.txt", base_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Path::new("/tmp/cache/safe/path/file.txt"));
    }

    #[test]
    fn test_validate_path_traversal() {
        let base_dir = Path::new("/tmp/cache");
        let result = validate_path("../../../etc/passwd", base_dir);
        assert!(result.is_err());
        match result.unwrap_err() {
            LithiumError::PathTraversal { path } => {
                assert_eq!(path, "../../../etc/passwd");
            }
            _ => panic!("Expected PathTraversal error"),
        }
    }

    #[test]
    fn test_validate_path_absolute() {
        let base_dir = Path::new("/tmp/cache");
        let result = validate_path("absolute/path", base_dir);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Path::new("/tmp/cache/absolute/path"));
    }

    // ---- download_file tests ----

    struct MockBackend;

    #[async_trait]
    impl StorageBackend for MockBackend {
        async fn store(&self, _path: &str, data: Bytes) -> Result<usize> {
            Ok(data.len())
        }
        async fn delete(&self, _path: &str) -> Result<()> {
            Ok(())
        }
        fn accel_redirect_path(&self, path: &str) -> String {
            format!("/mock{}", path)
        }
    }

    #[tokio::test]
    async fn test_download_file_rejects_traversal() {
        let client = reqwest::Client::new();
        let backend = MockBackend;
        let result = download_file(&client, &backend, "https://example.com/file", "/../etc/shadow").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            LithiumError::PathTraversal { path } => {
                assert_eq!(path, "/../etc/shadow");
            }
            e => panic!("Expected PathTraversal, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_download_file_rejects_invalid_scheme() {
        let client = reqwest::Client::new();
        let backend = MockBackend;
        let result = download_file(&client, &backend, "ftp://example.com/file", "/valid/path").await;
        assert!(result.is_err());
        matches!(result.unwrap_err(), LithiumError::InvalidPath { .. });
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cd /Users/sotoon/personal/lithium && cargo test --lib download
```

Expected: 5 tests pass (3 validate_path + 2 download_file)

- [ ] **Step 3: Commit**

```bash
cd /Users/sotoon/personal/lithium && git add src/download.rs && git commit -m "feat(download): use StorageBackend trait instead of direct fs write

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 7: Migrate Sweeper to tokio tasks

**Files:**
- Modify: `src/cache_controller.rs`

Replace std threads + std mpsc with tokio::spawn + tokio::sync::mpsc. The file_deleter task now calls `backend.delete()` asynchronously. The sweep scheduler task uses `tokio::time::sleep` instead of `std::thread::sleep`.

- [ ] **Step 1: Update imports in src/cache_controller.rs**

Replace the import block at the top of `src/cache_controller.rs`:

```rust
use crate::backend::StorageBackend;
use crate::error::{LithiumError, Result};
use std::sync::{Arc, RwLock, Mutex};
use std::collections::{HashMap, BTreeMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::PathBuf;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::task::JoinHandle;
use tracing::{info, error, debug};
```

(Remove: `std::thread::{JoinHandle, spawn}`, `std::sync::mpsc::{Sender, channel}`, `std::fs`)

- [ ] **Step 2: Update sweep() and sweep_once() to use UnboundedSender**

In `CacheController`, the `sweep` and `sweep_once` methods use `Sender<String>` from std mpsc. Change to `UnboundedSender<String>` from tokio:

Find and replace the `sweep` method signature:
```rust
fn sweep(&mut self, file_deleter: &UnboundedSender<String>) -> u64 {
```

Find and replace the `sweep_once` method signature:
```rust
fn sweep_once(&mut self, file_deleter: &UnboundedSender<String>) -> bool {
```

Inside `sweep_once`, the `file_deleter.send(url.clone())` call stays the same (tokio unbounded_channel send returns `Result<(), SendError>` too).

- [ ] **Step 3: Replace Sweeper struct and impl**

Find the `pub struct Sweeper` block and replace the entire `Sweeper` implementation (from `pub struct Sweeper` through the closing `}` of `impl Sweeper`):

```rust
pub struct Sweeper {
    sweeper_handle: JoinHandle<()>,
    file_deleter_handle: JoinHandle<()>,
}

impl Sweeper {
    pub fn new(
        cache_controller: Arc<RwLock<CacheController>>,
        backend: Arc<dyn StorageBackend>,
        stop: Arc<AtomicBool>,
    ) -> Self {
        let (tx, mut rx) = unbounded_channel::<String>();

        let stop_sweeper = stop.clone();
        let sweeper_handle = tokio::spawn(async move {
            info!("Sweeper task started");
            while !stop_sweeper.load(Ordering::Relaxed) {
                let delay_secs = match cache_controller.write() {
                    Ok(mut cache) => cache.sweep(&tx),
                    Err(e) => {
                        error!("Failed to acquire cache lock in sweeper: {}", e);
                        60
                    }
                };
                // Sleep in 100ms increments to check stop flag frequently
                let mut elapsed = 0u64;
                while elapsed < delay_secs * 1000 && !stop_sweeper.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    elapsed += 100;
                }
            }
            info!("Sweeper task stopped");
        });

        let file_deleter_handle = tokio::spawn(async move {
            info!("File deleter task started");
            while let Some(path) = rx.recv().await {
                if let Err(e) = backend.delete(&path).await {
                    error!("Failed to delete {}: {}", path, e);
                }
            }
            info!("File deleter task stopped");
        });

        Self {
            sweeper_handle,
            file_deleter_handle,
        }
    }

    pub async fn join(self) {
        let _ = self.sweeper_handle.await;
        let _ = self.file_deleter_handle.await;
    }
}
```

- [ ] **Step 4: Run cache_controller tests**

```bash
cd /Users/sotoon/personal/lithium && cargo test --lib cache_controller
```

Expected: all existing cache_controller tests pass

- [ ] **Step 5: Commit**

```bash
cd /Users/sotoon/personal/lithium && git add src/cache_controller.rs && git commit -m "feat(sweeper): migrate from std threads to tokio tasks for async backend.delete()

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 8: Wire backend into main.rs

**Files:**
- Modify: `src/main.rs`

Update `AppState` to include `backend: Arc<dyn StorageBackend>`. Update handler to use `backend.accel_redirect_path()`. Pass backend to `download_file` and `Sweeper`. Add S3 error to `IntoResponse`. Update `Sweeper::join()` call to `.await`.

- [ ] **Step 1: Replace src/main.rs completely**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Router,
    response::IntoResponse,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tower_http::trace::TraceLayer;
use std::sync::RwLock;

mod cache_controller;
mod download;
mod config;
mod error;
mod backend;

use cache_controller::*;
use download::*;
use config::*;
use error::*;
use backend::{StorageBackend, create_backend};

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
        let mut cache = state.cache_controller.write()
            .map_err(|_| LithiumError::Cache { message: "Failed to acquire cache lock".to_string() })?;
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
            return Ok((
                StatusCode::SERVICE_UNAVAILABLE,
                [("Retry-After", "1")],
            ).into_response());
        }
        HitMiss::Miss => {
            tracing::info!("Cache miss for {}", path);
        }
    }

    // Download and store via backend
    let download_url = format!("{}{}", state.config.base_url, path);

    match download_file(&state.client, state.backend.as_ref(), &download_url, &path).await {
        Ok(size) => {
            let mut cache = state.cache_controller.write()
                .map_err(|_| LithiumError::Cache { message: "Failed to acquire cache lock".to_string() })?;
            if let Err(e) = cache.download_done(&path, size) {
                tracing::error!("Failed to update cache: {}", e);
                cache.download_failed(&path);
                return Err(e);
            }
            cache.dump();
        }
        Err(e) => {
            tracing::error!("Download failed for {}: {}", path, e);
            let mut cache = state.cache_controller.write()
                .map_err(|_| LithiumError::Cache { message: "Failed to acquire cache lock".to_string() })?;
            cache.download_failed(&path);
            return Err(e);
        }
    }

    let xaccel = state.backend.accel_redirect_path(&path);
    Ok(xaccel_redirect(&xaccel))
}

fn xaccel_redirect(internal_url: &str) -> axum::response::Response {
    (StatusCode::OK, [("X-Accel-Redirect", internal_url.to_string())]).into_response()
}

impl IntoResponse for LithiumError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            LithiumError::Download { message } => (StatusCode::BAD_GATEWAY, message),
            LithiumError::Http(_) => (StatusCode::BAD_GATEWAY, "HTTP error".to_string()),
            LithiumError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "IO error".to_string()),
            LithiumError::S3 { message } => (StatusCode::BAD_GATEWAY, format!("S3 error: {}", message)),
            LithiumError::PathTraversal { path } => (StatusCode::BAD_REQUEST, format!("Path traversal detected: {}", path)),
            LithiumError::InvalidPath { path } => (StatusCode::BAD_REQUEST, format!("Invalid path: {}", path)),
            LithiumError::Cache { message } => (StatusCode::INTERNAL_SERVER_ERROR, message),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string()),
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
        config.cache.size_limit,
        config.cache.soft_limit_ratio,
        config.cache.sweep_interval_secs,
        config.cache.max_delete_per_iteration,
        config.cache.max_file_size,
    )));

    let stop = Arc::new(AtomicBool::new(false));
    let sweeper = Sweeper::new(cache_controller.clone(), backend.clone(), stop.clone());

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

    let listener = tokio::net::TcpListener::bind(
        format!("{}:{}", config.server.host, config.server.port)
    ).await?;
    tracing::info!("Server listening on {}:{}", config.server.host, config.server.port);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.ok();
            tracing::info!("Shutdown signal received");
            stop.store(true, Ordering::Relaxed);
        })
        .await?;

    sweeper.join().await;

    Ok(())
}
```

- [ ] **Step 2: Run full test suite**

```bash
cd /Users/sotoon/personal/lithium && cargo test
```

Expected: all tests pass

- [ ] **Step 3: Build release**

```bash
cd /Users/sotoon/personal/lithium && cargo build --release
```

Expected: clean compile, no errors

- [ ] **Step 4: Commit**

```bash
cd /Users/sotoon/personal/lithium && git add src/main.rs && git commit -m "feat: wire StorageBackend into AppState, handler, and Sweeper

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```
