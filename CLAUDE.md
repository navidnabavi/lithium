# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --release                      # production build
cargo run                                  # run with lithium.toml config
cargo test                                 # run all tests
cargo test cache                           # run specific test by name filter
RUST_LOG=debug cargo run                   # run with debug logging
cargo clippy --all-targets -- -D warnings  # lint (must use --all-targets; tests use methods missed otherwise)
```

## Architecture

Lithium is a proxy cache CDN: requests come in, server checks in-memory cache, fetches from `base_url` if miss, stores via the configured backend, responds with `X-Accel-Redirect` header (nginx handles actual file serving — never 3xx redirects).

**Request flow:**
1. `main.rs` handler receives `/*path`
2. `CacheController::access()` returns `Hit | Downloading | Miss`
3. On miss: `download_file()` fetches from upstream, calls `backend.store()`
4. `cache.download_done()` updates size tracking; `download_failed()` cleans up
5. Always responds with `X-Accel-Redirect: <backend.accel_redirect_path(path)>`

**Key modules:**
- `backend/mod.rs` — `StorageBackend` trait (`store`, `delete`, `accel_redirect_path`) + `create_backend()` async factory. `AppState` holds `Arc<dyn StorageBackend>`.
- `backend/file.rs` — `FileBackend`: writes to `base_dir`, serves via `/files<path>`.
- `backend/s3.rs` — `S3Backend`: PutObject/DeleteObject, serves via configurable `accel_prefix` (e.g. `/s3-internal`). Credentials via env vars (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`).
- `cache_controller.rs` — in-memory LRU-like cache using `BTreeMap<(u64, String), ()>` composite key + `HashMap<url, TimeUrl>`. `Sweeper` runs two tokio tasks: one sweeps evictions, one calls `backend.delete()` via unbounded channel.
- `download.rs` — async reqwest download with path traversal validation (`path_clean` + `ParentDir` check). Backend-agnostic: calls `backend.store()`.
- `config.rs` — TOML config loaded from `lithium.toml`; falls back to defaults if file missing. `BackendConfig` enum selects backend at runtime via `[backend] type = "file"|"s3"`.
- `error.rs` — `LithiumError` enum with `thiserror`; maps to HTTP status codes in `IntoResponse` impl.

**Concurrency model:** `AppState` holds `Arc<RwLock<CacheController>>`. Write lock acquired per request for cache mutations. `downloading` set inside `CacheController` is `Arc<Mutex<HashSet>>` — tracks in-flight downloads to return `Downloading` instead of duplicate miss. `Sweeper` uses `tokio::sync::mpsc::unbounded_channel` (not std) so `backend.delete()` can be awaited.

**Cache eviction:** Soft limit triggers sweep (LRU order via BTreeMap composite timestamp+url keys). Evicted paths sent to file-deleter task via channel.

## Configuration

`lithium.toml` at project root. `[sweeper]` fields have defaults and are optional when `enabled = false`. `max_file_size` must be ≤ `sweeper.size_limit` when sweeper enabled. For S3, `accel_prefix` must be non-empty.

Top-level `base_url` is required. `[cache]` only holds `max_file_size` — all eviction config lives in `[sweeper]`.
