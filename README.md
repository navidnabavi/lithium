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
   ```bash
   cp lithium.toml.example lithium.toml
   # Edit lithium.toml with your settings
   ```

4. **Run**:
   ```bash
   cargo run
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

Run tests:
```bash
cargo test
```

Run with debug logging:
```bash
RUST_LOG=debug cargo run
```

## License

MIT License - see LICENSE file for details.
