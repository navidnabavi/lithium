# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --release       # production build
cargo run                   # run with lithium.toml config
cargo test                  # run all tests
cargo test cache            # run specific test by name filter
RUST_LOG=debug cargo run    # run with debug logging
```

## Architecture

Lithium is a proxy cache CDN: requests come in, server checks in-memory cache, fetches from `base_url` if miss, stores to `base_dir`, responds with `X-Accel-Redirect` header (nginx handles actual file serving).

**Request flow:**
1. `main.rs` handler receives `/*path`
2. `CacheController::access()` returns `Hit | Downloading | Miss`
3. On miss: `download_file()` fetches from upstream, stores to disk
4. `cache.download_done()` updates size tracking; `download_failed()` cleans up
5. Always responds with `X-Accel-Redirect: /files/<path>`

**Key modules:**
- `cache_controller.rs` — in-memory LRU-like cache using `BTreeMap<timestamp, url>` + `HashMap<url, TimeUrl>`. `Sweeper` runs background thread pair: one sweeps evictions, one deletes files via channel.
- `download.rs` — async reqwest download with path traversal validation. Creates parent dirs automatically.
- `config.rs` — TOML config loaded from `lithium.toml`; falls back to defaults if file missing.
- `error.rs` — `LithiumError` enum with `thiserror`; maps to HTTP status codes in `IntoResponse` impl.

**Concurrency model:** `AppState` holds `Arc<RwLock<CacheController>>`. Write lock acquired per request for cache mutations. `downloading` set inside `CacheController` is `Arc<Mutex<HashSet>>` — tracks in-flight downloads to return `Downloading` instead of duplicate miss.

**Cache eviction:** Soft limit triggers sweep (LRU order via BTreeMap timestamp keys). Hard limit check available but not currently enforced in handler.

## Configuration

`lithium.toml` at project root. All fields required when file exists — no partial overrides. `max_file_size` must be ≤ `size_limit`.
