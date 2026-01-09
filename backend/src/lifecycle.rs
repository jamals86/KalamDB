//! Server lifecycle management helpers.
//!
//! This module encapsulates the heavy lifting previously handled directly
//! in `main.rs`: bootstrapping databases and services, wiring the HTTP
//! server, and coordinating graceful shutdown.

use crate::middleware;
use crate::routes;
use crate::ServerConfig;
use actix_web::{web, App, HttpServer};
use anyhow::Result;
use kalamdb_api::handlers::AuthConfig;
use kalamdb_api::rate_limiter::{RateLimitConfig, RateLimiter};
use kalamdb_commons::{AuthType, Role, StorageId, StorageMode, UserId};
use kalamdb_core::live::ConnectionsManager;
use kalamdb_core::live_query::LiveQueryManager;
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_store::RocksDBBackend;
use kalamdb_store::RocksDbInit;
use log::debug;
use log::{info, warn};
use std::sync::Arc;

/// Aggregated application components that need to be shared across the
/// HTTP server and shutdown handling.
pub struct ApplicationComponents {
    pub session_factory: Arc<DataFusionSessionFactory>,
    pub sql_executor: Arc<SqlExecutor>,
    pub rate_limiter: Arc<RateLimiter>,
    pub live_query_manager: Arc<LiveQueryManager>,
    pub user_repo: Arc<dyn kalamdb_auth::UserRepository>,
    pub connection_registry: Arc<ConnectionsManager>,
}

/// Initialize RocksDB, DataFusion, services, rate limiter, and flush scheduler.
pub async fn bootstrap(
    config: &ServerConfig,
) -> Result<(
    ApplicationComponents,
    Arc<kalamdb_core::app_context::AppContext>,
)> {
    let bootstrap_start = std::time::Instant::now();

    // Initialize RocksDB
    let phase_start = std::time::Instant::now();
    let db_path = std::path::PathBuf::from(&config.storage.rocksdb_path);
    std::fs::create_dir_all(&db_path)?;

    let db_init = RocksDbInit::new(db_path.to_str().unwrap(), config.storage.rocksdb.clone());
    let db = db_init.open()?;
    info!(
        "RocksDB initialized at {} ({:.2}ms)",
        db_path.display(),
        phase_start.elapsed().as_secs_f64() * 1000.0
    );

    // Initialize RocksDB backend with performance settings from config
    // sync_writes=false (default) gives 10-100x better write throughput
    // WAL is still enabled so data is safe from crashes (only ~1s of data could be lost)
    let backend = Arc::new(RocksDBBackend::with_options(
        db,
        config.storage.rocksdb.sync_writes,
        config.storage.rocksdb.disable_wal,
    ));
    if !config.storage.rocksdb.sync_writes {
        debug!("RocksDB async writes enabled (sync_writes=false) for high throughput");
    }

    // Initialize core stores from generic backend (uses kalamdb_store::StorageBackend)
    // Phase 5: AppContext now creates all dependencies internally!
    // Uses constants from kalamdb_commons for table prefixes
    let phase_start = std::time::Instant::now();
    
    // Node ID: use cluster.node_id (u64) if cluster mode, otherwise default to 1 for standalone
    let node_id = if let Some(cluster) = &config.cluster {
        kalamdb_commons::NodeId::new(cluster.node_id)
    } else {
        kalamdb_commons::NodeId::new(1) // Standalone mode uses node ID 1
    };
    
    let app_context = kalamdb_core::app_context::AppContext::init(
        backend.clone(),
        node_id,
        config.storage.default_storage_path.clone(),
        config.clone(), // ServerConfig needs to be cloned for Arc storage in AppContext
    );
    info!(
        "AppContext initialized with all stores, managers, registries, and providers ({:.2}ms)",
        phase_start.elapsed().as_secs_f64() * 1000.0
    );

    // Start the executor (Raft cluster in cluster mode, no-op in standalone)
    let phase_start = std::time::Instant::now();
    if app_context.executor().is_cluster_mode() {
        info!("╔═══════════════════════════════════════════════════════════════════╗");
        info!("║               Starting Raft Cluster                               ║");
        info!("╚═══════════════════════════════════════════════════════════════════╝");
        info!("Node ID: {}", app_context.executor().node_id());
        info!("Starting Raft networking and group initialization...");
        
        app_context.executor().start().await
            .map_err(|e| anyhow::anyhow!("Failed to start Raft cluster: {}", e))?;
        info!("✓ Raft networking started successfully");
        
        // Auto-bootstrap: The node with the lowest node_id among available nodes becomes the bootstrap node.
        // For single-node clusters or when this is node_id=1, always bootstrap.
        // For multi-node clusters, node_id=1 is the designated bootstrap node.
        let should_bootstrap = config.cluster.as_ref().map(|c| {
            // Bootstrap if: single-node mode (no peers) OR this is node_id=1 (designated bootstrap)
            c.peers.is_empty() || c.node_id == 1
        }).unwrap_or(false);
        
        if should_bootstrap {
            let has_peers = config.cluster.as_ref().map(|c| !c.peers.is_empty()).unwrap_or(false);
            if has_peers {
                info!("Node {} is the designated bootstrap node - initializing cluster with peers", 
                    config.cluster.as_ref().map(|c| c.node_id).unwrap_or(1));
            } else {
                info!("No peers configured - initializing as single-node cluster");
            }
            app_context.executor().initialize_cluster().await
                .map_err(|e| anyhow::anyhow!("Failed to initialize cluster: {}", e))?;
            info!("✓ Cluster initialized successfully");
        } else {
            let peer_count = config.cluster.as_ref().map(|c| c.peers.len()).unwrap_or(0);
            let node_id = config.cluster.as_ref().map(|c| c.node_id).unwrap_or(0);
            info!("Node {} joining cluster with {} peers - waiting for leader (node_id=1 is bootstrap)", node_id, peer_count);
            // Non-bootstrap nodes wait for the bootstrap node (node_id=1) to initialize the cluster,
            // then they will be added as learners and promoted to voters
        }
        
        info!(
            "✓ Raft cluster started ({:.2}ms)",
            phase_start.elapsed().as_secs_f64() * 1000.0
        );
        info!("───────────────────────────────────────────────────────────────────");
    } else {
        info!("Standalone mode - no Raft cluster to start");
    }

    // Manifest cache uses lazy loading via get_or_load() - no pre-loading needed
    // When a manifest is needed, get_or_load() checks hot cache → RocksDB → returns None
    // This avoids loading manifests that may never be accessed

    // Initialize system tables and verify schema version (Phase 10 Phase 7, T075-T079)
    let phase_start = std::time::Instant::now();
    kalamdb_system::initialize_system_tables(backend.clone()).await?;
    info!(
        "System tables initialized with schema version tracking ({:.2}ms)",
        phase_start.elapsed().as_secs_f64() * 1000.0
    );

    // Start JobsManager run loop (Phase 9, T163)
    let job_manager = app_context.job_manager();
    let max_concurrent = config.jobs.max_concurrent;
    tokio::spawn(async move {
        info!(
            "Starting JobsManager run loop with max {} concurrent jobs",
            max_concurrent
        );
        if let Err(e) = job_manager.run_loop(max_concurrent as usize).await {
            log::error!("JobsManager run loop failed: {}", e);
        }
    });
    info!("JobsManager background task spawned");

    // Seed default storage if necessary (using SystemTablesRegistry)
    let phase_start = std::time::Instant::now();
    let storages_provider = app_context.system_tables().storages();
    let existing_storages = storages_provider.scan_all_storages()?;
    let storage_count = existing_storages.num_rows();

    //TODO: Extract as a separate function create_default_storage_if_needed
    if storage_count == 0 {
        info!("No storages found, creating default 'local' storage");
        let now = chrono::Utc::now().timestamp_millis();
        let default_storage = kalamdb_commons::system::Storage {
            storage_id: StorageId::from("local"),
            storage_name: "Local Filesystem".to_string(),
            description: Some("Default local filesystem storage".to_string()),
            storage_type: kalamdb_commons::models::StorageType::Filesystem,
            base_directory: config.storage.default_storage_path.clone(), // Need clone for Storage struct
            credentials: None,
            config_json: None,
            shared_tables_template: config.storage.shared_tables_template.clone(), // Need clone for Storage struct
            user_tables_template: config.storage.user_tables_template.clone(), // Need clone for Storage struct
            created_at: now,
            updated_at: now,
        };
        storages_provider.insert_storage(default_storage)?;
        info!("Default 'local' storage created successfully");
    } else {
        info!("Found {} existing storage(s)", storage_count);
    }
    info!(
        "Storage initialization completed ({:.2}ms)",
        phase_start.elapsed().as_secs_f64() * 1000.0
    );

    // Get references from AppContext for services that need them
    let live_query_manager = app_context.live_query_manager();
    let session_factory = app_context.session_factory();
    // session_factory obtained above

    // Get system table providers for job executor and flush scheduler
    let users_provider = app_context.system_tables().users();
    let user_repo: Arc<dyn kalamdb_auth::UserRepository> = Arc::new(
        kalamdb_api::repositories::CoreUsersRepo::new(users_provider),
    );

    // SqlExecutor now uses AppContext directly
    let phase_start = std::time::Instant::now();
    let sql_executor = Arc::new(SqlExecutor::new(
        app_context.clone(),
        config.auth.enforce_password_complexity,
    ));

    app_context.set_sql_executor(sql_executor.clone());
    live_query_manager.set_sql_executor(sql_executor.clone());

    info!(
        "SqlExecutor initialized with DROP TABLE support, storage registry, job manager, and table registration"
    );

    sql_executor.load_existing_tables().await?;
    info!(
        "Existing tables loaded and registered with DataFusion ({:.2}ms)",
        phase_start.elapsed().as_secs_f64() * 1000.0
    );

    // Rate limiter
    let rate_limit_config = RateLimitConfig {
        max_queries_per_user: config.rate_limit.max_queries_per_sec,
        max_subscriptions_per_user: config.rate_limit.max_subscriptions_per_user,
        max_messages_per_connection: config.rate_limit.max_messages_per_sec,
        window: std::time::Duration::from_secs(1),
    };
    let rate_limiter = Arc::new(RateLimiter::with_config(rate_limit_config));
    info!(
        "Rate limiter initialized ({} queries/sec, {} messages/sec, {} max subscriptions)",
        config.rate_limit.max_queries_per_sec,
        config.rate_limit.max_messages_per_sec,
        config.rate_limit.max_subscriptions_per_user
    );

    // Connection Manager for WebSocket connections - use the one from AppContext
    // This is CRITICAL: the same ConnectionsManager must be used by both:
    // 1. ws_handler (for registering connections and marking authentication)
    // 2. LiveQueryManager's SubscriptionService (for checking connection state during subscription)
    // 
    // Previously, lifecycle.rs created a NEW ConnectionsManager which caused the error:
    // "Connection not found" when trying to register subscriptions because the connection
    // was registered in one manager but looked up in another.
    let connection_registry = app_context.connection_registry();
    let client_timeout = config.websocket.client_timeout_secs.unwrap_or(10);
    let auth_timeout = config.websocket.auth_timeout_secs.unwrap_or(3);
    let heartbeat_interval = 5; // Fixed at 5s in AppContext
    info!(
        "ConnectionsManager initialized (client_timeout={}s, auth_timeout={}s, heartbeat_interval={}s)",
        client_timeout,
        auth_timeout,
        heartbeat_interval
    );

    // Phase 9: All job scheduling now handled by JobsManager
    // Stream eviction is now handled by JobsManager.run_loop() - no separate scheduler needed
    // JobsManager periodically checks for STREAM tables with TTL and creates eviction jobs
    // Crash recovery handled by JobsManager.recover_incomplete_jobs() in run_loop
    // Flush scheduling via FLUSH TABLE/FLUSH ALL TABLES commands

    info!("Job management delegated to JobsManager (already running in background, handles stream eviction)");

    // Get users provider for system user initialization
    let phase_start = std::time::Instant::now();
    let users_provider_for_init = app_context.system_tables().users();

    // T125-T127: Create default system user on first startup
    create_default_system_user(users_provider_for_init.clone()).await?;

    // Security warning: Check if remote access is enabled with empty root password
    check_remote_access_security(config, users_provider_for_init).await?;
    info!(
        "User initialization completed ({:.2}ms)",
        phase_start.elapsed().as_secs_f64() * 1000.0
    );

    let components = ApplicationComponents {
        session_factory,
        sql_executor,
        rate_limiter,
        live_query_manager,
        user_repo,
        connection_registry,
    };

    info!(
        "🚀 Server bootstrap completed in {:.2}ms",
        bootstrap_start.elapsed().as_secs_f64() * 1000.0
    );

    Ok((components, app_context))
}

/// Start the HTTP server and manage graceful shutdown.
pub async fn run(
    config: &ServerConfig,
    components: ApplicationComponents,
    app_context: Arc<kalamdb_core::app_context::AppContext>,
) -> Result<()> {
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Starting HTTP server on {}", bind_addr);
    info!("Endpoints: POST /v1/api/sql, GET /v1/ws");

    // Log server configuration for debugging
    info!(
        "Server config: workers={}, max_connections={}, backlog={}, blocking_threads={}, body_limit={}MB",
        if config.server.workers == 0 {
            num_cpus::get()
        } else {
            config.server.workers
        },
        config.performance.max_connections,
        config.performance.backlog,
        config.performance.worker_max_blocking_threads,
        config.rate_limit.request_body_limit_bytes / (1024 * 1024)
    );

    if config.rate_limit.enable_connection_protection {
        info!(
            "Connection protection: max_conn_per_ip={}, max_req_per_ip_per_sec={}, ban_duration={}s",
            config.rate_limit.max_connections_per_ip,
            config.rate_limit.max_requests_per_ip_per_sec,
            config.rate_limit.ban_duration_seconds
        );
    } else {
        warn!("Connection protection is DISABLED - server may be vulnerable to DoS attacks");
    }

    // Get JobsManager for graceful shutdown
    let job_manager_shutdown = app_context.job_manager();
    let shutdown_timeout_secs = config.shutdown.flush.timeout;

    let session_factory = components.session_factory.clone();
    let sql_executor = components.sql_executor.clone();
    let rate_limiter = components.rate_limiter.clone();
    let live_query_manager = components.live_query_manager.clone();
    let user_repo = components.user_repo.clone();
    let connection_registry = components.connection_registry.clone();

    // Create connection protection middleware from config
    let connection_protection = middleware::ConnectionProtection::from_server_config(config);
    
    // Build CORS middleware from config (uses actix-cors)
    let cors_config = config.clone();

    let app_context_for_handler = app_context.clone();
    let connection_registry_for_handler = connection_registry.clone();
    
    // Create auth config from server config (reads jwt_secret from server.toml)
    let auth_config = AuthConfig::from_server_config(config);
    let ui_path = config.server.ui_path.clone();
    
    // Log UI serving status
    if kalamdb_api::routes::is_embedded_ui_available() {
        info!("Admin UI enabled at /ui (embedded in binary)");
    } else if let Some(ref path) = ui_path {
        info!("Admin UI enabled at /ui (serving from filesystem: {})", path);
    } else {
        info!("Admin UI not available (run 'npm run build' in ui/ and rebuild server)");
    }
    
    let server = HttpServer::new(move || {
        let mut app = App::new()
            // Connection protection (first middleware - drops bad requests early)
            .wrap(connection_protection.clone())
            // Standard middleware
            .wrap(middleware::request_logger())
            .wrap(middleware::build_cors_from_config(&cors_config))
            .app_data(web::Data::new(app_context_for_handler.clone()))
            .app_data(web::Data::new(session_factory.clone()))
            .app_data(web::Data::new(sql_executor.clone()))
            .app_data(web::Data::new(rate_limiter.clone()))
            .app_data(web::Data::new(live_query_manager.clone()))
            .app_data(web::Data::new(user_repo.clone()))
            .app_data(web::Data::new(connection_registry_for_handler.clone()))
            .app_data(web::Data::new(auth_config.clone()))
            .configure(routes::configure);
        
        // Add UI routes - prefer embedded, fallback to filesystem path
        if kalamdb_api::routes::is_embedded_ui_available() {
            app = app.configure(kalamdb_api::routes::configure_embedded_ui_routes);
        } else if let Some(ref path) = ui_path {
            let path = path.clone();
            app = app.configure(move |cfg| {
                kalamdb_api::routes::configure_ui_routes(cfg, &path);
            });
        }
        
        app
    })
    // Set backlog BEFORE bind() - this affects the listen queue size
    .backlog(config.performance.backlog);
    
    // Bind with HTTP/2 support if enabled, otherwise use HTTP/1.1 only
    let server = if config.server.enable_http2 {
        info!("HTTP/2 support enabled (h2c - HTTP/2 cleartext)");
        server.bind_auto_h2c(&bind_addr)?
    } else {
        info!("HTTP/1.1 only mode");
        server.bind(&bind_addr)?
    };
    
    let server = server
    .workers(if config.server.workers == 0 {
        num_cpus::get()
    } else {
        config.server.workers
    })
    // Per-worker max concurrent connections (default: 25000)
    .max_connections(config.performance.max_connections)
    // Blocking thread pool size per worker for RocksDB and CPU-intensive ops
    .worker_max_blocking_threads(config.performance.worker_max_blocking_threads)
    // Enable HTTP keep-alive for connection reuse (improves throughput 2-3x)
    // Connections stay open for reuse, reducing TCP handshake overhead
    .keep_alive(std::time::Duration::from_secs(config.performance.keepalive_timeout))
    // Client must send request headers within this time
    .client_request_timeout(std::time::Duration::from_secs(config.performance.client_request_timeout))
    // Allow time for graceful connection shutdown
    .client_disconnect_timeout(std::time::Duration::from_secs(config.performance.client_disconnect_timeout))
    .run();

    let server_handle = server.handle();
    let server_task = tokio::spawn(server);

    tokio::select! {
        result = server_task => {
            if let Err(e) = result {
                log::error!("Server task failed: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, initiating graceful shutdown...");

            // Stop accepting new HTTP connections
            server_handle.stop(true).await;

            // Gracefully shutdown WebSocket connections
            info!("Shutting down WebSocket connections...");
            connection_registry.shutdown(std::time::Duration::from_secs(5)).await;

            info!(
                "Waiting up to {}s for active jobs to complete...",
                shutdown_timeout_secs
            );

            // Signal shutdown to JobsManager (non-async, uses AtomicBool)
            job_manager_shutdown.shutdown();

            // Wait for active jobs with timeout
            let timeout = std::time::Duration::from_secs(shutdown_timeout_secs as u64);
            let start = std::time::Instant::now();

            loop {
                // Check for Running jobs
                let filter = kalamdb_commons::system::JobFilter {
                    status: Some(kalamdb_commons::JobStatus::Running),
                    ..Default::default()
                };

                match job_manager_shutdown.list_jobs(filter).await {
                    Ok(jobs) if jobs.is_empty() => {
                        info!("All jobs completed successfully");
                        break;
                    }
                    Ok(jobs) => {
                        if start.elapsed() > timeout {
                            warn!("Timeout waiting for {} active jobs to complete", jobs.len());
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                    Err(e) => {
                        log::error!("Error checking job status during shutdown: {}", e);
                        break;
                    }
                }
            }
            
            // Ensure all file descriptors are released
            info!("Performing cleanup to release file descriptors...");
            
            // Shutdown the executor (Raft cluster shutdown in cluster mode)
            if app_context.executor().is_cluster_mode() {
                info!("Shutting down Raft cluster...");
                if let Err(e) = app_context.executor().shutdown().await {
                    log::error!("Error shutting down Raft cluster: {}", e);
                }
            }
            
            drop(components);
            drop(app_context);
            
            info!("Graceful shutdown complete");
        }
    }

    info!("Server shutdown complete");
    Ok(())
}

/// T125-T127: Create default system user on database initialization
///
/// Creates a default system user with:
/// - Username: "root" (AUTH::DEFAULT_SYSTEM_USERNAME)
/// - Auth type: Internal (localhost-only by default)
/// - Role: System
/// - Random password for emergency remote access
///
/// Periodic scheduler for stream table TTL eviction runs in background,
/// checking all STREAM tables and creating eviction jobs for tables
/// that have TTL configured.
///
/// On first startup, logs the credentials to stdout for the administrator to save.
///
/// # Arguments
/// * `users_provider` - UsersTableProvider for system.users table
///
/// # Returns
/// Result indicating success or failure
async fn create_default_system_user(
    users_provider: Arc<kalamdb_system::UsersTableProvider>,
) -> Result<()> {
    use kalamdb_commons::constants::AuthConstants;
    use kalamdb_commons::system::User;

    // Check if root user already exists
    let existing_user = users_provider.get_user_by_username(AuthConstants::DEFAULT_SYSTEM_USERNAME);

    match existing_user {
        Ok(Some(_)) => {
            // User already exists, skip creation
            debug!(
                "System user '{}' already exists, skipping initialization",
                AuthConstants::DEFAULT_SYSTEM_USERNAME
            );
            Ok(())
        }
        Ok(None) | Err(_) => {
            // User doesn't exist, create new system user
            let user_id = UserId::root();
            let username = AuthConstants::DEFAULT_SYSTEM_USERNAME.to_string();
            let email = format!("{}@localhost", AuthConstants::DEFAULT_SYSTEM_USERNAME);
            let role = Role::System; // Highest privilege level
            let created_at = chrono::Utc::now().timestamp_millis();

            // Check for KALAMDB_ROOT_PASSWORD environment variable
            // If set, hash the password for remote access support
            // Otherwise, create with empty password for localhost-only access
            let password_hash = match std::env::var("KALAMDB_ROOT_PASSWORD") {
                Ok(password) if !password.is_empty() => {
                    // Hash the provided password for remote access
                    bcrypt::hash(&password, bcrypt::DEFAULT_COST)
                        .map_err(|e| anyhow::anyhow!("Failed to hash root password: {}", e))?
                }
                _ => {
                    // T126: Create with EMPTY password hash for localhost-only access
                    // This allows passwordless authentication from localhost (127.0.0.1, ::1)
                    // For remote access, set a password using: ALTER USER root SET PASSWORD '...'
                    String::new() // Empty = localhost-only, no password required
                }
            };
            let has_password = !password_hash.is_empty();
            
            // If password is set via env var, enable remote access
            let auth_data = if has_password {
                Some(r#"{"allow_remote":true}"#.to_string())
            } else {
                None
            };

            let user = User {
                id: user_id,
                username: username.clone().into(),
                password_hash,
                role,
                email: Some(email),
                auth_type: AuthType::Internal, // System user uses Internal auth
                auth_data,                     // allow_remote flag if password is set
                storage_mode: StorageMode::Table,
                storage_id: Some(StorageId::local()),
                failed_login_attempts: 0,
                locked_until: None,
                last_login_at: None,
                created_at,
                updated_at: created_at,
                last_seen: None,
                deleted_at: None,
            };

            users_provider.create_user(user)?;

            // T127: Log system user information to stdout
            if has_password {
                info!(
                    "✓ Created system user '{}' (remote access enabled via KALAMDB_ROOT_PASSWORD)",
                    username
                );
            } else {
                info!(
                    "✓ Created system user '{}' (localhost-only access, no password required)",
                    username
                );
                info!("  To enable remote access, set a password: ALTER USER root SET PASSWORD '...'");
                info!("  Or set KALAMDB_ROOT_PASSWORD environment variable before startup");
            }

            Ok(())
        }
    }
}

/// Check for security issues with remote access configuration
///
/// Informs users about password requirements for remote access
async fn check_remote_access_security(
    config: &ServerConfig,
    users_provider: Arc<kalamdb_system::UsersTableProvider>,
) -> Result<()> {
    use kalamdb_commons::constants::AuthConstants;

    // Check if root user exists and has empty password
    // Always show this info if root has no password, regardless of allow_remote_access setting
    if let Ok(Some(user)) =
        users_provider.get_user_by_username(AuthConstants::DEFAULT_SYSTEM_USERNAME)
    {
        if user.password_hash.is_empty() {
            // Root user has no password - this is secure for localhost-only but warn about limitations
            warn!("╔═══════════════════════════════════════════════════════════════════╗");
            warn!("║                    ⚠️  SECURITY NOTICE ⚠️                           ║");
            warn!("╠═══════════════════════════════════════════════════════════════════╣");
            warn!("║                                                                   ║");
            warn!("║  Root user has NO PASSWORD (localhost-only access enabled)       ║");
            warn!("║                                                                   ║");
            warn!("║  SECURITY ENFORCEMENT:                                           ║");
            warn!("║  • Remote authentication is BLOCKED for users with no password   ║");
            warn!("║  • Root can only connect from localhost (127.0.0.1)              ║");
            warn!("║  • This configuration is secure by design                        ║");
            warn!("║                                                                   ║");
            warn!("║  TO ENABLE REMOTE ACCESS:                                        ║");
            warn!("║  Set a strong password for the root user:                        ║");
            warn!("║     ALTER USER root SET PASSWORD 'strong-password-here';         ║");
            warn!("║                                                                   ║");
            warn!(
                "║  Note: allow_remote_access config is currently: {}               ║",
                if config.auth.allow_remote_access {
                    "ENABLED "
                } else {
                    "DISABLED"
                }
            );
            warn!("║  (Remote access still requires password for system users)        ║");
            warn!("║                                                                   ║");
            warn!("╚═══════════════════════════════════════════════════════════════════╝");
        }
    }

    Ok(())
}
