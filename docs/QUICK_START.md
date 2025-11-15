# KalamDB Quick Start

Fast path: build, run, and issue your first SQL query.

## 1. Prerequisites

- Git
- Rust 1.90+
- C++ toolchain (build-essential / Xcode CLT / MSVC)

For full setup and troubleshooting, see `docs/DEVELOPMENT_SETUP.md`.

## 2. Clone and build

```bash
git clone https://github.com/jamals86/KalamDB.git
cd KalamDB/backend

cargo build --release --bin kalamdb-server
```

## 3. Run the server

```bash
cargo run --release --bin kalamdb-server
```

You should see logs indicating the server is listening on `http://127.0.0.1:8080`.

## 4. Healthcheck

```bash
curl http://127.0.0.1:8080/v1/api/healthcheck
```

Expected:

```json
{"status":"ok"}
```

## 5. Create namespace and table

```bash
curl -X POST http://127.0.0.1:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -d '{"sql": "CREATE NAMESPACE app;"}'

curl -X POST http://127.0.0.1:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -d '{"sql": "CREATE USER TABLE app.messages (id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(), content TEXT NOT NULL, created_at TIMESTAMP DEFAULT NOW()) FLUSH ROW_THRESHOLD 1000;"}'
```

## 6. Insert and query

```bash
curl -X POST http://127.0.0.1:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -d '{"sql": "INSERT INTO app.messages (content) VALUES (''Hello from KalamDB'');"}'

curl -X POST http://127.0.0.1:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -d '{"sql": "SELECT * FROM app.messages ORDER BY created_at DESC LIMIT 10;"}'
```

## 7. Next steps

- `docs/SQL.md` – more SQL examples
- `docs/API.md` – HTTP & WebSocket API
- `docs/cli.md` – `kalam` command-line client
