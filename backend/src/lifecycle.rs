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
use kalamdb_api::auth::jwt::JwtAuth;
use kalamdb_api::rate_limiter::{RateLimitConfig, RateLimiter};
use kalamdb_commons::{AuthType, Role, StorageId, StorageMode, UserId};
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
    pub jwt_auth: Arc<JwtAuth>,
    pub rate_limiter: Arc<RateLimiter>,
    pub live_query_manager: Arc<LiveQueryManager>,
    pub user_repo: Arc<dyn kalamdb_auth::UserRepository>,
}

/// Initialize RocksDB, DataFusion, services, rate limiter, and flush scheduler.
pub async fn bootstrap(config: &ServerConfig) -> Result<(ApplicationComponents, Arc<kalamdb_core::app_context::AppContext>)> {
    // Initialize RocksDB
    let db_path = std::path::PathBuf::from(&config.storage.rocksdb_path);
    std::fs::create_dir_all(&db_path)?;

    let db_init = RocksDbInit::new(db_path.to_str().unwrap(), config.storage.rocksdb.clone());
    let db = db_init.open()?;
    info!("RocksDB initialized at {}", db_path.display());

    // Initialize RocksDB backend for all components (single StorageBackend trait)
    let backend = Arc::new(RocksDBBackend::new(db.clone()));

    // Initialize core stores from generic backend (uses kalamdb_store::StorageBackend)
    // Phase 5: AppContext now creates all dependencies internally!
    // Uses constants from kalamdb_commons for table prefixes
    let app_context = kalamdb_core::app_context::AppContext::init(
        backend.clone(),
        kalamdb_commons::NodeId::new(config.server.node_id.clone()),
        config.storage.default_storage_path.clone(),
        config.clone(), // Pass ServerConfig to AppContext for centralized access
    );
    info!("AppContext initialized with all stores, managers, registries, and providers");

    // Initialize system tables and verify schema version (Phase 10 Phase 7, T075-T079)
    kalamdb_core::tables::system::initialize_system_tables(backend.clone()).await?;
    info!("System tables initialized with schema version tracking");

    // Start JobsManager run loop (Phase 9, T163)
    let job_manager = app_context.job_manager();
    let max_concurrent = config.jobs.max_concurrent;
    tokio::spawn(async move {
        info!("Starting JobsManager run loop with max {} concurrent jobs", max_concurrent);
        if let Err(e) = job_manager.run_loop(max_concurrent as usize).await {
            log::error!("JobsManager run loop failed: {}", e);
        }
    });
    info!("JobsManager background task spawned");

    // Seed default storage if necessary (using SystemTablesRegistry)
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
            storage_type: "filesystem".to_string(),
            base_directory: "".to_string(),
            credentials: None,
            shared_tables_template: config.storage.shared_tables_template.clone(),
            user_tables_template: config.storage.user_tables_template.clone(),
            created_at: now,
            updated_at: now,
        };
        storages_provider.insert_storage(default_storage)?;
        info!("Default 'local' storage created successfully");
    } else {
        info!("Found {} existing storage(s)", storage_count);
    }

    // Get references from AppContext for services that need them
    let live_query_manager = app_context.live_query_manager();
    let session_factory = app_context.session_factory();
    // session_factory obtained above
    
    // Get system table providers for job executor and flush scheduler
    let users_provider = app_context.system_tables().users();
    let user_repo: Arc<dyn kalamdb_auth::UserRepository> =
        Arc::new(kalamdb_api::repositories::CoreUsersRepo::new(users_provider));
    
    // SqlExecutor now uses AppContext directly
    let sql_executor = Arc::new(SqlExecutor::new(
        app_context.clone(),
        config.auth.enforce_password_complexity,
    ));

    info!(
        "SqlExecutor initialized with DROP TABLE support, storage registry, job manager, and table registration"
    );

    sql_executor.load_existing_tables().await?;
    info!("Existing tables loaded and registered with DataFusion");

    // JWT authentication
    use jsonwebtoken::Algorithm;
    let jwt_secret = "kalamdb-dev-secret-key-change-in-production".to_string();
    let jwt_auth = Arc::new(JwtAuth::new(jwt_secret, Algorithm::HS256));
    info!("JWT authentication initialized (HS256)");

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

    // Phase 9: All job scheduling now handled by JobsManager
    // Crash recovery handled by JobsManager.recover_incomplete_jobs() in run_loop
    // Flush scheduling via FLUSH TABLE/FLUSH ALL TABLES commands
    // Stream eviction and user cleanup via scheduled job creation (TODO: implement cron scheduler)
    
    info!("Job management delegated to JobsManager (already running in background)");

    // Get users provider for system user initialization
    let users_provider_for_init = app_context.system_tables().users();

    // T125-T127: Create default system user on first startup
    create_default_system_user(users_provider_for_init.clone()).await?;

    // Security warning: Check if remote access is enabled with empty root password
    check_remote_access_security(config, users_provider_for_init).await?;

    let components = ApplicationComponents {
        session_factory,
        sql_executor,
        jwt_auth,
        rate_limiter,
        live_query_manager,
        user_repo,
    };

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

    // Get JobsManager for graceful shutdown
    let job_manager_shutdown = app_context.job_manager();
    let shutdown_timeout_secs = config.shutdown.flush.timeout;

    let session_factory = components.session_factory.clone();
    let sql_executor = components.sql_executor.clone();
    let jwt_auth = components.jwt_auth.clone();
    let rate_limiter = components.rate_limiter.clone();
    let live_query_manager = components.live_query_manager.clone();
    let user_repo = components.user_repo.clone();

    let app_context_for_handler = app_context.clone();
    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::request_logger())
            .wrap(middleware::build_cors())
            .app_data(web::Data::new(app_context_for_handler.clone()))
            .app_data(web::Data::new(session_factory.clone()))
            .app_data(web::Data::new(sql_executor.clone()))
            .app_data(web::Data::new(jwt_auth.clone()))
            .app_data(web::Data::new(rate_limiter.clone()))
            .app_data(web::Data::new(live_query_manager.clone()))
        .app_data(web::Data::new(user_repo.clone()))
            .configure(routes::configure)
    })
    .bind(&bind_addr)?
    .workers(if config.server.workers == 0 {
        num_cpus::get()
    } else {
        config.server.workers
    })
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

            server_handle.stop(true).await;

            info!(
                "Waiting up to {}s for active jobs to complete...",
                shutdown_timeout_secs
            );

            // Signal shutdown to JobsManager
            job_manager_shutdown.shutdown().await;
            
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
/// On first startup, logs the credentials to stdout for the administrator to save.
///
/// # Arguments
/// * `users_provider` - UsersTableProvider for system.users table
///
/// # Returns
/// Result indicating success or failure
async fn create_default_system_user(
    users_provider: Arc<kalamdb_core::tables::system::UsersTableProvider>,
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
            let user_id = UserId::new(AuthConstants::DEFAULT_SYSTEM_USER_ID);
            let username = AuthConstants::DEFAULT_SYSTEM_USERNAME.to_string();
            let email = format!("{}@localhost", AuthConstants::DEFAULT_SYSTEM_USERNAME);
            let role = Role::System; // Highest privilege level
            let created_at = chrono::Utc::now().timestamp_millis();

            // T126: Create with EMPTY password hash for localhost-only access
            // This allows passwordless authentication from localhost (127.0.0.1, ::1)
            // For remote access, set a password using: ALTER USER root SET PASSWORD '...'
            let password_hash = String::new(); // Empty = localhost-only, no password required

            let user = User {
                id: user_id,
                username: username.clone().into(),
                password_hash,
                role,
                email: Some(email),
                auth_type: AuthType::Internal, // System user uses Internal auth
                auth_data: None,               // No allow_remote flag = localhost-only by default
                storage_mode: StorageMode::Table,
                storage_id: Some(StorageId::local()),
                created_at,
                updated_at: created_at,
                last_seen: None,
                deleted_at: None,
            };

            users_provider.create_user(user)?;

            // T127: Log system user information to stdout
            info!(
                "✓ Created system user '{}' (localhost-only access, no password required)",
                username
            );
            info!("  To enable remote access, set a password: ALTER USER root SET PASSWORD '...'",);

            Ok(())
        }
    }
}

/// Check for security issues with remote access configuration
///
/// Informs users about password requirements for remote access
async fn check_remote_access_security(
    config: &ServerConfig,
    users_provider: Arc<kalamdb_core::tables::system::UsersTableProvider>,
) -> Result<()> {
    use kalamdb_commons::constants::AuthConstants;

    // Check if root user exists and has empty password
    // Always show this info if root has no password, regardless of allow_remote_access setting
    if let Ok(Some(user)) = users_provider.get_user_by_username(AuthConstants::DEFAULT_SYSTEM_USERNAME) {
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
