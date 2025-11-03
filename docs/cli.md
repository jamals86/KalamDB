# KalamDB CLI - Command Reference

The Kalam CLI is an interactive terminal client for KalamDB, providing a rich SQL interface with real-time subscriptions, auto-completion, and credential management.

## Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Command-Line Options](#command-line-options)
- [Interactive Commands](#interactive-commands)
- [SQL Statements](#sql-statements)
- [Credential Management](#credential-management)
- [Configuration](#configuration)
- [Examples](#examples)
- [Smoke Tests](#smoke-tests)
- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Tips & Tricks](#tips--tricks)

---

## Installation

```bash
# Build from source
cd cli
cargo build --release

# The binary will be at: target/release/kalam
```

---

## Quick Start

```bash
# Connect to local server
kalam

# Connect to specific server
kalam --url http://localhost:8080

# Connect with authentication
kalam --url http://localhost:8080 --username alice --password secret123

# Execute SQL file and exit
kalam --file queries.sql

# Execute single command and exit
kalam --command "SELECT * FROM users"
```

---

## Command-Line Options

### Connection Options

| Option | Short | Description | Example |
|--------|-------|-------------|---------|
| `--url <URL>` | `-u` | Server URL | `--url http://localhost:8080` |
| `--host <HOST>` | `-H` | Host address | `--host localhost` |
| `--port <PORT>` | `-p` | Port number (default: 3000) | `--port 8080` |
| `--instance <NAME>` | | Database instance name (default: local) | `--instance production` |

### Authentication Options

| Option | Description | Example |
|--------|-------------|---------|
| `--username <USER>` | HTTP Basic Auth username | `--username alice` |
| `--password <PASS>` | HTTP Basic Auth password | `--password secret123` |
| `--token <TOKEN>` | JWT authentication token | `--token eyJhbGc...` |

### Output Options

| Option | Short | Description | Example |
|--------|-------|-------------|---------|
| `--format <FORMAT>` | | Output format: table, json, csv | `--format json` |
| `--json` | | Enable JSON output | `--json` |
| `--csv` | | Enable CSV output | `--csv` |
| `--no-color` | | Disable colored output | `--no-color` |

### Execution Options

| Option | Short | Description | Example |
|--------|-------|-------------|---------|
| `--file <PATH>` | `-f` | Execute SQL from file and exit | `--file queries.sql` |
| `--command <SQL>` | `-c` | Execute SQL command and exit | `--command "SELECT 1"` |
| `--verbose` | `-v` | Enable verbose logging | `--verbose` |

### Configuration Options

| Option | Description | Default | Example |
|--------|-------------|---------|---------|
| `--config <PATH>` | Configuration file path | `~/.kalam/config.toml` | `--config /etc/kalam.toml` |

### Credential Management Options

| Option | Description | Example |
|--------|-------------|---------|
| `--list-instances` | List all stored credential instances | `--list-instances` |
| `--show-credentials` | Show stored credentials for instance | `--show-credentials --instance prod` |
| `--update-credentials` | Update/store credentials for instance | `--update-credentials --instance local` |
| `--delete-credentials` | Delete stored credentials for instance | `--delete-credentials --instance test` |

---

## Interactive Commands

Once connected, you can use both SQL statements and meta-commands (prefixed with `\`).

### Meta-Commands

#### General Commands

| Command | Alias | Description | Example |
|---------|-------|-------------|---------|
| `\quit` | `\q` | Exit the CLI | `\quit` |
| `\help` | `\?` | Show help message | `\help` |

#### Connection Commands

| Command | Description | Example |
|---------|-------------|---------|
| `\connect <url>` | Connect to a different server | `\connect http://localhost:8080` |
| `\health` | Check server health | `\health` |
| `\config` | Show current configuration | `\config` |

#### Table Management

| Command | Alias | Description | Example |
|---------|-------|-------------|---------|
| `\dt` | `\tables` | List all tables | `\dt` |
| `\d <table>` | `\describe` | Describe table schema | `\d users` |
| `\refresh-tables` | `\refresh` | Refresh table names for autocomplete | `\refresh-tables` |

#### Output Formatting

| Command | Description | Example |
|---------|-------------|---------|
| `\format <type>` | Set output format (table, json, csv) | `\format json` |

#### Data Management

| Command | Alias | Description | Example |
|---------|-------|-------------|---------|
| `\flush` | | Flush all data to disk | `\flush` |
| `\stats` | `\metrics` | Show cache statistics and system metrics | `\stats` |

#### Streaming/Subscriptions

| Command | Alias | Description | Example |
|---------|-------|-------------|---------|
| `\subscribe <query>` | `\watch` | Start WebSocket subscription for real-time updates | `\subscribe SELECT * FROM messages` |
| `\unsubscribe` | `\unwatch` | Cancel active subscription | `\unsubscribe` |
| `\pause` | | Pause ingestion | `\pause` |
| `\continue` | | Resume ingestion | `\continue` |

---

## SQL Statements

The CLI supports all standard SQL statements:

### Data Query Language (DQL)

```sql
-- Simple select
SELECT * FROM users;

-- With WHERE clause
SELECT name, age FROM users WHERE age > 18;

-- With ORDER BY and LIMIT
SELECT * FROM users ORDER BY created_at DESC LIMIT 10;

-- Joins
SELECT u.name, o.total 
FROM users u 
JOIN orders o ON u.id = o.user_id;

-- Aggregations
SELECT country, COUNT(*) as user_count 
FROM users 
GROUP BY country 
HAVING user_count > 100;
```

### Data Manipulation Language (DML)

```sql
-- Insert single row
INSERT INTO users (name, age, email) VALUES ('Alice', 25, 'alice@example.com');

-- Insert multiple rows
INSERT INTO users (name, age) VALUES 
  ('Bob', 30),
  ('Carol', 28);

-- Update rows
UPDATE users SET age = 26 WHERE name = 'Alice';

-- Delete rows
DELETE FROM users WHERE age < 18;
```

### Data Definition Language (DDL)

```sql
-- Create table
CREATE TABLE users (
  id INTEGER PRIMARY KEY AUTO_INCREMENT,
  name VARCHAR(100) NOT NULL,
  email VARCHAR(255) UNIQUE,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create stream table with TTL
CREATE STREAM TABLE events (
  event_id VARCHAR(50),
  event_type VARCHAR(50),
  payload JSON,
  timestamp TIMESTAMP
) WITH (
  TTL = 3600,
  FLUSH_POLICY = 'immediate'
);

-- Drop table
DROP TABLE old_users;

-- Alter table (if supported)
ALTER TABLE users ADD COLUMN phone VARCHAR(20);
```

### System Tables

```sql
-- List all tables
SELECT * FROM system.tables;

-- View users
SELECT * FROM system.users;

-- Check jobs
SELECT * FROM system.jobs;

-- View namespaces
SELECT * FROM system.namespaces;

-- Monitor live queries
SELECT * FROM system.live_queries;
```

---

## Credential Management

The CLI can store credentials securely for multiple database instances.

### Store Credentials

```bash
# Interactive prompt (recommended for security)
kalam --update-credentials --instance production
# Prompts for: Username, Password

# Command-line (less secure, visible in shell history)
kalam --update-credentials --instance local \
  --username alice \
  --password secret123 \
  --url http://localhost:8080
```

### View Credentials

```bash
# List all stored instances
kalam --list-instances

# Show credentials for specific instance (password hidden)
kalam --show-credentials --instance production
```

### Use Stored Credentials

```bash
# Connect using stored credentials (auto-loaded)
kalam --instance production

# Override instance selection
kalam --instance local
```

### Delete Credentials

```bash
# Remove stored credentials
kalam --delete-credentials --instance test
```

### Credential Storage Location

- **Linux/macOS**: `~/.config/kalamdb/credentials.toml`
- **Windows**: `%APPDATA%\kalamdb\credentials.toml`

File format (TOML):
```toml
[instances.local]
username = "alice"
password = "secret123"
server_url = "http://localhost:8080"

[instances.production]
username = "admin"
password = "prod_password"
server_url = "https://db.example.com"
```

**Security Notes**:
- File permissions automatically set to 0600 (owner read/write only) on Unix
- Passwords stored in plain text - consider using OS keyring for production
- Credentials never logged or displayed (except in masked form)

---

## Configuration

### Configuration File

Default location: `~/.kalam/config.toml`

```toml
[server]
url = "http://localhost:8080"
timeout = 30
max_retries = 3

[auth]
jwt_token = "your-jwt-token"

[ui]
format = "table"  # table, json, csv
color = true
history_size = 1000
```

### Priority Order

Configuration values are loaded in this priority order (highest to lowest):

1. **Command-line arguments** - Direct flags like `--url`, `--username`
2. **Stored credentials** - From `~/.config/kalamdb/credentials.toml`
3. **Config file** - From `~/.kalam/config.toml`
4. **Defaults** - Built-in default values

### Command History

Command history is automatically saved to: `~/.kalam/history`

- Default size: 1000 commands
- Persists across sessions
- Navigate with ↑/↓ arrow keys
- Search with Ctrl+R (reverse search)

---

## Examples

### Basic Queries

```bash
# Start interactive session
kalam --url http://localhost:8080

# In the CLI:
kalam> SELECT * FROM users LIMIT 5;
kalam> \dt
kalam> \d users
kalam> \quit
```

### Non-Interactive Mode

```bash
# Execute single query
kalam -c "SELECT COUNT(*) FROM users" --json

# Execute from file
kalam -f migration.sql --verbose

# Pipe SQL commands
echo "SELECT * FROM users;" | kalam --csv > users.csv
```

### Real-Time Subscriptions

```bash
kalam> \subscribe SELECT * FROM messages WHERE created_at > NOW() - INTERVAL 5 MINUTES
# Now you'll receive real-time updates as new messages arrive
# Press Ctrl+C or use \unsubscribe to stop
```

### Multiple Instances

```bash
# Setup credentials for different environments
kalam --update-credentials --instance dev --username dev_user
kalam --update-credentials --instance staging --username staging_user
kalam --update-credentials --instance prod --username prod_admin

# Switch between instances
kalam --instance dev      # Connect to dev
kalam --instance staging  # Connect to staging
kalam --instance prod     # Connect to production
```

### Advanced Queries

```bash
# Complex aggregation with output formatting
kalam --instance prod \
  --command "SELECT country, COUNT(*) as users FROM users GROUP BY country" \
  --format json \
  --no-color > stats.json

# Stream processing with TTL
kalam> CREATE STREAM TABLE sensor_data (
    sensor_id VARCHAR(50),
    temperature FLOAT,
    timestamp TIMESTAMP
) WITH (TTL = 3600);

kalam> SELECT * FROM sensor_data WHERE temperature > 100;
```

---

## Smoke Tests

Fast end-to-end checks that your server and CLI are wired correctly. The suite covers:

- User table subscription lifecycle
- Shared table CRUD
- System tables and user lifecycle
- Stream table subscription
- User table row-level security (per-user isolation)

Requirements:

- Server running at http://localhost:8080 (tests will skip if it’s not available)
- Subscriptions are supported only for user and stream tables (not shared tables)

Run options:

1) From the CLI folder using the helper script

```bash
cd cli
./run_integration_tests.sh smoke
```

2) Directly with Cargo (filter matches the smoke test binary and names)

```bash
cargo test -p kalam-cli smoke -- --test-threads=1 --nocapture
```

Run individual tests (examples):

```bash
# User table subscription lifecycle
cargo test -p kalam-cli smoke_user_table_subscription_lifecycle -- --nocapture

# Shared table CRUD
cargo test -p kalam-cli smoke_shared_table_crud -- --nocapture

# System tables + user lifecycle
cargo test -p kalam-cli smoke_system_tables_and_user_lifecycle -- --nocapture

# Stream table subscription
cargo test -p kalam-cli smoke_stream_table_subscription -- --nocapture

# User table RLS (per-user isolation)
cargo test -p kalam-cli smoke_user_table_rls_isolation -- --nocapture
```

Notes:

- Tests are tolerant of output formatting and will skip cleanly when the server isn’t running.
- Default server URL for tests is http://localhost:8080.

---

## Keyboard Shortcuts

### Line Editing

| Shortcut | Action |
|----------|--------|
| `Ctrl+A` | Move to beginning of line |
| `Ctrl+E` | Move to end of line |
| `Ctrl+K` | Delete from cursor to end of line |
| `Ctrl+U` | Delete from cursor to beginning of line |
| `Ctrl+W` | Delete word before cursor |
| `Alt+D` | Delete word after cursor |

### History Navigation

| Shortcut | Action |
|----------|--------|
| `↑` | Previous command |
| `↓` | Next command |
| `Ctrl+R` | Reverse search history |
| `Ctrl+S` | Forward search history |

### Completion

| Shortcut | Action |
|----------|--------|
| `Tab` | Autocomplete SQL keywords, tables, columns |
| `Tab Tab` | Show all completions |

### Control

| Shortcut | Action |
|----------|--------|
| `Ctrl+C` | Cancel current query/subscription |
| `Ctrl+D` | Exit CLI (alternative to `\quit`) |
| `Ctrl+L` | Clear screen |

---

## Tips & Tricks

### 1. Auto-Completion

The CLI provides intelligent auto-completion:
- **SQL Keywords**: `SEL` + Tab → `SELECT`
- **Table Names**: `FROM us` + Tab → `FROM users`
- **Column Names**: Context-aware completion in SELECT/WHERE clauses

```sql
kalam> SELECT na[Tab]
kalam> SELECT name FROM us[Tab]
kalam> SELECT name FROM users WHERE a[Tab]
```

### 2. Loading Indicator

Queries taking longer than 200ms show a loading spinner:
```
⠋ Executing query...
```

### 3. Pretty Tables

Tables automatically adjust to terminal width:
- Columns exceeding 50 characters are truncated with `...`
- Total table width respects terminal size
- Change format with `\format json` or `\format csv`

### 4. Color Output

Disable colors for piping or logging:
```bash
kalam --no-color -c "SELECT * FROM users" > output.txt
```

### 5. Timing Information

Execution time is displayed for all queries:
```
(10 rows)

Took: 245.123 ms
```

### 6. Error Messages

Clear, actionable error messages:
```
ERROR 1001: Table 'users' not found
Details: Available tables: system.tables, system.users, events
```

### 7. Batch Operations

Execute multiple statements from a file:
```sql
-- migration.sql
CREATE TABLE products (id INT, name VARCHAR(100));
INSERT INTO products VALUES (1, 'Laptop'), (2, 'Phone');
SELECT * FROM products;
```

```bash
kalam -f migration.sql
```

### 8. Watch Mode (Real-Time)

Monitor live data changes:
```sql
-- Terminal 1: Start watching
kalam> \subscribe SELECT * FROM orders WHERE status = 'pending'

-- Terminal 2: Insert data
kalam> INSERT INTO orders (id, status) VALUES (1, 'pending');

-- Terminal 1 automatically shows the new row
```

### 9. Quick Health Check

```bash
# One-liner health check
kalam -c "\health" && echo "Database is up!"
```

### 10. System Introspection

```sql
-- Find large tables
SELECT table_name, row_count 
FROM system.tables 
ORDER BY row_count DESC;

-- Monitor active connections
SELECT * FROM system.users WHERE last_seen > NOW() - INTERVAL 5 MINUTES;

-- Check running jobs
SELECT * FROM system.jobs WHERE status = 'running';
```

### 11. Cache Statistics and System Metrics

View real-time cache performance and system metrics using the `\stats` command (alias: `\metrics`):

```bash
# Show all cache statistics
kalam> \stats

# Or use the alias
kalam> \metrics
```

**Expected Output**:

```
┌──────────────────────────┬──────────┐
│ key                      │ value    │
├──────────────────────────┼──────────┤
│ schema_cache_hit_rate    │ 0.998    │
│ schema_cache_size        │ 147      │
│ schema_cache_hits        │ 98234    │
│ schema_cache_misses      │ 201      │
│ schema_cache_evictions   │ 0        │
└──────────────────────────┴──────────┘
```

**Key Metrics Explained**:

| Metric | Description | Target Value |
|--------|-------------|--------------|
| `schema_cache_hit_rate` | Percentage of schema lookups served from cache (0.0-1.0) | >0.99 (99%+) |
| `schema_cache_size` | Current number of cached table schemas | ≤1000 |
| `schema_cache_hits` | Total cache hit count (monotonic) | N/A |
| `schema_cache_misses` | Total cache miss count (monotonic) | <1% of hits |
| `schema_cache_evictions` | LRU evictions (when cache size exceeds 1000) | Low |

**SQL Equivalent**:

The `\stats` command is equivalent to:

```sql
SELECT * FROM system.stats ORDER BY key;
```

**Filtering Specific Metrics**:

```sql
-- View only cache-related stats
SELECT * FROM system.stats WHERE key LIKE 'schema_cache%';

-- Calculate hit ratio
SELECT 
  (SELECT value::FLOAT FROM system.stats WHERE key = 'schema_cache_hits') / 
  ((SELECT value::FLOAT FROM system.stats WHERE key = 'schema_cache_hits') + 
   (SELECT value::FLOAT FROM system.stats WHERE key = 'schema_cache_misses')) AS hit_ratio;
```

**Interpreting Results**:

**Healthy System** (Expected):
- ✅ `schema_cache_hit_rate` ≥ 0.99 (99%+)
- ✅ `schema_cache_evictions` = 0 or very low
- ✅ Cache misses <1% of hits

**Performance Issues** (Investigate):
- ⚠️ `schema_cache_hit_rate` < 0.90 (90%) - High table churn or cache too small
- ⚠️ `schema_cache_evictions` growing rapidly - Cache size too small (>1000 tables)
- ⚠️ Cache misses >10% of hits - Frequent schema changes (ALTER TABLE)

**Example Monitoring Script**:

```bash
# Monitor cache hit rate every 10 seconds
while true; do
  echo "=== $(date) ==="
  kalam -c "SELECT key, value FROM system.stats WHERE key = 'schema_cache_hit_rate'" --format table
  sleep 10
done
```

**Real-World Example**:

```bash
kalam> \stats

┌──────────────────────────┬──────────┐
│ key                      │ value    │
├──────────────────────────┼──────────┤
│ schema_cache_hit_rate    │ 0.992    │  # 99.2% hit rate (excellent)
│ schema_cache_size        │ 247      │  # 247 tables cached
│ schema_cache_hits        │ 1250482  │  # 1.25M hits
│ schema_cache_misses      │ 10024    │  # 10K misses (0.8%)
│ schema_cache_evictions   │ 0        │  # No evictions (cache not full)
└──────────────────────────┴──────────┘

# Analysis: System is performing optimally
# - Hit rate 99.2% (above 99% target)
# - Cache size 247 << 1000 (plenty of capacity)
# - Zero evictions (LRU not triggered)
```

**Performance Tuning**:

If cache hit rate is low, consider:

1. **Reduce table churn**: Avoid frequent CREATE/DROP TABLE operations
2. **Increase cache size**: Modify `SchemaCache::new(2000)` in backend code
3. **Batch schema changes**: Group ALTER TABLE operations together

**Future Metrics** (Roadmap):

The `system.stats` table will expand to include:
- `queries_per_second` - Query throughput
- `avg_query_latency_ms` - Average query execution time
- `memory_usage_bytes` - Total memory consumption
- `cpu_usage_percent` - CPU utilization
- `disk_space_used_bytes` - Storage usage
- `active_connections` - Current WebSocket connections

---

## Troubleshooting

### Connection Issues

```bash
# Test connection
kalam --url http://localhost:8080 -c "\health"

# Verbose mode for debugging
kalam --verbose --url http://localhost:8080
```

### Authentication Failures

```bash
# Verify stored credentials
kalam --show-credentials --instance local

# Clear and re-enter credentials
kalam --delete-credentials --instance local
kalam --update-credentials --instance local
```

### Performance Issues

```bash
# Reduce timeout for faster failures
# Edit ~/.kalam/config.toml:
[server]
timeout = 10  # seconds

# Check query execution time
kalam> SELECT * FROM large_table LIMIT 1;
# Took: 1234.567 ms
```

### Display Issues

```bash
# Disable colors if rendering incorrectly
kalam --no-color

# Switch to JSON for machine-readable output
kalam --format json

# Adjust terminal width or use CSV
kalam --csv
```

---

## Related Documentation

- [API Examples (Bruno collection)](API-Kalam/) - REST API request examples
- [SQL Syntax](architecture/SQL_SYNTAX.md) - Complete SQL syntax guide
- [WebSocket Protocol](architecture/WEBSOCKET_PROTOCOL.md) - Real-time subscription details
- [Development Setup](build/DEVELOPMENT_SETUP.md) - Build and development guide

---

## Support

For issues, questions, or contributions:
- GitHub: [github.com/jamals86/KalamDB](https://github.com/jamals86/KalamDB)
- Documentation: [docs/README.md](README.md)

---

**Version**: 0.1.0  
**Last Updated**: October 28, 2025
