# Quickstart: Production Readiness Features

## Prerequisites
- KalamDB running locally
- `kalam` CLI installed

## 1. ALTER TABLE Demo

```bash
# Create a table
kalam sql "CREATE TABLE users (id INT PRIMARY KEY, name TEXT)"

# Insert data
kalam sql "INSERT INTO users VALUES (1, 'Alice')"

# Add a column
kalam sql "ALTER TABLE users ADD COLUMN age INT"

# Verify schema
kalam sql "SELECT * FROM system.tables WHERE table_name = 'users'"

# Insert with new column
kalam sql "INSERT INTO users VALUES (2, 'Bob', 30)"

# Select all (Alice should have NULL age)
kalam sql "SELECT * FROM users"
```

## 2. System Tables Demo

```bash
# List all tables with options
kalam sql "SELECT table_name, options FROM system.tables"

# Check live queries (run in separate terminal)
kalam sql "SELECT * FROM system.live_queries"
```

## 3. Manifest Verification

```bash
# Check if manifest.json exists
ls -l data/storage/default/users/manifest.json
```
