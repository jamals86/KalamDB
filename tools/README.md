# KalamDB Tools

This directory contains scripts and tools for building, testing, and releasing KalamDB.

## Scripts

### `release.sh`

Automated release script that builds both the CLI and server binaries and uploads them to GitHub Releases.

**Prerequisites:**
- [GitHub CLI (`gh`)](https://cli.github.com/) installed and authenticated
- Rust toolchain installed
- `tar` and `gzip` (for Linux/macOS) or `zip` (for Windows)
- `sha256sum` or `shasum` for generating checksums

**Usage:**

```bash
./tools/release.sh <version> [--draft] [--prerelease]
```

**Examples:**

```bash
# Create a stable release
./tools/release.sh v0.1.0

# Create a draft release (not published immediately)
./tools/release.sh v0.1.0 --draft

# Create a pre-release (beta, rc, etc.)
./tools/release.sh v0.2.0-beta --prerelease

# Create a draft pre-release
./tools/release.sh v0.2.0-rc1 --draft --prerelease
```

**What it does:**

1. ✅ Validates GitHub CLI installation and authentication
2. 🧹 Cleans previous builds
3. 🔨 Builds CLI (`cli/` directory)
4. 🔨 Builds Server (`backend/` directory)
5. 📦 Creates platform-specific archives (`.tar.gz` or `.zip`)
6. 🔐 Generates SHA256 checksums
7. 🚀 Creates GitHub release with all artifacts
8. 📝 Generates release notes with installation instructions

**Output:**

The script creates a `dist/` directory containing:
- `kalam-cli-<version>-<platform>` - CLI binary
- `kalamdb-server-<version>-<platform>` - Server binary
- `kalam-cli-<version>-<platform>.tar.gz` or `.zip` - CLI archive
- `kalamdb-server-<version>-<platform>.tar.gz` or `.zip` - Server archive
- `SHA256SUMS` - Checksums file

**Platform Detection:**

The script automatically detects your platform:
- Linux: `linux-x86_64`, `linux-aarch64`
- macOS: `darwin-x86_64`, `darwin-aarch64`
- Windows: `windows-x86_64`

**Notes:**

- The script will prompt before deleting an existing release with the same version tag
- Use `--draft` to review the release before publishing
- The `dist/` directory can be cleaned up after the release is created

## GitHub CLI Setup

If you haven't set up the GitHub CLI yet:

1. **Install GitHub CLI:**
   - **macOS:** `brew install gh`
   - **Linux:** See [installation instructions](https://github.com/cli/cli/blob/trunk/docs/install_linux.md)
   - **Windows:** `winget install --id GitHub.cli` or download from [releases](https://github.com/cli/cli/releases)

2. **Authenticate:**
   ```bash
   gh auth login
   ```
   
   Follow the prompts to authenticate with your GitHub account.

3. **Verify:**
   ```bash
   gh auth status
   ```

## Cross-Compilation (Advanced)

To build binaries for multiple platforms, you can use Rust's cross-compilation:

```bash
# Install cross-compilation tool
cargo install cross

# Build for different targets
cross build --release --target x86_64-unknown-linux-gnu
cross build --release --target aarch64-unknown-linux-gnu
cross build --release --target x86_64-apple-darwin
cross build --release --target aarch64-apple-darwin
cross build --release --target x86_64-pc-windows-gnu
```

Note: The current `release.sh` script builds only for the host platform. Multi-platform builds require additional setup.

## Troubleshooting

### "gh: command not found"

Install the GitHub CLI as described in the setup section above.

### "Not logged in to GitHub"

Run `gh auth login` and follow the authentication flow.

### "Binary not found"

Ensure the build succeeded. Check for errors in the cargo build output.

### Permission denied on Unix-like systems

Make the script executable:
```bash
chmod +x tools/release.sh
```

## Contributing

When adding new tools or scripts to this directory:

1. Use bash for maximum compatibility
2. Add error handling (`set -e`)
3. Include usage documentation
4. Update this README
5. Test on multiple platforms if possible
