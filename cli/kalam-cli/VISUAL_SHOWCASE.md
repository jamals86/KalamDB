# Kalam CLI - Visual Showcase 🎨

## Welcome Experience

```
╔═══════════════════════════════════════════════════════════╗
║                                                           ║
║        🗄️  Kalam CLI - Interactive Database Terminal        ║
║                                                           ║
╚═══════════════════════════════════════════════════════════╝

  📡  Connected to: http://localhost:3000
  📚  Version: 0.1.0
  💡  Type \help for help, \quit to exit

  kalam ❯ _
```

## Syntax Highlighting in Action

```sql
-- As you type, colors appear in real-time:

  kalam ❯ SELECT name, age FROM users WHERE age > 18;
          ^^^^^^ ^^^^^ ^^^ ^^^^ ^^^^^ ^^^^^ ^^^ ^ ^^
          blue   white white blue white blue white yellow

  kalam ❯ INSERT INTO messages (text, sender) VALUES ('Hello', 'Alice');
          ^^^^^^ ^^^^ ^^^^^^^^ ^^^^^ ^^^^^^ ^^^^^^ ^^^^^^^^^ ^^^^^^^^^
          blue   blue  white   white white  blue   green     green

  kalam ❯ CREATE TABLE products (id INTEGER PRIMARY KEY, name TEXT);
          ^^^^^^ ^^^^^ ^^^^^^^^ ^^ ^^^^^^^ ^^^^^^^ ^^^ ^^^^ ^^^^
          blue   blue   white   white magenta blue    blue white magenta
```

## Autocomplete Suggestions

Press TAB after typing `SE`:

```
  kalam ❯ SE_
          
  SELECT  keyword
  SET     keyword
```

Press TAB after `SELECT * FROM us`:

```
  kalam ❯ SELECT * FROM us_
          
  users         table
  user_logs     table
  user_sessions table
```

Press TAB after `SELECT users.n`:

```
  kalam ❯ SELECT users.n_
          
  name        column
  name_first  column
  name_last   column
```

Press TAB after `\`:

```
  kalam ❯ \_
          
  \quit             command
  \help             command
  \tables           command
  \describe         command
  \refresh-tables   command
```

## Query Execution with Timing

```
  kalam ❯ SELECT COUNT(*) FROM large_table;
⠋ Executing query...

┌─────────┐
│ count   │
├─────────┤
│ 1000000 │
└─────────┘

⏱  Time: 523.456 ms
```

## Success Messages

```
  kalam ❯ \refresh-tables
✓ Table names refreshed

  kalam ❯ CREATE TABLE test (id INT);
✓ Table created successfully
```

## Error Messages

```
  kalam ❯ SELEC * FROM users;
✗ Parse error: Invalid SQL syntax near 'SELEC'

  kalam ❯ SELECT * FROM nonexistent_table;
✗ Error: Table 'nonexistent_table' does not exist
```

## Warning Messages

```
  kalam ❯ [Server connection lost]
⚠ Could not fetch table names: connection timeout

  kalam ❯ [Attempting reconnection]
⚠ Retrying connection (attempt 2/3)...
```

## Disconnected State

```
  kalam ❯ [Server stopped]
  
  kalam ❯ SELECT * FROM users;
          ^^^^^^^^^^^^^^^^^^^^
          (prompt changes to red background)

✗ Error: Not connected to server
```

## Help Command Output

```
  kalam ❯ \help

Kalam CLI Commands:

  SQL Statements:
    SELECT, INSERT, UPDATE, DELETE, CREATE TABLE, etc.

  Meta-commands:
    \quit, \q              Exit the CLI
    \help, \?              Show this help message
    \connect <url>         Connect to a different server
    \config                Show current configuration
    \flush                 Flush all data to disk
    \health                Check server health
    \pause                 Pause ingestion
    \continue              Resume ingestion
    \dt, \tables           List all tables
    \d <table>             Describe table schema
    \format <type>         Set output format (table, json, csv)
    \subscribe <query>     Start WebSocket subscription
    \watch <query>         Alias for \subscribe
    \unsubscribe           Cancel active subscription
    \refresh-tables        Refresh table names for autocomplete

  Features:
    - TAB completion for SQL keywords, table names, and columns
    - Loading indicator for queries taking longer than 200ms
    - Command history (saved in ~/.kalam/history)

  Examples:
    SELECT * FROM users WHERE age > 18;
    INSERT INTO users (name, age) VALUES ('Alice', 25);
    \dt
    \d users
    \subscribe SELECT * FROM messages
```

## Color Palette Reference

### Text Colors:
- **Blue Bold** (`\x1b[1;34m`) - SQL keywords (SELECT, FROM, WHERE)
- **Magenta Bold** (`\x1b[1;35m`) - Data types (INTEGER, TEXT, VARCHAR)
- **Green** (`\x1b[32m`) - String literals, success messages
- **Yellow** (`\x1b[33m`) - Numbers, warnings
- **Red** (`\x1b[31m`) - Errors
- **Cyan** (`\x1b[36m`) - Operators, meta-commands
- **Bright Cyan** (`\x1b[96m`) - Meta-commands, prompt arrow
- **White/Normal** (`\x1b[0m`) - Identifiers, regular text
- **Dimmed** (`\x1b[2m`) - Secondary information (timing, categories)

### Background Colors:
- **Bright Cyan Background** (`\x1b[106m`) - Connected prompt
- **Red Background** (`\x1b[41m`) - Disconnected prompt

### Icons Used:
- 🗄️ - Database/CLI branding
- 📡 - Connection status
- 📚 - Version information
- 💡 - Help/Tips
- ✓ - Success
- ✗ - Error
- ⚠ - Warning
- ⏱ - Timing information
- ❯ - Prompt arrow
- ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ - Loading spinner frames

## Terminal Compatibility

Works beautifully on:
- Windows Terminal ✅
- PowerShell 7 ✅
- VS Code Terminal ✅
- Git Bash ✅
- WSL ✅
- macOS Terminal ✅
- iTerm2 ✅
- Linux terminals (GNOME, KDE, etc.) ✅

## Accessibility

- Colors can be disabled with `--no-color` flag
- All information available in plain text mode
- High contrast colors for readability
- Icons are supplementary, not required for understanding
- Screen reader friendly in no-color mode

---

**Experience the difference**: Compare this to a plain black-and-white terminal and see how much more enjoyable and productive your database work becomes! 🚀
