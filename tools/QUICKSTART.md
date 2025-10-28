# Quick Release Guide

## Prerequisites ✓

```bash
# 1. Install Docker
https://docs.docker.com/get-docker/

# 2. Install GitHub CLI
winget install --id GitHub.cli   # Windows
brew install gh                   # macOS
# See: https://cli.github.com/

# 3. Authenticate
gh auth login
```

## Create a Release

### All Platforms (Recommended)

```bash
./tools/release-multiplatform.sh v0.1.0
```

### Specific Platforms

```bash
./tools/release-multiplatform.sh v0.1.0 --platforms=linux-x86_64,darwin-aarch64
```

### Draft Release

```bash
./tools/release-multiplatform.sh v0.1.0 --draft
```

### Pre-release

```bash
./tools/release-multiplatform.sh v0.2.0-beta --prerelease
```

## Available Platforms

- `linux-x86_64` - Linux Intel/AMD 64-bit
- `linux-aarch64` - Linux ARM 64-bit
- `darwin-x86_64` - macOS Intel
- `darwin-aarch64` - macOS Apple Silicon
- `windows-x86_64` - Windows 64-bit

## Output

All binaries go to `dist/`:
- `kalam-cli-<version>-<platform>(.exe)`
- `kalamdb-server-<version>-<platform>(.exe)`
- Archives (`.tar.gz` or `.zip`)
- `SHA256SUMS`

## Common Commands

```bash
# Pre-build Docker image (optional but recommended)
./tools/build-docker-image.sh

# Single platform release (current OS only)
./tools/release.sh v0.1.0

# View GitHub releases
gh release list --repo jamals86/KalamDB

# Delete a release
gh release delete v0.1.0 --repo jamals86/KalamDB --yes
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

```bash
# 1. Navigate to project
cd KalamDB

# 2. Build Docker image (5-10 min first time)
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
