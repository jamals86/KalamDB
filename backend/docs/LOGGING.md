# KalamDB Logging System

## Overview

KalamDB uses a dual-output logging system with separate formatting for console and file outputs, similar to SLF4J patterns.

## Features

- ✅ **Colored Console Output**: Timestamps, log levels, and module names with colors
- ✅ **Plain File Output**: Clean, parseable log format without ANSI codes
- ✅ **Dual Output**: Simultaneously write to both console and file with different formats
- ✅ **Configurable Log Levels**: error, warn, info, debug, trace

## Log Formats

### Console Format (Colored)
```
[timestamp] [LEVEL] - thread - module:line - message
```

**Example:**
```
[2025-10-14 16:48:54.208] [INFO ] - main - kalamdb_server::logging:84 - Logging initialized
[2025-10-14 16:48:54.210] [INFO ] - main - kalamdb_server:34 - Starting KalamDB Server v0.1.0
```

**Colors:**
- Timestamp: **Bright Green Bold**
- ERROR: **Bright Red Bold**
- WARN: **Bright Yellow Bold**
- INFO: **Bright Green Bold**
- DEBUG: **Bright Blue Bold**
- TRACE: **Bright Magenta Bold**
- Thread/Module: **Bright Magenta**

### File Format (Plain Text)
```
[timestamp] [LEVEL] [thread - module:line] - message
```

**Example:**
```
[2025-10-14 16:48:54.208] [INFO ] [main - kalamdb_server::logging:84] - Logging initialized
[2025-10-14 16:48:54.210] [INFO ] [main - kalamdb_server:34] - Starting KalamDB Server v0.1.0
```

## Configuration

Edit `config.toml`:

```toml
[logging]
# Log level: "error", "warn", "info", "debug", "trace"
level = "info"

# Log file path
file_path = "./logs/app.log"

# Enable console output with colors
log_to_console = true
```

## Comparison with SLF4J

This implementation is inspired by SLF4J/Logback patterns:

**SLF4J Console Pattern:**
```xml
<property name="CONSOLE.PATTERN" 
  value="%-34(%boldGreen([%date]) %highlight([%level])) - %-50(%magenta(%thread - %logger:%M)) - %message%n%xException{25}" />
```

**KalamDB Console Pattern:**
```rust
[timestamp].green.bold() [LEVEL].colored().bold() - thread - module:line.magenta() - message
```

**SLF4J File Pattern:**
```xml
<property name="FILE.PATTERN" 
  value="[%date] [%level] %-30([%thread] - [%logger{15}:%M]) - %message%n%xException" />
```

**KalamDB File Pattern:**
```rust
[timestamp] [LEVEL] [thread - module:line] - message
```

## Implementation Details

- **Framework**: Uses `fern` for dual-output logging
- **Colors**: Uses `colored` crate for terminal colors
- **Thread-Safe**: All logging is thread-safe and async-safe
- **No Performance Impact**: File output has no ANSI escape codes overhead

## Log Levels

| Level | Description | Example Use Case |
|-------|-------------|------------------|
| ERROR | Critical errors that need immediate attention | Database connection failures, panic scenarios |
| WARN  | Warning conditions that should be reviewed | Deprecated API usage, recoverable errors |
| INFO  | Informational messages about application flow | Server startup, configuration loaded, requests |
| DEBUG | Detailed information for debugging | Function entry/exit, variable values |
| TRACE | Very detailed diagnostic information | Step-by-step execution flow |

## Examples

### Startup Logs
```
[2025-10-14 16:48:54.208] [INFO ] - main - kalamdb_server::logging:84 - Logging initialized: level=info, console=yes, file=./logs/app.log
[2025-10-14 16:48:54.210] [INFO ] - main - kalamdb_server:34 - Starting KalamDB Server v0.1.0
[2025-10-14 16:48:54.210] [INFO ] - main - kalamdb_server:35 - Configuration loaded: host=127.0.0.1, port=8080
[2025-10-14 16:48:54.210] [INFO ] - main - kalamdb_server:38 - Opening RocksDB at: ./data/rocksdb
[2025-10-14 16:48:54.240] [INFO ] - main - kalamdb_server:44 - RocksDB opened successfully
[2025-10-14 16:48:54.241] [INFO ] - main - kalamdb_server:57 - Starting HTTP server on 127.0.0.1:8080
```

## Dependencies

```toml
log = "0.4"
fern = "0.6"
colored = "2.1"
chrono = "0.4"
```
