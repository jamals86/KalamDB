# Kalam CLI

Interactive command-line client for KalamDB - a real-time database with WebSocket subscriptions.

## Features

- 🎯 **Interactive SQL Execution** - Execute queries with instant results
- 📊 **Multiple Output Formats** - Table, JSON, and CSV output modes
- 🔄 **Live Query Subscriptions** - Real-time data updates via WebSocket
- 🎨 **Syntax Highlighting** - Beautiful colored SQL syntax
- 📝 **Command History** - Persistent history with arrow key navigation
- ⚡ **Auto-completion** - TAB completion for SQL keywords, tables, and columns
- 🔐 **Authentication** - JWT token and API key support
- 📁 **Batch Execution** - Run SQL scripts from files
- 🎭 **Progress Indicators** - Visual feedback for long-running queries

## Installation

### From Source

```bash
cd cli
cargo build --release
```

The binary will be available at `target/release/kalam-cli`.

### Using Cargo

```bash
cargo install --path kalam-cli
```

## Quick Start

### Connect to Server

```bash
# Connect to default localhost:3000
kalam-cli

# Connect to specific host
kalam-cli -h myserver.com

# Connect with authentication
kalam-cli --token YOUR_JWT_TOKEN
```

### Execute SQL

```sql
-- Create namespace
CREATE NAMESPACE app;

-- Create table
CREATE USER TABLE app.users (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    username TEXT NOT NULL,
    email TEXT,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Insert data
INSERT INTO app.users (username, email) 
VALUES ('alice', 'alice@example.com');

-- Query data
SELECT * FROM app.users;
```

### Live Subscriptions

```sql
-- Subscribe to changes
SUBSCRIBE TO app.messages WHERE user_id = 'user123';

-- Pause subscription
\pause

-- Resume subscription
\continue

-- Stop subscription (Ctrl+C)
```

## Usage

### Command-Line Options

```
kalam-cli [OPTIONS]

OPTIONS:
    -h, --host <HOST>          Server hostname (default: localhost)
    -p, --port <PORT>          Server port (default: 3000)
    -u, --user-id <USER_ID>    User ID for authentication
    --token <TOKEN>            JWT authentication token
    --apikey <APIKEY>          API key for authentication
    --json                     Output in JSON format
    --csv                      Output in CSV format
    --color                    Enable colored output (default: auto)
    --file <FILE>              Execute SQL from file
    --help                     Print help information
    --version                  Print version information
```

### Interactive Commands

Special commands starting with backslash (`\`):

| Command | Description |
|---------|-------------|
| `\help` | Show available commands |
| `\quit` or `\q` | Exit the CLI |
| `\connect HOST [PORT]` | Connect to different server |
| `\config` | Show current configuration |
| `\flush` | Execute manual flush |
| `\health` | Check server health |
| `\pause` | Pause active subscription |
| `\continue` | Resume paused subscription |
| `\refresh-tables` | Refresh table/column autocomplete cache |

### Output Formats

#### Table Format (Default)

```
┌─────────┬──────────┬─────────────────────┬────────────────────────┐
│ id      │ username │ email               │ created_at             │
├─────────┼──────────┼─────────────────────┼────────────────────────┤
│ 1234567 │ alice    │ alice@example.com   │ 2025-10-24 10:30:00    │
│ 1234568 │ bob      │ bob@example.com     │ 2025-10-24 10:31:00    │
└─────────┴──────────┴─────────────────────┴────────────────────────┘
(2 rows)
Took: 5.234 ms
```

#### JSON Format

```bash
kalam-cli --json
```

```json
{
  "status": "success",
  "results": [
    {
      "id": 1234567,
      "username": "alice",
      "email": "alice@example.com",
      "created_at": "2025-10-24T10:30:00Z"
    }
  ],
  "took_ms": 5.234
}
```

#### CSV Format

```bash
kalam-cli --csv
```

```csv
id,username,email,created_at
1234567,alice,alice@example.com,2025-10-24T10:30:00Z
1234568,bob,bob@example.com,2025-10-24T10:31:00Z
```

### Batch Execution

Execute SQL from a file:

```bash
kalam-cli --file setup.sql
```

Example `setup.sql`:

```sql
CREATE NAMESPACE prod;
CREATE USER TABLE prod.events (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    event_type TEXT NOT NULL,
    data TEXT,
    created_at TIMESTAMP DEFAULT NOW()
);
INSERT INTO prod.events (event_type, data) VALUES ('login', '{"user":"alice"}');
```

### Configuration File

The CLI reads configuration from `~/.kalam/config.toml`:

```toml
[connection]
host = "localhost"
port = 3000

[auth]
user_id = "default_user"
# token = "your_jwt_token"
# apikey = "your_api_key"

[output]
format = "table"  # table, json, or csv
color = true

[autocomplete]
refresh_on_startup = true
```

## Authentication

### Localhost Bypass

When connecting from localhost without credentials, the CLI automatically uses a default user:

```bash
kalam-cli  # Uses default user on localhost
```

### JWT Token

```bash
kalam-cli --token eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

### API Key

```bash
kalam-cli --apikey YOUR_API_KEY
```

### User ID (Localhost Only)

```bash
kalam-cli -u myuser
```

## Advanced Features

### Syntax Highlighting

The CLI provides beautiful SQL syntax highlighting with:
- **Blue bold** keywords (SELECT, INSERT, CREATE, etc.)
- **Magenta** data types (INT, TEXT, TIMESTAMP, etc.)
- **Green** string literals
- **Yellow** numeric literals

### Auto-completion

Press **TAB** to auto-complete:
- SQL keywords (SELECT, INSERT, UPDATE, etc.)
- Table names (loaded from `system.tables`)
- Column names (context-aware after table name)
- SQL types (INT, TEXT, TIMESTAMP, etc.)
- CLI commands (\help, \quit, etc.)

Refresh table/column cache:

```sql
\refresh-tables
```

### Command History

- **↑/↓** arrows: Navigate command history
- History stored in `~/.kalam/history`
- Persistent across sessions

### Progress Indicators

For queries taking longer than 200ms, the CLI shows a spinner with elapsed time:

```
⠋ Executing query... 1.2s
```

### Live Query Notifications

When subscribed, the CLI displays real-time notifications:

```
[2025-10-24 10:30:15] INSERT: {"id": 1234567, "content": "New message"}
[2025-10-24 10:30:20] UPDATE: {"id": 1234567, "content": "Updated message"}
[2025-10-24 10:30:25] DELETE: {"id": 1234567}
```

## Troubleshooting

### Connection Refused

```
Error: Failed to connect to localhost:3000
```

**Solution:** Ensure the KalamDB server is running:

```bash
cd backend
cargo run --bin kalamdb-server
```

### Authentication Failed

```
Error: Authentication failed (401 Unauthorized)
```

**Solution:** Provide valid credentials:

```bash
kalam-cli --token YOUR_JWT_TOKEN
```

### Table Not Found

```
Error: table 'app.users' not found
```

**Solution:** Create the namespace and table first:

```sql
CREATE NAMESPACE app;
CREATE USER TABLE app.users (...);
```

## Examples

### Create and Query Table

```sql
-- Create namespace
CREATE NAMESPACE myapp;

-- Create table with auto-incrementing ID
CREATE USER TABLE myapp.users (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    username TEXT NOT NULL,
    email TEXT,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Insert data
INSERT INTO myapp.users (username, email) VALUES 
    ('alice', 'alice@example.com'),
    ('bob', 'bob@example.com');

-- Query all users
SELECT * FROM myapp.users;

-- Query with filter
SELECT username, email FROM myapp.users WHERE username = 'alice';
```

### Live Subscription

```sql
-- Create messages table
CREATE USER TABLE chat.messages (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    user_id TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Subscribe to new messages
SUBSCRIBE TO chat.messages WHERE user_id = 'alice';

-- In another terminal, insert messages:
-- INSERT INTO chat.messages (user_id, content) VALUES ('alice', 'Hello!');
-- The first terminal will show real-time notifications
```

### Batch Processing

Create `import.sql`:

```sql
CREATE NAMESPACE analytics;
CREATE USER TABLE analytics.events (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    event_type TEXT NOT NULL,
    user_id TEXT,
    data TEXT,
    created_at TIMESTAMP DEFAULT NOW()
);

INSERT INTO analytics.events (event_type, user_id, data) VALUES
    ('page_view', 'user1', '{"page":"/home"}'),
    ('click', 'user1', '{"button":"signup"}'),
    ('conversion', 'user1', '{"amount":99.99}');
```

Execute:

```bash
kalam-cli --file import.sql
```

## Development

### Running Tests

```bash
cd cli
cargo test
```

### Building from Source

```bash
cd cli
cargo build --release
```

### Project Structure

```
cli/
├── kalam-link/          # Reusable library for KalamDB connectivity
│   ├── src/
│   │   ├── client.rs    # HTTP client
│   │   ├── auth.rs      # Authentication
│   │   └── models.rs    # Request/response models
│   └── tests/
├── kalam-cli/           # Interactive CLI application
│   ├── src/
│   │   ├── main.rs      # Entry point
│   │   ├── session.rs   # Session management
│   │   ├── formatter.rs # Output formatting
│   │   ├── completer.rs # Auto-completion
│   │   └── parser.rs    # Command parsing
│   └── tests/
└── Cargo.toml
```

## License

Same license as KalamDB main project.

## Contributing

Contributions welcome! Please see the main KalamDB repository for contribution guidelines.

## Related Documentation

- [KalamDB API Reference](../../docs/architecture/API_REFERENCE.md)
- [SQL Syntax Guide](../../docs/architecture/SQL_SYNTAX.md)
- [WebSocket Protocol](../../docs/architecture/WEBSOCKET_PROTOCOL.md)
- [Quick Start Guide](../../docs/quickstart/QUICK_START.md)
