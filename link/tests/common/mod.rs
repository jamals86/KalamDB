use base64::Engine;
use kalamdb_configs::ServerConfig;
use kalamdb_server::lifecycle::RunningTestHttpServer;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tempfile::TempDir;
use tokio::runtime::Runtime;

static SERVER_URL: OnceLock<String> = OnceLock::new();
static AUTO_TEST_SERVER: OnceLock<Mutex<Option<AutoTestServer>>> = OnceLock::new();
static AUTO_TEST_RUNTIME: OnceLock<Runtime> = OnceLock::new();

struct AutoTestServer {
    base_url: String,
    _temp_dir: TempDir,
    data_dir: PathBuf,
    _running: RunningTestHttpServer,
}

fn should_auto_start_test_server() -> bool {
    if std::env::var("KALAMDB_SERVER_URL").is_ok() {
        return false;
    }

    std::env::var("KALAMDB_AUTO_START_TEST_SERVER")
        .map(|value| {
            let value = value.trim();
            !(value.eq_ignore_ascii_case("0") || value.eq_ignore_ascii_case("false"))
        })
        .unwrap_or(true)
}

fn url_reachable(url: &str) -> bool {
    let host_port = url
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("127.0.0.1:8080");
    std::net::TcpStream::connect(host_port).map(|_| true).unwrap_or(false)
}

fn ensure_auto_test_server() -> Option<(String, PathBuf)> {
    let server_mutex = AUTO_TEST_SERVER.get_or_init(|| Mutex::new(None));
    let mut guard = server_mutex.lock().ok()?;

    if guard.is_none() {
        let runtime = AUTO_TEST_RUNTIME.get_or_init(|| {
            Runtime::new().expect("Failed to create auto test server runtime")
        });
        match runtime.block_on(start_local_test_server()) {
            Ok(server) => {
                *guard = Some(server);
            }
            Err(err) => {
                eprintln!("Failed to auto-start test server: {}", err);
                return None;
            }
        }
    }

    guard
        .as_ref()
        .map(|server| (server.base_url.clone(), server.data_dir.clone()))
}

async fn start_local_test_server() -> Result<AutoTestServer, Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let data_path = temp_dir.path().to_path_buf();

    let mut config = ServerConfig::default();
    config.server.host = "127.0.0.1".to_string();
    config.server.port = 0;
    config.server.ui_path = None;
    config.storage.data_path = data_path.to_string_lossy().into_owned();
    config.rate_limit.max_queries_per_sec = 100000;
    config.rate_limit.max_messages_per_sec = 10000;

    let (components, app_context) = kalamdb_server::lifecycle::bootstrap_isolated(&config).await?;
    let running =
        kalamdb_server::lifecycle::run_for_tests(&config, components, app_context).await?;

    Ok(AutoTestServer {
        base_url: running.base_url.clone(),
        _temp_dir: temp_dir,
        data_dir: data_path,
        _running: running,
    })
}

pub fn server_url() -> &'static str {
    SERVER_URL
        .get_or_init(|| {
            if let Ok(url) = std::env::var("KALAMDB_SERVER_URL") {
                return url;
            }

            let default_url = "http://localhost:8080".to_string();
            if url_reachable(&default_url) {
                return default_url;
            }

            if should_auto_start_test_server() {
                if let Some((url, storage_dir)) = ensure_auto_test_server() {
                    std::env::set_var("KALAMDB_SERVER_URL", &url);
                    std::env::set_var("KALAMDB_ROOT_PASSWORD", "");
                    std::env::set_var("KALAMDB_STORAGE_DIR", storage_dir.to_string_lossy().to_string());
                    return url;
                }
            }

            default_url
        })
        .as_str()
}

pub async fn is_server_running() -> bool {
    let credentials = base64::engine::general_purpose::STANDARD.encode("root:");
    match reqwest::Client::new()
        .post(format!("{}/v1/api/sql", server_url()))
        .header("Authorization", format!("Basic {}", credentials))
        .json(&serde_json::json!({ "sql": "SELECT 1" }))
        .timeout(Duration::from_secs(2))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

pub fn websocket_url() -> String {
    let base = server_url();
    if base.starts_with("https://") {
        base.replacen("https://", "wss://", 1)
    } else {
        base.replacen("http://", "ws://", 1)
    }
}
