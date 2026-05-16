# Lithium

A modern, secure cache-based file CDN written in Rust.

## Features

- **High Performance**: Built with async/await and modern Rust
- **Multi-Backend Storage**: File system or S3/MinIO, selected at runtime via config
- **Secure**: Path traversal protection and input validation
- **nginx offload**: Responds with `X-Accel-Redirect` — nginx serves bytes, Lithium stays lean
- **Configurable**: TOML-based configuration
- **Observable**: Comprehensive logging and metrics

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

### File backend (default)

```toml
base_url = "https://example.com"

[server]
host = "0.0.0.0"
port = 9999

[cache]
max_file_size = 10000000  # 10MB

[sweeper]
enabled = true
size_limit = 100000000     # 100MB
soft_limit_ratio = 0.85
sweep_interval_secs = 10
max_delete_per_iteration = 100

[backend]
type = "file"
base_dir = "/tmp/lithium-cache"
```

### S3/MinIO backend

```toml
base_url = "https://example.com"

[server]
host = "0.0.0.0"
port = 9999

[cache]
max_file_size = 10000000  # 10MB

[sweeper]
enabled = true
size_limit = 100000000
soft_limit_ratio = 0.85
sweep_interval_secs = 10
max_delete_per_iteration = 100

[backend]
type = "s3"
bucket = "my-bucket"
endpoint = "https://s3.us-east-1.amazonaws.com"
region = "us-east-1"
accel_prefix = "/s3-internal"
```

### Disable sweeper (unbounded cache)

```toml
[sweeper]
enabled = false
```

S3 credentials are read from environment variables — not stored in `lithium.toml`:

```bash
export AWS_ACCESS_KEY_ID=...
export AWS_SECRET_ACCESS_KEY=...
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

Lithium responds with `X-Accel-Redirect` — nginx serves the actual bytes. No 3xx redirects ever.

**File backend:**

```nginx
location / {
    proxy_pass http://127.0.0.1:9999;
}

location /files/ {
    internal;
    root /tmp/lithium-cache;
}
```

**S3/MinIO backend:**

```nginx
location / {
    proxy_pass http://127.0.0.1:9999;
}

location /s3-internal/ {
    internal;
    proxy_pass https://<bucket>.<endpoint>/;
}
```

## License

MIT License - see LICENSE file for details.
