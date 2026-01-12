use anyhow::{Context, Result};
use base64::Engine;
use kalamdb_api::models::SqlResponse;
use once_cell::sync::Lazy;
use reqwest::header;
use reqwest::Client;
use serde_json::json;
use serde_json::Value as JsonValue;
use std::path::Path;
use std::path::PathBuf;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration, Instant};
use kalam_link::{AuthProvider, KalamLinkClient};
use kalamdb_auth::jwt_auth::{JwtClaims, generate_jwt_token};

static HTTP_TEST_SERVER_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// A near-production HTTP server instance for tests.
///
/// Uses the real `kalamdb_server::lifecycle::bootstrap()` and `run_for_tests()` wiring.
pub struct HttpTestServer {
    _temp_dir: tempfile::TempDir,
    // Cross-process lock to avoid running multiple near-production servers concurrently.
    // Some subsystems (notably Raft/bootstrap) are not safe to initialize concurrently
    // across integration test binaries.
    _global_lock: Option<std::fs::File>,
    pub base_url: String,
    #[allow(dead_code)]
    data_path: PathBuf,
    client: Client,
    root_auth_header: String,
    jwt_secret: String,
    running: kalamdb_server::lifecycle::RunningTestHttpServer,
}

fn acquire_global_http_test_server_lock() -> Result<Option<std::fs::File>> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;

        let lock_path = std::env::temp_dir().join("kalamdb-http-test-server.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("Failed to open global lock file: {}", lock_path.display()))?;

        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        if rc != 0 {
            return Err(anyhow::anyhow!(
                "Failed to acquire global lock (flock) on {}",
                lock_path.display()
            ));
        }

        Ok(Some(file))
    }

    #[cfg(not(unix))]
    {
        Ok(None)
    }
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

    /// Returns the server's isolated data directory for this test instance.
    #[allow(dead_code)]
    pub fn data_path(&self) -> &Path {
        &self.data_path
    }

    /// Returns the storage root directory (e.g. `<data_path>/storage`).
    #[allow(dead_code)]
    pub fn storage_root(&self) -> PathBuf {
        self.data_path.join("storage")
    }

    /// Returns the WebSocket URL for this server (ws://localhost:port/v1/ws)
    pub fn websocket_url(&self) -> String {
        self.base_url.replace("http://", "ws://") + "/v1/ws"
    }

    /// Returns the base URL without trailing slash
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Creates a JWT token for the specified user.
    pub fn create_jwt_token(&self, username: &str) -> String {
        // For test purposes, we assume role is 'system' for root, and 'user' for others 
        // unless we want to make it more complex. For now, matching previous behavior.
        let role = if username == "root" { "system" } else { "user" };
        let user_id = if username == "root" { "1" } else { "test-user-id" };

        let (token, _claims) = kalamdb_auth::jwt_auth::create_and_sign_token(
            user_id,
            username,
            role,
            None,
            Some(1), // 1 hour expiry
            &self.jwt_secret
        ).expect("Failed to generate JWT token");
        
        token
    }

    /// Returns a pre-configured KalamLinkClient authenticated as specified user using JWT.
    pub fn link_client(&self, username: &str) -> KalamLinkClient {
        let token = self.create_jwt_token(username);
        
        KalamLinkClient::builder()
            .base_url(self.base_url())
            .auth(AuthProvider::jwt_token(token))
            .build()
            .expect("Failed to build KalamLinkClient")
    }

    /// Execute SQL via the real HTTP API as the localhost `root` user.
    pub async fn execute_sql(&self, sql: &str) -> Result<SqlResponse> {
        self.execute_sql_with_auth(sql, &self.root_auth_header)
            .await
    }

    /// Execute a parameterized SQL query via the real HTTP API as the localhost `root` user.
    pub async fn execute_sql_with_params(&self, sql: &str, params: Vec<JsonValue>) -> Result<SqlResponse> {
        self.execute_sql_with_auth_and_params(sql, &self.root_auth_header, params)
            .await
    }

    /// Execute SQL via the real HTTP API using an explicit `Authorization` header.
    pub async fn execute_sql_with_auth(&self, sql: &str, auth_header: &str) -> Result<SqlResponse> {
        self.execute_sql_with_auth_and_params(sql, auth_header, Vec::new())
            .await
    }

    /// Execute SQL (optionally parameterized) via the real HTTP API using an explicit `Authorization` header.
    pub async fn execute_sql_with_auth_and_params(
        &self,
        sql: &str,
        auth_header: &str,
        params: Vec<JsonValue>,
    ) -> Result<SqlResponse> {
        let url = format!("{}/v1/api/sql", self.base_url);

        let mut body = json!({ "sql": sql });
        if !params.is_empty() {
            body["params"] = JsonValue::Array(params);
        }

        let response = self
            .client
            .post(url)
            .header(header::AUTHORIZATION, auth_header)
            .json(&body)
            .send()
            .await
            .context("Failed to send /v1/api/sql request")?;

        let status = response.status();
        let text = response
            .text()
            .await
            .context("Failed to read /v1/api/sql response body")?;

        serde_json::from_str::<SqlResponse>(&text).with_context(|| {
            format!(
                "Failed to parse /v1/api/sql response (http_status={}, body={})",
                status, text
            )
        })
    }

    pub async fn shutdown(self) {
        // Actix `stop(true)` is graceful: it waits for existing keep-alive connections.
        // Drop the reqwest client first so the connection pool closes immediately.
        let HttpTestServer {
            _temp_dir,
            _global_lock,
            client,
            running,
            ..
        } = self;

        drop(client);
        running.shutdown().await;
        drop(_global_lock);
        drop(_temp_dir);
    }

    async fn wait_until_ready(&self) -> Result<()> {
        // The server may bind before subsystems (notably Raft/bootstrap) are fully ready.
        // We probe a Raft-backed system table query until it succeeds or a short timeout elapses.
        let deadline = Instant::now() + Duration::from_secs(10);

        loop {
            match self
                .execute_sql("SELECT namespace_id FROM system.namespaces LIMIT 1")
                .await
            {
                Ok(resp) if resp.status.to_string() == "success" => return Ok(()),
                Ok(_) | Err(_) => {
                    if Instant::now() >= deadline {
                        return Err(anyhow::anyhow!(
                            "HTTP test server did not become ready in time"
                        ));
                    }
                    sleep(Duration::from_millis(50)).await;
                }
            }
        }
    }
}

/// Start a near-production HTTP server on a random available port.
///
/// This is intended for integration tests that want to use `reqwest`/WebSocket
/// clients against a real server instance.
#[allow(dead_code)]
pub async fn start_http_test_server() -> Result<HttpTestServer> {
    let global_lock = acquire_global_http_test_server_lock()?;

    let temp_dir = tempfile::TempDir::new()?;
    let data_path = temp_dir.path().to_path_buf();

    let mut config = kalamdb_commons::config::ServerConfig::default();
    config.server.host = "127.0.0.1".to_string();
    config.server.port = 0;
    config.server.ui_path = None;
    config.storage.data_path = data_path.to_string_lossy().into_owned();

    // Match production behavior: set env var early so auth uses the same secret.
    if std::env::var("KALAMDB_JWT_SECRET").is_err() {
        std::env::set_var("KALAMDB_JWT_SECRET", &config.auth.jwt_secret);
    }

    let (components, app_context) = kalamdb_server::lifecycle::bootstrap(&config).await?;
    let running =
        kalamdb_server::lifecycle::run_for_tests(&config, components, app_context).await?;

    let base_url = running.base_url.clone();
    let jwt_secret = config.auth.jwt_secret.clone();

    let server = HttpTestServer {
        _temp_dir: temp_dir,
        _global_lock: global_lock,
        base_url,
        data_path,
        client: Client::new(),
        root_auth_header: HttpTestServer::basic_auth_header("root", ""),
        jwt_secret,
        running,
    };

    server.wait_until_ready().await?;

    Ok(server)
}

/// Start a near-production HTTP server with a config override.
#[allow(dead_code)]
pub async fn start_http_test_server_with_config(
    override_config: impl FnOnce(&mut kalamdb_commons::config::ServerConfig),
) -> Result<HttpTestServer> {
    let global_lock = acquire_global_http_test_server_lock()?;

    let temp_dir = tempfile::TempDir::new()?;
    let data_path = temp_dir.path().to_path_buf();

    let mut config = kalamdb_commons::config::ServerConfig::default();
    config.server.host = "127.0.0.1".to_string();
    config.server.port = 0;
    config.server.ui_path = None;
    config.storage.data_path = data_path.to_string_lossy().into_owned();

    override_config(&mut config);

    if std::env::var("KALAMDB_JWT_SECRET").is_err() {
        std::env::set_var("KALAMDB_JWT_SECRET", &config.auth.jwt_secret);
    }

    let (components, app_context) = kalamdb_server::lifecycle::bootstrap(&config).await?;
    let running =
        kalamdb_server::lifecycle::run_for_tests(&config, components, app_context).await?;

    let base_url = running.base_url.clone();
    let jwt_secret = config.auth.jwt_secret.clone();

    let server = HttpTestServer {
        _temp_dir: temp_dir,
        _global_lock: global_lock,
        base_url,
        data_path,
        client: Client::new(),
        root_auth_header: HttpTestServer::basic_auth_header("root", ""),
        jwt_secret,
        running,
    };

    server.wait_until_ready().await?;
    Ok(server)
}

/// Run a test closure against a freshly started HTTP test server (with config override), then shut it down.
#[allow(dead_code)]
pub async fn with_http_test_server_config<T, F>(
    override_config: impl FnOnce(&mut kalamdb_commons::config::ServerConfig),
    f: F,
) -> Result<T>
where
    F: for<'a> FnOnce(
        &'a HttpTestServer,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + 'a>>,
{
    let _guard = HTTP_TEST_SERVER_LOCK.lock().await;

    let server = start_http_test_server_with_config(override_config).await?;
    let result = f(&server).await;
    server.shutdown().await;
    result
}

/// Run a test closure against a freshly started HTTP test server (with config override), but fail
/// fast if the test takes longer than `timeout`.
///
/// This helper still guarantees the server is shut down and the global/process locks are released.
#[allow(dead_code)]
pub async fn with_http_test_server_config_timeout<T, F>(
    timeout: Duration,
    override_config: impl FnOnce(&mut kalamdb_commons::config::ServerConfig),
    f: F,
) -> Result<T>
where
    F: for<'a> FnOnce(
        &'a HttpTestServer,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + 'a>>,
{
    let _guard = HTTP_TEST_SERVER_LOCK.lock().await;

    let server = start_http_test_server_with_config(override_config).await?;

    let result = tokio::select! {
        res = f(&server) => res,
        _ = sleep(timeout) => Err(anyhow::anyhow!("HTTP test timed out after {:?}", timeout)),
    };

    server.shutdown().await;
    result
}

/// Run a test closure against a freshly started HTTP test server, then shut it down.
///
/// This keeps tests concise and prevents forgetting `shutdown()`.
#[allow(dead_code)]
pub async fn with_http_test_server<T, F>(f: F) -> Result<T>
where
    F: for<'a> FnOnce(
        &'a HttpTestServer,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + 'a>>,
{
    // Starting multiple near-production servers concurrently is currently unsafe
    // because some global subsystems (e.g., Raft bootstrap) do not support
    // parallel initialization within the same process.
    let _guard = HTTP_TEST_SERVER_LOCK.lock().await;

    let server = start_http_test_server().await?;
    let result = f(&server).await;
    server.shutdown().await;
    result
}

/// Run a test closure against a freshly started HTTP test server, but fail fast if the test takes
/// longer than `timeout`.
///
/// This helper still guarantees the server is shut down and the global/process locks are released.
#[allow(dead_code)]
pub async fn with_http_test_server_timeout<T, F>(timeout: Duration, f: F) -> Result<T>
where
    F: for<'a> FnOnce(
        &'a HttpTestServer,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + 'a>>,
{
    let _guard = HTTP_TEST_SERVER_LOCK.lock().await;

    let server = start_http_test_server().await?;

    let result = tokio::select! {
        res = f(&server) => res,
        _ = sleep(timeout) => Err(anyhow::anyhow!("HTTP test timed out after {:?}", timeout)),
    };

    server.shutdown().await;
    result
}
