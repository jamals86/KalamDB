# Helper script to run CLI tests with custom server URL and authentication
#
# Usage:
#   .\run-tests.ps1                                              # Use defaults
#   .\run-tests.ps1 -Url "http://localhost:3000"                # Custom URL
#   .\run-tests.ps1 -Password "mypass"                          # Custom password
#   .\run-tests.ps1 -Url "http://localhost:3000" -Password "mypass" -Test "smoke"
#
# Examples:
#   .\run-tests.ps1 -Test "smoke"                               # Run smoke tests only
#   .\run-tests.ps1 -Url "http://localhost:3000"                # Test on port 3000
#   .\run-tests.ps1 -Test "smoke_test_core" -NoCapture          # Run specific test with output

param(
    [string]$Url = $env:KALAMDB_SERVER_URL,
    [string]$Password = $env:KALAMDB_ROOT_PASSWORD,
    [string]$Test = "",
    [switch]$NoCapture,
    [switch]$Help
)

# Set defaults if not provided
if ([string]::IsNullOrEmpty($Url)) {
    $Url = "http://127.0.0.1:8080"
}
if ([string]::IsNullOrEmpty($Password)) {
    $Password = ""
}

if ($Help) {
    Write-Host "Usage: .\run-tests.ps1 [OPTIONS]"
    Write-Host ""
    Write-Host "Options:"
    Write-Host "  -Url <URL>           Server URL (default: http://127.0.0.1:8080)"
    Write-Host "  -Password <PASS>     Root password (default: empty)"
    Write-Host "  -Test <FILTER>       Test filter (e.g., 'smoke', 'smoke_test_core')"
    Write-Host "  -NoCapture           Show test output"
    Write-Host "  -Help                Show this help message"
    Write-Host ""
    Write-Host "Examples:"
    Write-Host "  .\run-tests.ps1 -Test smoke -NoCapture"
    Write-Host "  .\run-tests.ps1 -Url http://localhost:3000 -Password mypass"
    Write-Host "  .\run-tests.ps1 -Url http://localhost:3000 -Test smoke"
    exit 0
}

# Display configuration
Write-Host "================================================"
Write-Host "Running KalamDB CLI Tests"
Write-Host "================================================"
Write-Host "Server URL:      $Url"
if ([string]::IsNullOrEmpty($Password)) {
    Write-Host "Root Password:   (empty)"
} else {
    Write-Host "Root Password:   ***"
}
if ([string]::IsNullOrEmpty($Test)) {
    Write-Host "Test Filter:     (all tests)"
} else {
    Write-Host "Test Filter:     $Test"
}
Write-Host "================================================"
Write-Host ""

# Set environment variables
$env:KALAMDB_SERVER_URL = $Url
$env:KALAMDB_ROOT_PASSWORD = $Password

# Build test command
$testArgs = @("test")

if (![string]::IsNullOrEmpty($Test)) {
    # Check if it's a smoke test
    if ($Test -like "smoke*") {
        $testArgs += @("--test", "smoke", $Test)
    } else {
        $testArgs += $Test
    }
}

if ($NoCapture) {
    $testArgs += @("--", "--nocapture")
} elseif (![string]::IsNullOrEmpty($Test)) {
    $testArgs += "--"
}

# Run tests
Write-Host "Executing: cargo $($testArgs -join ' ')"
Write-Host ""

& cargo @testArgs
