# Lithium

A modern, secure cache-based file CDN written in Rust.

## Features

- **High Performance**: Built with async/await and modern Rust
- **Secure**: Path traversal protection and input validation
- **Configurable**: TOML-based configuration
- **Observable**: Comprehensive logging and metrics
- **Thread-Safe**: Proper synchronization and error handling

## Quick Start

1. **Install Rust** (if not already installed):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Clone and build**:
   ```bash
   git clone <repository-url>
   cd lithium
   cargo build --release
   ```

3. **Configure** (optional):

   Edit `lithium.toml` with your settings (defaults work out of the box).

4. **Run**:
   ```bash
   cargo run
   # or release build:
   cargo run --release
   ```

## Configuration

The application can be configured via `lithium.toml`:

```toml
[server]
host = "0.0.0.0"
port = 9999

[cache]
size_limit = 100000000  # 100MB
soft_limit_ratio = 0.85
sweep_interval_secs = 10
max_delete_per_iteration = 100

base_url = "https://example.com"
base_dir = "/tmp/lithium-cache"
```

## Usage

The server acts as a proxy cache. Requests to `http://localhost:9999/path/to/file` will:

1. Check if the file is already cached
2. If not, download it from `base_url + /path/to/file`
3. Cache it locally and serve it via X-Accel-Redirect

## Security

- Path traversal attacks are prevented
- Input validation on all paths
- Secure file handling
- No arbitrary code execution vulnerabilities

## Development

### Running

```bash
# Development (with auto-recompile on change)
cargo run

# Release build
cargo run --release

# With debug logging
RUST_LOG=debug cargo run

# Custom log level per module
RUST_LOG=lithium=debug,tower_http=info cargo run
```

### Testing

```bash
# Run all tests
cargo test

# Run a specific test by name
cargo test test_cache

# Run tests with output (don't suppress println/logs)
cargo test -- --nocapture

# Run tests in a specific module
cargo test cache_controller
```

### Manual testing

Once the server is running on port 9999, test with curl:

```bash
# Request a file (cache miss → downloads from base_url, returns X-Accel-Redirect)
curl -v http://localhost:9999/path/to/file

# Second request (cache hit)
curl -v http://localhost:9999/path/to/file

# While a file is downloading, a concurrent request returns 503 + Retry-After: 1
curl -v http://localhost:9999/large-file
```

### nginx integration

Lithium responds with `X-Accel-Redirect` — nginx must serve the actual files. Example nginx config:

```nginx
location / {
    proxy_pass http://127.0.0.1:9999;
}

location /files/ {
    internal;
    root /tmp/lithium-cache;
}
```

## License

MIT License - see LICENSE file for details.
