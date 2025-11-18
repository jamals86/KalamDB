# Quick Build & Release Guide

## 🎯 Choose Your Workflow

### 1. Build Only (No Upload) ⭐ NEW!

```bash
# Build for specific platform
./tools/build-platform.sh 0.1.0 linux-x86_64
./tools/build-platform.sh 0.1.0 macos-aarch64
./tools/build-platform.sh 0.1.0 windows-x86_64

# Build for all platforms
./tools/build-all-platforms.sh 0.1.0
```

**When to use:**
- Testing builds before release
- Local development and validation
- No GitHub upload needed

**Output:** `dist/<version>/` with conventional naming

---

### 2. Build + Upload to GitHub

```bash
# Current OS only
./tools/release.sh v0.1.0

# All platforms (Docker)
./tools/release-multiplatform.sh v0.1.0
```

**When to use:**
- Ready to publish
- Creating GitHub releases

**Prerequisites:** GitHub CLI (`gh auth login`)

---

## Prerequisites

### For `build-platform.sh` / `build-all-platforms.sh`

```bash
# 1. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Cross-compilation tools (auto-installed)
# (cross will be installed automatically when needed)
```

### For `release.sh` / `release-multiplatform.sh`

```bash
# 1. Install Docker (multiplatform only)
https://docs.docker.com/get-docker/

# 2. Install GitHub CLI
winget install --id GitHub.cli   # Windows
brew install gh                   # macOS
# See: https://cli.github.com/

# 3. Authenticate
gh auth login
```

---

## Common Commands

### Build Commands (Local, No Upload)

```bash
# Build Linux x86_64
./tools/build-platform.sh 0.1.0 linux-x86_64

# Build macOS Apple Silicon
./tools/build-platform.sh 0.1.0 macos-aarch64

# Build Windows
./tools/build-platform.sh 0.1.0 windows-x86_64

# Build ALL platforms
./tools/build-all-platforms.sh 0.1.0
```

### Release Commands (Build + Upload)

```bash
# Current OS only
./tools/release.sh v0.1.0

# All platforms (Docker)
./tools/release-multiplatform.sh v0.1.0

# Specific platforms only
./tools/release-multiplatform.sh v0.1.0 --platforms=linux-x86_64,darwin-aarch64

# Draft release
./tools/release-multiplatform.sh v0.1.0 --draft

# Pre-release
./tools/release-multiplatform.sh v0.2.0-beta --prerelease
```

## Available Platforms

- `linux-x86_64` - Linux Intel/AMD 64-bit
- `linux-aarch64` - Linux ARM 64-bit
- `darwin-x86_64` - macOS Intel
- `darwin-aarch64` - macOS Apple Silicon
- `windows-x86_64` - Windows 64-bit

---

## Output Structure

### New Scripts (build-platform.sh / build-all-platforms.sh)

```
dist/
└── 0.1.0/                                    # Version directory
    ├── kalam-0.1.0-linux-x86_64
    ├── kalam-0.1.0-linux-x86_64.tar.gz
    ├── kalamdb-server-0.1.0-linux-x86_64
    ├── kalamdb-server-0.1.0-linux-x86_64.tar.gz
    ├── kalam-0.1.0-macos-aarch64
    ├── kalam-0.1.0-macos-aarch64.tar.gz
    ├── kalamdb-server-0.1.0-macos-aarch64
    ├── kalamdb-server-0.1.0-macos-aarch64.tar.gz
    ├── SHA256SUMS                            # Combined checksums
    └── SHA256SUMS-<platform>                 # Per-platform checksums
```

### Old Scripts (release.sh / release-multiplatform.sh)

```
dist/
├── kalam-cli-v0.1.0-<platform>
├── kalamdb-server-v0.1.0-<platform>
├── Archives (.tar.gz or .zip)
└── SHA256SUMS
```

---

## GitHub Release Commands

```bash
# View releases
gh release list --repo jamals86/KalamDB

# Delete a release
gh release delete v0.1.0 --repo jamals86/KalamDB --yes

# Manual upload after build-all-platforms.sh
gh release create v0.1.0 ./dist/0.1.0/* --title "Release v0.1.0"
```

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Docker not found | Install Docker Desktop and start it |
| gh not found | Install GitHub CLI |
| Not authenticated | Run `gh auth login` |
| Permission denied | `chmod +x tools/*.sh` |
| macOS build fails | Use `--platforms` to skip macOS |
| Out of disk space | `docker system prune -a` |

## First Time Setup

### For Build-Only Scripts

```bash
# 1. Navigate to project
cd KalamDB

# 2. Build for current platform
./tools/build-platform.sh 0.0.1-test macos-aarch64

# 3. Test the binary
./dist/0.0.1-test/kalam-0.0.1-test-macos-aarch64 --version

# 4. Build all platforms (if needed)
./tools/build-all-platforms.sh 0.0.1-test
```

### For Release Scripts (with GitHub)

```bash
# 1. Navigate to project
cd KalamDB

# 2. Pre-build Docker image (multiplatform only, 5-10 min first time)
./tools/build-docker-image.sh

# 3. Create a draft release to test
./tools/release-multiplatform.sh v0.0.1-test --draft

# 4. Verify on GitHub
https://github.com/jamals86/KalamDB/releases

# 5. Delete test release
gh release delete v0.0.1-test --repo jamals86/KalamDB --yes
```

## Release Checklist

- [ ] Update version in `Cargo.toml` files
- [ ] Update CHANGELOG.md
- [ ] Commit and push changes
- [ ] Create git tag: `git tag v0.1.0`
- [ ] Push tag: `git push origin v0.1.0`
- [ ] Run release script: `./tools/release-multiplatform.sh v0.1.0`
- [ ] Verify release on GitHub
- [ ] Test download and installation
- [ ] Announce release

## Need Help?

See detailed documentation in:
- `tools/README.md` - Complete guide
- `tools/ARCHITECTURE.md` - Technical details
