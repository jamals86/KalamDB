# KalamDB Backend

Rust-based messaging server with RocksDB storage and REST API.

## 🚀 Quick Start

### Prerequisites

**First time setting up?** See our comprehensive setup guides:

- **[📘 Development Setup Guide](../docs/DEVELOPMENT_SETUP.md)** - Complete installation for Windows/macOS/Linux
- **[🚀 Quick Start](../docs/QUICK_START.md)** - Get running in 10 minutes

**Requirements:**
- Rust 1.75 or later
- LLVM/Clang (required for RocksDB compilation)
- C++ Compiler (Visual Studio Build Tools on Windows, Xcode CLI on macOS, build-essential on Linux)

### Build

```bash
cd backend
cargo build
```

**Note**: First build takes 10-20 minutes as it compiles RocksDB, Arrow, and Parquet from source.

### Configure

Copy the example configuration and customize it:

```bash
cp config.example.toml config.toml
# Edit config.toml with your settings
```

### Run

```bash
cargo run
```

The server will start on `http://127.0.0.1:8080` by default.

## Development

### Project Structure

```
backend/
├── Cargo.toml                    # Workspace configuration
├── config.example.toml           # Example configuration
├── crates/
│   ├── kalamdb-core/            # Core storage library
│   ├── kalamdb-api/             # REST API library
│   └── kalamdb-server/          # Main server binary
└── tests/                        # Integration tests
```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_message_storage

# Run integration tests only
cargo test --test '*'

# Run with output
cargo test -- --nocapture
```

### Code Quality

```bash
# Check for errors
cargo check

# Run clippy (linter)
cargo clippy

# Format code
cargo fmt

# Build optimized release
cargo build --release
```

## Configuration

See `config.example.toml` for all available configuration options:

- **Server**: Host, port, worker threads
- **Storage**: RocksDB path, WAL settings, compression
- **Limits**: Message size, query limits
- **Logging**: Log level, file path, format
- **Performance**: Timeouts, connection limits

## API Endpoints

### POST /api/v1/messages
Insert a new message

**Request:**
```json
{
  "conversation_id": "conv_123",
  "from": "user_alice",
  "content": "Hello, world!",
  "metadata": {"role": "user"}
}
```

**Response:**
```json
{
  "msg_id": 1234567890,
  "status": "stored"
}
```

### POST /api/v1/query
Query messages

**Request:**
```json
{
  "conversation_id": "conv_123",
  "limit": 50
}
```

**Response:**
```json
{
  "messages": [...],
  "count": 50,
  "has_more": false
}
```

### GET /health
Health check endpoint

**Response:**
```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

## Logging

Logs are written to the path specified in `config.toml` (default: `./logs/app.log`).

Log levels: `error`, `warn`, `info`, `debug`, `trace`

To change log level:
```toml
[logging]
level = "debug"
```

## Architecture

- **kalamdb-core**: Core storage engine with RocksDB wrapper, message models, and ID generation
- **kalamdb-api**: REST API handlers and routes using Actix-web
- **kalamdb-server**: Main binary that ties everything together

## Current Status

**Phase 1 Complete**: Project structure and dependencies set up ✅

**Phase 2 Next**: Implement foundational components (config, logging, models, storage)

See `specs/001-build-a-rust/tasks.md` for the complete task list.

## License

MIT OR Apache-2.0
