# Logging Format Comparison

## Console Output (With Colors)

The console output uses ANSI color codes for better readability:

```
[2025-10-14 16:48:54.208] [INFO ] - main - kalamdb_server::logging:84 - Logging initialized: level=info, console=yes, file=./logs/app.log
    ^^^^^^^^^^^^^^^^^^^^^^^^^    ^^^^^      ^^^^   ^^^^^^^^^^^^^^^^^^^^^^^^
    GREEN BOLD                   GREEN BOLD        MAGENTA
```

**Color Scheme:**
- `[timestamp]` - Bright Green Bold
- `[INFO]` - Bright Green Bold
- `[WARN]` - Bright Yellow Bold
- `[ERROR]` - Bright Red Bold
- `[DEBUG]` - Bright Blue Bold
- `[TRACE]` - Bright Magenta Bold
- `thread - module:line` - Bright Magenta
- `message` - Default terminal color

## File Output (Plain Text)

The file output contains the same information but without color codes:

```
[2025-10-14 16:48:54.208] [INFO ] [main - kalamdb_server::logging:84] - Logging initialized: level=info, console=yes, file=./logs/app.log
```

**Key Differences:**

| Aspect | Console | File |
|--------|---------|------|
| Colors | ✅ Yes (ANSI codes) | ❌ No (plain text) |
| Thread/Module Format | `- thread - module:line -` | `[thread - module:line] -` |
| Timestamp Format | Same | Same |
| Log Level | Colored and padded | Plain and padded |
| Readability | High (colors help) | High (easier parsing) |
| Greppable | Moderate | Excellent |
| Size | Larger (ANSI codes) | Smaller |

## Pattern Breakdown

### Console Pattern
```
[timestamp] [LEVEL] - thread - module:line - message
    ^          ^       ^        ^     ^        ^
    |          |       |        |     |        |
    |          |       |        |     |        +-- Log message
    |          |       |        |     +----------- Line number
    |          |       |        +----------------- Module/target name
    |          |       +-------------------------- Thread name
    |          +---------------------------------- Log level (5 chars, padded)
    +--------------------------------------------- ISO timestamp with milliseconds
```

### File Pattern
```
[timestamp] [LEVEL] [thread - module:line] - message
    ^          ^      ^        ^     ^          ^
    |          |      |        |     |          |
    |          |      |        |     |          +-- Log message
    |          |      |        |     +------------- Line number
    |          |      |        +------------------- Module/target name
    |          |      +---------------------------- Thread name
    |          +----------------------------------- Log level (5 chars, padded)
    +---------------------------------------------- ISO timestamp with milliseconds
```

## Usage Examples

### Info Messages
**Console:** `[2025-10-14 16:48:54.208] [INFO ] - main - kalamdb_server:34 - Starting KalamDB Server v0.1.0`
**File:**    `[2025-10-14 16:48:54.208] [INFO ] [main - kalamdb_server:34] - Starting KalamDB Server v0.1.0`

### Warning Messages
**Console:** `[2025-10-14 16:48:54.208] [WARN ] - worker-1 - kalamdb_api::handlers:156 - Rate limit approaching`
**File:**    `[2025-10-14 16:48:54.208] [WARN ] [worker-1 - kalamdb_api::handlers:156] - Rate limit approaching`

### Error Messages
**Console:** `[2025-10-14 16:48:54.208] [ERROR] - worker-2 - kalamdb_core::storage:89 - Database connection lost`
**File:**    `[2025-10-14 16:48:54.208] [ERROR] [worker-2 - kalamdb_core::storage:89] - Database connection lost`

## Grep Examples

### Search for errors in file log
```bash
grep "\[ERROR\]" logs/app.log
```

### Search for specific module
```bash
grep "kalamdb_server::logging" logs/app.log
```

### Search by time range
```bash
grep "2025-10-14 16:48" logs/app.log
```

### Search by thread
```bash
grep "\[main -" logs/app.log
```

## Benefits

1. **Developer-Friendly Console**: Colors make it easy to spot errors and warnings during development
2. **Machine-Readable File**: Plain text format is perfect for log aggregation tools
3. **Thread Information**: Easy to debug multi-threaded issues
4. **Source Location**: Module and line number for quick navigation
5. **Timestamps**: Millisecond precision for performance analysis
