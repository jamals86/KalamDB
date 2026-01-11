use anyhow::Result;
use base64::Engine;

/// A near-production HTTP server instance for tests.
///
/// Uses the real `kalamdb_server::lifecycle::bootstrap()` and `run_for_tests()` wiring.
pub struct HttpTestServer {
    _temp_dir: tempfile::TempDir,
    pub base_url: String,
    pub config: kalamdb_commons::config::ServerConfig,
    running: kalamdb_server::lifecycle::RunningTestHttpServer,
}

impl HttpTestServer {
    /// Build a Basic auth header value for localhost requests.
    ///
    /// Example: `Authorization: Basic <base64(username:password)>`
    pub fn basic_auth_header(username: &str, password: &str) -> String {
        let raw = format!("{}:{}", username, password);
        let b64 = base64::engine::general_purpose::STANDARD.encode(raw);
        format!("Basic {}", b64)
    }

    pub async fn shutdown(self) {
        self.running.shutdown().await;
    }
}

/// Start a near-production HTTP server on a random available port.
///
/// This is intended for integration tests that want to use `reqwest`/WebSocket
/// clients against a real server instance.
pub async fn start_http_test_server() -> Result<HttpTestServer> {
    let temp_dir = tempfile::TempDir::new()?;

    let mut config = kalamdb_commons::config::ServerConfig::default();
    config.server.host = "127.0.0.1".to_string();
    config.server.port = 0;
    config.server.ui_path = None;
    config.storage.data_path = temp_dir.path().join("data").to_string_lossy().into_owned();

    // Match production behavior: set env var early so auth uses the same secret.
    if std::env::var("KALAMDB_JWT_SECRET").is_err() {
        std::env::set_var("KALAMDB_JWT_SECRET", &config.auth.jwt_secret);
    }

    let (components, app_context) = kalamdb_server::lifecycle::bootstrap(&config).await?;
    let running = kalamdb_server::lifecycle::run_for_tests(&config, components, app_context).await?;

    let base_url = running.base_url.clone();

    Ok(HttpTestServer {
        _temp_dir: temp_dir,
        base_url,
        config,
        running,
    })
}

/// Run a test closure against a freshly started HTTP test server, then shut it down.
///
/// This keeps tests concise and prevents forgetting `shutdown()`.
pub async fn with_http_test_server<T, F>(f: F) -> Result<T>
where
    F: for<'a> FnOnce(
        &'a HttpTestServer,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + 'a>>,
{
    let server = start_http_test_server().await?;
    let result = f(&server).await;
    server.shutdown().await;
    result
}
