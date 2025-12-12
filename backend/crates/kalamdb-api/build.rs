// Build script to capture Git commit hash and build timestamp
// Sets environment variables for use in the crate at compile time
// Also builds the UI for release builds (needed before rust-embed runs)

use std::process::Command;

fn main() {
    // Build UI for release builds FIRST (before rust-embed macro runs)
    build_ui_if_release();

    // Capture Git commit hash (short version)
    let commit_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
        .trim()
        .to_string();

    // Capture build date/time in ISO 8601 format
    let build_date = chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string();

    // Capture Git branch name
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
        .trim()
        .to_string();

    // Set environment variables for use in the binary
    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", commit_hash);
    println!("cargo:rustc-env=BUILD_DATE={}", build_date);
    println!("cargo:rustc-env=GIT_BRANCH={}", branch);

    // Re-run build script if .git/HEAD changes (new commits)
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads/");
}

/// Build the UI for release builds
/// This ensures the UI is always up-to-date when building for release
fn build_ui_if_release() {
    // Only build UI for release-like builds (release, release-dist, docker profiles)
    let profile = std::env::var("PROFILE").unwrap_or_default();
    let is_release_build = profile == "release" || profile == "release-dist" || profile == "docker";
    if !is_release_build {
        // For debug builds, just ensure the dist folder exists with a placeholder
        ensure_ui_dist_exists();
        return;
    }

    // Check if we should skip UI build (for CI or when UI is pre-built)
    if std::env::var("SKIP_UI_BUILD").is_ok() {
        println!("cargo:warning=Skipping UI build (SKIP_UI_BUILD is set)");
        // Verify UI dist exists when skipping build
        let dist_dir = std::path::Path::new("../../../ui/dist");
        let index_file = dist_dir.join("index.html");
        if !index_file.exists() {
            panic!("SKIP_UI_BUILD is set but ui/dist/index.html does not exist! Build UI first or unset SKIP_UI_BUILD.");
        }
        return;
    }

    let ui_dir = std::path::Path::new("../../../ui");
    if !ui_dir.exists() {
        panic!("UI directory not found at ../../../ui - UI is required for release builds!");
    }

    // Check if npm is available
    let npm_check = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", "npm", "--version"]).output()
    } else {
        Command::new("npm").arg("--version").output()
    };

    if npm_check.is_err() || !npm_check.as_ref().unwrap().status.success() {
        panic!("npm not found - npm is required for building the UI for release builds!");
    }

    println!("cargo:warning=Building UI for release...");

    // Run npm install if node_modules doesn't exist
    let node_modules = ui_dir.join("node_modules");
    if !node_modules.exists() {
        println!("cargo:warning=Installing UI dependencies...");
        let install_status = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .args(["/C", "npm", "install"])
                .current_dir(ui_dir)
                .status()
        } else {
            Command::new("npm")
                .arg("install")
                .current_dir(ui_dir)
                .status()
        };

        match install_status {
            Ok(status) if status.success() => {}
            Ok(status) => {
                panic!("npm install failed with status: {} - UI dependencies are required for release builds!", status);
            }
            Err(e) => {
                panic!("Failed to run npm install: {} - UI dependencies are required for release builds!", e);
            }
        }
    }

    // Run npm run build
    // IMPORTANT: use `.status()` (stream output) instead of `.output()`.
    // Capturing large stdout/stderr can make builds appear "stuck" at `kalamdb-api(build)`.
    println!("cargo:warning=Running UI build (npm run build) — this can take a few minutes...");
    let build_status = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "npm", "run", "build"])
            .current_dir(ui_dir)
            .status()
    } else {
        Command::new("npm")
            .args(["run", "build"])
            .current_dir(ui_dir)
            .status()
    };

    match build_status {
        Ok(status) if status.success() => {
            println!("cargo:warning=UI build completed successfully");
        }
        Ok(status) => {
            panic!(
                "UI build failed with status: {}\n\nUI is required for release builds!",
                status
            );
        }
        Err(e) => {
            panic!("Failed to run npm build: {} - UI is required for release builds!", e);
        }
    }

    // Verify dist was created
    let dist_dir = ui_dir.join("dist");
    let index_file = dist_dir.join("index.html");
    if !index_file.exists() {
        panic!("UI build completed but ui/dist/index.html not found - UI build may have failed!");
    }

    // Rerun if UI source files change
    println!("cargo:rerun-if-changed=../../../ui/src");
    println!("cargo:rerun-if-changed=../../../ui/package.json");
    println!("cargo:rerun-if-changed=../../../ui/vite.config.ts");
    println!("cargo:rerun-if-changed=../../../link/sdks/typescript/src");
}

/// Ensure ui/dist exists with at least a placeholder file
/// This prevents rust-embed from failing when the UI hasn't been built
fn ensure_ui_dist_exists() {
    let dist_dir = std::path::Path::new("../../../ui/dist");
    if !dist_dir.exists() {
        println!("cargo:warning=Creating placeholder ui/dist directory");
        if let Err(e) = std::fs::create_dir_all(dist_dir) {
            println!("cargo:warning=Failed to create ui/dist: {}", e);
            return;
        }
        // Create a placeholder index.html
        let placeholder = dist_dir.join("index.html");
        let content = r#"<!DOCTYPE html>
<html>
<head><title>KalamDB Admin UI</title></head>
<body>
<h1>UI Not Built</h1>
<p>Run <code>npm run build</code> in the ui/ directory to build the admin UI.</p>
</body>
</html>"#;
        if let Err(e) = std::fs::write(&placeholder, content) {
            println!("cargo:warning=Failed to create placeholder index.html: {}", e);
        }
    }
}
