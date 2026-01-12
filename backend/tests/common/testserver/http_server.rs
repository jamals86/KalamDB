use anyhow::{Context, Result};
use base64::Engine;
use kalamdb_commons::{Role, UserId, UserName};
use once_cell::sync::Lazy;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration, Instant};
use kalam_link::{AuthProvider, KalamLinkClient};
use kalam_link::models::{QueryResponse, ResponseStatus};

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
    root_auth_header: String,
    jwt_secret: String,
    link_client_cache: Mutex<HashMap<String, KalamLinkClient>>,
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
    fn auth_provider_from_header(auth_header: &str) -> Result<AuthProvider> {
        let auth_header = auth_header.trim();

        if auth_header.is_empty() {
            return Ok(AuthProvider::none());
        }

        if let Some(encoded) = auth_header.strip_prefix("Basic ") {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .context("Failed to decode Basic auth header")?;
            let decoded = String::from_utf8(decoded).context("Basic auth decoded value was not valid UTF-8")?;

            let (username, password) = decoded
                .split_once(':')
                .context("Basic auth decoded value missing ':' separator")?;

            return Ok(AuthProvider::basic_auth(
                username.to_string(),
                password.to_string(),
            ));
        }

        if let Some(token) = auth_header.strip_prefix("Bearer ") {
            return Ok(AuthProvider::jwt_token(token.to_string()));
        }

        Err(anyhow::anyhow!(
            "Unsupported Authorization header format: {}",
            auth_header
        ))
    }

    /// Build a Basic auth header value for localhost requests.
    ///
    /// Example: `Authorization: Basic <base64(username:password)>`
    pub fn basic_auth_header(username: &UserName, password: &str) -> String {
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

    /// Creates a JWT token for the specified user with explicit user_id.
    pub fn create_jwt_token_with_id(&self, user_id: &UserId, username: &UserName, role: &Role) -> String {
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

    /// Creates a JWT token for the specified user.
    /// For test purposes, assumes role is 'system' for root, and 'user' for others.
    pub fn create_jwt_token(&self, username: &UserName) -> String {
        let role = if username.as_str() == "root" { Role::System } else { Role::User };
        // Use username as user_id for non-root users in tests
        // This allows USER tables to work correctly with partitioning
        let user_id = if username.as_str() == "root" { UserId::new("1") } else { UserId::new(username.as_str()) };

        let (token, _claims) = kalamdb_auth::jwt_auth::create_and_sign_token(
            &user_id,
            username,
            &role,
            None,
            Some(1), // 1 hour expiry
            &self.jwt_secret
        ).expect("Failed to generate JWT token");
        
        token
    }

    /// Create a new user if it doesnt exists already
    async fn ensure_user_exists(&self, username: &str, password: &str, role: &Role) -> Result<()> {
        let check_sql = format!("SELECT COUNT(*) AS user_count FROM system.users WHERE username = '{}'", username);
        let resp = self.execute_sql(&check_sql).await?;
        let user_count = resp
            .results
            .get(0)
            .and_then(|result| {
                let idx = result
                    .schema
                    .iter()
                    .find(|f| f.name == "user_count")
                    .map(|f| f.index)
                    .unwrap_or(0);

                let value = result.rows.as_ref()?.get(0)?.get(idx)?;
                match value {
                    JsonValue::Number(n) => n.as_u64(),
                    JsonValue::String(s) => s.parse::<u64>().ok(),
                    _ => None,
                }
            })
            .unwrap_or(0);

        if user_count == 0 {
            let create_sql = format!("CREATE USER {} WITH PASSWORD '{}' ROLE '{}'", username, password, role.as_str());
            self.execute_sql(&create_sql).await?;
        }

        Ok(())
    }

    /// Returns a pre-configured KalamLinkClient authenticated as specified user using JWT with explicit user_id.
    pub fn link_client_with_id(&self, user_id: &str, username: &str, role: &Role) -> KalamLinkClient {
        let user_id_typed = UserId::new(user_id);
        let username_typed = UserName::new(username);
        let token = self.create_jwt_token_with_id(&user_id_typed, &username_typed, role);
        
        KalamLinkClient::builder()
            .base_url(self.base_url())
            .auth(AuthProvider::jwt_token(token))
            .build()
            .expect("Failed to build KalamLinkClient")
    }

    /// Returns a pre-configured KalamLinkClient authenticated as specified user using JWT.
    pub fn link_client(&self, username: &str) -> KalamLinkClient {
        let username_typed = UserName::new(username);
        let token = self.create_jwt_token(&username_typed);
        
        KalamLinkClient::builder()
            .base_url(self.base_url())
            .auth(AuthProvider::jwt_token(token))
            .build()
            .expect("Failed to build KalamLinkClient")
    }

    /// Execute SQL via the real HTTP API as the localhost `root` user.
    pub async fn execute_sql(&self, sql: &str) -> Result<QueryResponse> {
        self.execute_sql_with_auth(sql, &self.root_auth_header)
            .await
    }

    /// Execute a parameterized SQL query via the real HTTP API as the localhost `root` user.
    pub async fn execute_sql_with_params(&self, sql: &str, params: Vec<JsonValue>) -> Result<QueryResponse> {
        self.execute_sql_with_auth_and_params(sql, &self.root_auth_header, params)
            .await
    }

    /// Execute SQL via the real HTTP API using an explicit `Authorization` header.
    pub async fn execute_sql_with_auth(&self, sql: &str, auth_header: &str) -> Result<QueryResponse> {
        self.execute_sql_with_auth_and_params(sql, auth_header, Vec::new())
            .await
    }

    /// Execute SQL (optionally parameterized) via the real HTTP API using an explicit `Authorization` header.
    pub async fn execute_sql_with_auth_and_params(
        &self,
        sql: &str,
        auth_header: &str,
        params: Vec<JsonValue>,
    ) -> Result<QueryResponse> {
        let resp = self
            .execute_sql_raw_with_auth_and_params_no_wait(sql, auth_header, params)
            .await?;

        // Tests frequently issue DDL immediately followed by DML. In near-production mode
        // (Raft + multi-worker HTTP server), table registration can lag very slightly.
        // Add a short, targeted wait to prevent flaky "Table does not exist" failures.
        if resp.status == ResponseStatus::Success {
            if let Some((namespace_id, table_name)) = Self::try_parse_create_table_target(sql) {
                self.wait_for_table_queryable(&namespace_id, &table_name, auth_header)
                    .await?;
            }
        }

        Ok(resp)
    }

    async fn execute_sql_raw_with_auth_and_params_no_wait(
        &self,
        sql: &str,
        auth_header: &str,
        params: Vec<JsonValue>,
    ) -> Result<QueryResponse> {
        // Important: reuse HTTP connections across calls.
        // Actix selects a worker per TCP connection; creating a fresh client per
        // query can hit different workers and expose worker-local state.
        let client = {
            let mut cache = self.link_client_cache.lock().await;
            if let Some(existing) = cache.get(auth_header) {
                existing.clone()
            } else {
                let auth = Self::auth_provider_from_header(auth_header)?;
                let built = KalamLinkClient::builder()
                    .base_url(self.base_url())
                    .auth(auth)
                    .build()
                    .context("Failed to build KalamLinkClient")?;
                cache.insert(auth_header.to_string(), built.clone());
                built
            }
        };

        let resp = client
            .execute_query(
                sql,
                if params.is_empty() { None } else { Some(params) },
                None,
            )
            .await
            .context("KalamLink execute_query failed")?;

        if resp.status == ResponseStatus::Error {
            eprintln!("HTTP SQL Error: sql={:?} error={:?}", sql, resp.error);
        }

        Ok(resp)
    }
    fn try_parse_create_table_target(sql: &str) -> Option<(String, String)> {
        // Best-effort parse for statements like:
        //   CREATE TABLE ns.table ( ... ) WITH (...)
        // We keep this intentionally simple for tests; if parsing fails we just skip the wait.
        let upper = sql.trim_start().to_ascii_uppercase();
        if !upper.starts_with("CREATE TABLE") {
            return None;
        }

        let after = sql
            .trim_start()
            .get("CREATE TABLE".len()..)?
            .trim_start();

        let ident = after
            .split_whitespace()
            .next()?
            .trim_end_matches('(')
            .trim();

        let mut parts = ident.splitn(2, '.');
        let namespace_id = parts.next()?.trim().trim_matches('"').to_string();
        let table_name = parts.next()?.trim().trim_matches('"').to_string();

        if namespace_id.is_empty() || table_name.is_empty() {
            return None;
        }

        Some((namespace_id, table_name))
    }

    async fn wait_for_table_queryable(&self, namespace_id: &str, table_name: &str, auth_header: &str) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let probe = format!("SELECT 1 AS ok FROM {}.{} LIMIT 1", namespace_id, table_name);
        let mut last_error: Option<String> = None;
        let system_probe = format!(
            "SELECT COUNT(*) AS cnt FROM system.tables WHERE namespace_id='{}' AND table_name='{}'",
            namespace_id, table_name
        );
        let mut last_system_cnt: Option<u64> = None;

        loop {
            match self
                .execute_sql_raw_with_auth_and_params_no_wait(&probe, auth_header, Vec::new())
                .await
            {
                Ok(resp) if resp.status == ResponseStatus::Success => return Ok(()),
                Ok(resp) => {
                    last_error = resp.error.as_ref().map(|e| e.message.clone());

                    let is_missing = resp
                        .error
                        .as_ref()
                        .map(|e| {
                            let m = e.message.to_lowercase();
                            m.contains("does not exist") || m.contains("not found")
                        })
                        .unwrap_or(false);

                    if !is_missing {
                        return Err(anyhow::anyhow!(
                            "CREATE TABLE probe failed with non-missing error ({}.{}): {:?}",
                            namespace_id,
                            table_name,
                            resp.error
                        ));
                    }

                    // Check if the table definition is visible in system.tables yet.
                    if let Ok(sys_resp) = self
                        .execute_sql_raw_with_auth_and_params_no_wait(
                            &system_probe,
                            &self.root_auth_header,
                            Vec::new(),
                        )
                        .await
                    {
                        if sys_resp.status == ResponseStatus::Success {
                            last_system_cnt = sys_resp
                                .results
                                .first()
                                .and_then(|r| r.row_as_map(0))
                                .and_then(|row| row.get("cnt").cloned())
                                .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok())));
                        }
                    }
                }
                Err(e) => {
                    last_error = Some(format!("{:#}", e));
                }
            }

            if Instant::now() >= deadline {
                return Err(anyhow::anyhow!(
                    "CREATE TABLE did not become queryable in time ({}.{}): last_error={:?} system.tables_cnt={:?}",
                    namespace_id,
                    table_name,
                    last_error,
                    last_system_cnt
                ));
            }

            sleep(Duration::from_millis(20)).await;
        }
    }

    pub async fn shutdown(self) {
        // Actix `stop(true)` is graceful: it waits for existing keep-alive connections.
        let HttpTestServer {
            _temp_dir,
            _global_lock,
            running,
            ..
        } = self;
        running.shutdown().await;
        drop(_global_lock);
        drop(_temp_dir);
    }

    async fn wait_until_ready(&self) -> Result<()> {
        // The server may bind before subsystems (notably Raft/bootstrap) are fully ready.
        // For tests, "ready" means:
        // - SQL HTTP API is accepting requests
        // - Raft single-node cluster has completed initialization and elected a leader
        //
        // Avoid DDL-based probes here.
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut last_error: Option<String> = None;

        loop {
            // Prefer a probe that confirms Raft leadership is established.
            // system.cluster is registered during AppContext init.
            let cluster_probe = "SELECT is_self, is_leader FROM system.cluster";
            match self.execute_sql(cluster_probe).await {
                Ok(resp) if resp.status == kalam_link::models::ResponseStatus::Success => {
                    let is_ready = resp
                        .results
                        .first()
                        .map(|r| {
                            r.rows_as_maps().iter().any(|row| {
                                let is_self = row
                                    .get("is_self")
                                    .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")))
                                    .unwrap_or(false);
                                let is_leader = row
                                    .get("is_leader")
                                    .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| s == "true")))
                                    .unwrap_or(false);
                                is_self && is_leader
                            })
                        })
                        .unwrap_or(false);

                    if is_ready {
                        return Ok(());
                    }

                    last_error = Some("cluster not ready (no self leader row yet)".to_string());
                }
                Ok(resp) => {
                    // Fall back to a trivial read-only probe if system.cluster isn't available yet.
                    last_error = resp.error.as_ref().map(|e| e.message.clone());
                    let select_probe = self.execute_sql("SELECT 1 AS ok").await;
                    if matches!(
                        select_probe,
                        Ok(ref r) if r.status == kalam_link::models::ResponseStatus::Success
                    ) {
                        // SQL is up, but cluster leadership isn't confirmed yet.
                    }
                }
                Err(e) => {
                    last_error = Some(format!("{:#}", e));
                }
            }

            if Instant::now() >= deadline {
                return Err(anyhow::anyhow!(
                    "HTTP test server did not become ready in time (last_error={:?})",
                    last_error
                ));
            }

            sleep(Duration::from_millis(50)).await;
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
        root_auth_header: HttpTestServer::basic_auth_header(&UserName::new("root"), ""),
        jwt_secret,
        link_client_cache: Mutex::new(HashMap::new()),
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
        root_auth_header: HttpTestServer::basic_auth_header(&UserName::new("root"), ""),
        jwt_secret,
        link_client_cache: Mutex::new(HashMap::new()),
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
