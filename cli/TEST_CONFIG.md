# CLI Test Configuration Quick Reference

## Environment Variables

The CLI tests support two environment variables for configuration:

| Variable | Purpose | Default |
|----------|---------|---------|
| `KALAMDB_SERVER_URL` | Full server URL including protocol and port | `http://127.0.0.1:8080` |
| `KALAMDB_ROOT_PASSWORD` | Root user password for authentication | `""` (empty string) |

## Usage Examples

### Unix/Linux/macOS

```bash
# Test against different port
KALAMDB_SERVER_URL="http://127.0.0.1:3000" cargo test

# Test with authentication
KALAMDB_ROOT_PASSWORD="secure-password" cargo test

# Test against remote server
KALAMDB_SERVER_URL="https://kalamdb.example.com" \
KALAMDB_ROOT_PASSWORD="mypass" \
cargo test --test smoke -- --nocapture

# Using helper script (recommended)
./run-tests.sh --url http://localhost:3000 --password mypass --test smoke --nocapture
```

### Windows PowerShell

```powershell
# Set environment variables
$env:KALAMDB_SERVER_URL = "http://127.0.0.1:3000"
$env:KALAMDB_ROOT_PASSWORD = "secure-password"
cargo test

# Or use helper script (recommended)
.\run-tests.ps1 -Url "http://localhost:3000" -Password "mypass" -Test "smoke" -NoCapture
```

## Helper Scripts

### Bash Script (run-tests.sh)

```bash
./run-tests.sh [OPTIONS]

Options:
  -u, --url <URL>          Server URL (default: http://127.0.0.1:8080)
  -p, --password <PASS>    Root password (default: empty)
  -t, --test <FILTER>      Test filter (e.g., 'smoke', 'smoke_test_core')
  --nocapture              Show test output
  -h, --help               Show help message

Examples:
  ./run-tests.sh --test smoke --nocapture
  ./run-tests.sh --url http://localhost:3000 --password mypass
  ./run-tests.sh --url http://localhost:3000 --test smoke_test_core --nocapture
```

### PowerShell Script (run-tests.ps1)

```powershell
.\run-tests.ps1 [OPTIONS]

Options:
  -Url <URL>           Server URL (default: http://127.0.0.1:8080)
  -Password <PASS>     Root password (default: empty)
  -Test <FILTER>       Test filter (e.g., 'smoke', 'smoke_test_core')
  -NoCapture           Show test output
  -Help                Show help message

Examples:
  .\run-tests.ps1 -Test "smoke" -NoCapture
  .\run-tests.ps1 -Url "http://localhost:3000" -Password "mypass"
  .\run-tests.ps1 -Url "http://localhost:3000" -Test "smoke_test_core" -NoCapture
```

## Common Testing Scenarios

### Testing Against Cluster

```bash
# Test against node 1 (port 8081)
./run-tests.sh --url http://127.0.0.1:8081 --test smoke

# Test against node 2 (port 8082)
./run-tests.sh --url http://127.0.0.1:8082 --test smoke

# Test against node 3 (port 8083)
./run-tests.sh --url http://127.0.0.1:8083 --test smoke
```

### Testing With Authentication Enabled

```bash
# When server is started with authentication
./run-tests.sh --password "admin-password" --test smoke --nocapture
```

### Testing Specific Features

```bash
# Test only core operations
./run-tests.sh --test smoke_test_core_operations --nocapture

# Test only flush operations
./run-tests.sh --test smoke_test_flush --nocapture

# Test with custom configuration
./run-tests.sh --url http://localhost:3000 --password mypass --test smoke_test_all_datatypes --nocapture
```

## Implementation Details

The configuration is implemented in `cli/tests/common/mod.rs`:

```rust
pub fn server_url() -> &'static str {
    SERVER_URL
        .get_or_init(|| {
            std::env::var("KALAMDB_SERVER_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
        })
        .as_str()
}

pub fn root_password() -> &'static str {
    ROOT_PASSWORD
        .get_or_init(|| {
            std::env::var("KALAMDB_ROOT_PASSWORD")
                .unwrap_or_else(|_| "".to_string())
        })
        .as_str()
}
```

All tests use these functions to get the server URL and credentials, ensuring consistent configuration across the entire test suite.
