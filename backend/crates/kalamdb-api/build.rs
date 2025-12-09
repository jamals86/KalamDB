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
    // Only build UI for release builds
    let profile = std::env::var("PROFILE").unwrap_or_default();
    if profile != "release" {
        // For debug builds, just ensure the dist folder exists with a placeholder
        ensure_ui_dist_exists();
        return;
    }

    // Check if we should skip UI build (for CI or when UI is pre-built)
    if std::env::var("SKIP_UI_BUILD").is_ok() {
        println!("cargo:warning=Skipping UI build (SKIP_UI_BUILD is set)");
        ensure_ui_dist_exists();
        return;
    }

    let ui_dir = std::path::Path::new("../../../ui");
    if !ui_dir.exists() {
        println!("cargo:warning=UI directory not found, skipping UI build");
        ensure_ui_dist_exists();
        return;
    }

    // Check if npm is available
    let npm_check = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", "npm", "--version"]).output()
    } else {
        Command::new("npm").arg("--version").output()
    };

    if npm_check.is_err() || !npm_check.as_ref().unwrap().status.success() {
        println!("cargo:warning=npm not found, skipping UI build");
        ensure_ui_dist_exists();
        return;
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

        if let Err(e) = install_status {
            println!("cargo:warning=Failed to install UI dependencies: {}", e);
            ensure_ui_dist_exists();
            return;
        }
    }

    // Run npm run build
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
            println!("cargo:warning=UI build failed with status: {}", status);
            ensure_ui_dist_exists();
        }
        Err(e) => {
            println!("cargo:warning=Failed to run UI build: {}", e);
            ensure_ui_dist_exists();
        }
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
