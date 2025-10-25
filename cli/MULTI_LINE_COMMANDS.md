# Multi-Line Command Support in Kalam CLI

The Kalam CLI now supports multi-line commands with proper history preservation.

## How It Works

### Single-Line Commands
Commands ending with `;` or backslash commands (`\help`, `\quit`, etc.) execute immediately:

```
kalam> SELECT * FROM users;
```

### Multi-Line Commands
Commands without a semicolon will show a continuation prompt (`    -> `):

```
kalam> CREATE TABLE app.users (
    ->   id TEXT PRIMARY KEY,
    ->   name TEXT NOT NULL,
    ->   email TEXT
    -> );
```

### Pasting Multi-Line SQL
When you paste multi-line SQL (e.g., from a file), the CLI will:
1. Accumulate all lines
2. Execute when it sees the semicolon
3. **Store the entire command as ONE history entry**

This means pressing ↑ will recall the entire CREATE TABLE statement, not just individual lines.

## Features

### Continuation Prompt
- Main prompt: `kalam> `
- Continuation prompt: `    -> `

### Ctrl+C Behavior
- In multi-line mode: Cancels the current accumulated command
- In single-line mode: Shows "Use \quit or \q to exit"

### History Navigation
- ↑/↓ arrows navigate through complete commands
- Multi-line commands are preserved with newlines
- Press ↑ once to recall an entire CREATE TABLE or INSERT statement

## Examples

### Interactive Multi-Line
```
kalam> CREATE TABLE app.events (
    ->   id TEXT PRIMARY KEY,
    ->   event_type TEXT,
    ->   created_at INTEGER
    -> );
✓ Table app.events created successfully
```

### Pasted Multi-Line (preserved as single history entry)
```
kalam> CREATE TABLE app.messages (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  message TEXT,
  timestamp INTEGER
);
✓ Table app.messages created successfully
```

Now press ↑ to see the entire CREATE TABLE statement recalled as one command!

### Cancelling Multi-Line Input
```
kalam> CREATE TABLE app.test (
    ->   id TEXT,
    ->   ^C
Command cancelled
kalam> 
```

## Testing the Feature

1. Start the CLI:
   ```bash
   cd cli && cargo run --bin kalam-cli
   ```

2. Try a multi-line CREATE TABLE:
   ```sql
   CREATE TABLE app.test (
     id TEXT PRIMARY KEY,
     name TEXT
   );
   ```

3. Press ↑ to verify the entire command is recalled as one history entry

4. Try pasting multi-line SQL and verify it's stored as a single history entry
