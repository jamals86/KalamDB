# kalam_link — Developer Notes

This document is for contributors and developers working on the `kalam_link` Dart SDK itself.
For usage documentation, see [README.md](README.md).

## Architecture

The Dart SDK is a thin Dart layer over the `kalam-link` Rust core, bridged via
[flutter_rust_bridge](https://cjycode.com/flutter_rust_bridge/) v2.

```
Flutter App
  └─ kalam_link (Dart package)         ← this package
      └─ Generated FRB bindings          ← lib/src/generated/ (auto-generated, do not edit)
          └─ kalam-link-dart (Rust)      ← link/kalam-link-dart/  (bridge crate)
              └─ kalam-link              ← link/src/  (core client library)
```

The FRB codegen tool reads `#[frb]`-annotated Rust functions in `kalam-link-dart` and generates
`lib/src/generated/frb_generated.dart` (+ IO and Web variants). All FFI, async dispatch, and type
marshalling is handled automatically.

## Prerequisites

| Tool | Version |
|------|---------|
| Flutter SDK | >= 3.10 |
| Dart SDK | >= 3.3.0 |
| Rust toolchain | stable (via `rustup`) |
| `flutter_rust_bridge_codegen` | v2.x |
| Android NDK (for Android builds) | as required by Flutter |

Install the FRB codegen tool:

```bash
dart pub global activate flutter_rust_bridge
```

## Generating Dart Bindings

Run this whenever you change Rust-side API in `link/kalam-link-dart`:

```bash
cd link/kalam-link-dart
flutter_rust_bridge_codegen generate
```

Generated files land in `link/sdks/dart/lib/src/generated/` — commit them together with the
Rust changes.

## Local Scripts

From `link/sdks/dart`:

```bash
./build.sh          # flutter pub get + optional FRB generation + dart analyze
./test.sh           # flutter pub get + dart analyze + flutter test
./publish.sh        # validate + publish to pub.dev (requires clean git state)
./publish.sh --dry-run   # validate only, no upload
```

Control FRB regeneration:

```bash
FRB_GENERATE=always ./build.sh   # always regenerate
FRB_GENERATE=never  ./build.sh   # skip regeneration
```

## Running Tests

### Unit tests

```bash
flutter test
```

### Live server integration tests

Requires a running KalamDB instance (default: `http://localhost:8080`, user `admin`, pass `kalamdb123`):

```bash
KALAM_INTEGRATION_TEST=1 \
KALAM_URL=http://localhost:8080 \
KALAM_USER=admin \
KALAM_PASS=kalamdb123 \
flutter test test/live_server_test.dart
```

Notes:
- Integration tests create and drop temporary tables — safe to run against a dev instance.
- Set `KALAM_BUILD_DART_BRIDGE=1` (default) to auto-build the Rust bridge before tests if needed.

## Publishing

```bash
cd link/sdks/dart
./publish.sh            # full publish
./publish.sh --dry-run  # validate only
```

The script checks for a clean git state, runs `dart analyze`, optionally runs tests, then publishes
to [pub.dev](https://pub.dev/packages/kalam_link).

Before a new release:
1. Bump `version` in `pubspec.yaml`
2. Add an entry to `CHANGELOG.md`
3. Commit + tag (`git tag dart-v0.x.y`)
4. Run `./publish.sh`

## Crate Layout

| Path | Purpose |
|------|---------|
| `link/src/` | Core `kalam-link` Rust library (HTTP, WebSocket, auth) |
| `link/kalam-link-dart/` | FRB bridge crate — wraps `kalam-link` with `#[frb]` annotations |
| `link/sdks/dart/lib/src/generated/` | Auto-generated Dart bindings (do not edit manually) |
| `link/sdks/dart/lib/src/` | Hand-written Dart API layer (`kalam_client.dart`, `auth.dart`, `models.dart`) |
