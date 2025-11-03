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
use datafusion::catalog::memory::MemorySchemaProvider;
use kalamdb_api::auth::jwt::JwtAuth;
use kalamdb_api::rate_limiter::{RateLimitConfig, RateLimiter};
use kalamdb_commons::{AuthType, Role, StorageId, StorageMode, UserId};
use kalamdb_core::jobs::JobManager;
use kalamdb_core::live_query::LiveQueryManager;
use kalamdb_core::services::{
    NamespaceService, SharedTableService, StreamTableService, TableDeletionService,
    UserTableService,
};
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::storage::StorageRegistry;
use kalamdb_core::{
    jobs::{
        JobCleanupTask, JobExecutor, JobResult, StreamEvictionJob, StreamEvictionScheduler,
        TokioJobManager, UserCleanupConfig, UserCleanupJob,
    },
    scheduler::FlushScheduler,
};
use kalamdb_sql::KalamSql;
use kalamdb_sql::RocksDbAdapter;
use kalamdb_store::RocksDBBackend;
use kalamdb_store::RocksDbInit;
use log::debug;
use log::{info, warn};
use std::sync::Arc;
use std::time::Duration;

/// Aggregated application components that need to be shared across the
/// HTTP server and shutdown handling.
pub struct ApplicationComponents {
    pub session_factory: Arc<DataFusionSessionFactory>,
    pub sql_executor: Arc<SqlExecutor>,
    pub jwt_auth: Arc<JwtAuth>,
    pub rate_limiter: Arc<RateLimiter>,
    pub flush_scheduler: Arc<FlushScheduler>,
    pub live_query_manager: Arc<LiveQueryManager>,
    pub stream_eviction_scheduler: Arc<StreamEvictionScheduler>,
    pub rocks_db_adapter: Arc<RocksDbAdapter>,
}

/// Initialize RocksDB, DataFusion, services, rate limiter, and flush scheduler.
pub async fn bootstrap(config: &ServerConfig) -> Result<ApplicationComponents> {
    // Initialize RocksDB
    let db_path = std::path::PathBuf::from(&config.storage.rocksdb_path);
    std::fs::create_dir_all(&db_path)?;

    let db_init = RocksDbInit::new(db_path.to_str().unwrap());
    let db = db_init.open()?;
    info!("RocksDB initialized at {}", db_path.display());

    // Initialize RocksDB backend for all components (single StorageBackend trait)
    let backend = Arc::new(RocksDBBackend::new(db.clone()));

    // Initialize KalamSQL for system table access
    let kalam_sql = Arc::new(KalamSql::new(backend.clone())?);
    info!("KalamSQL initialized");

    // Extract RocksDbAdapter for API key authentication
    let rocks_db_adapter = Arc::new(kalam_sql.adapter().clone());

    // Seed default storage if necessary
    let storages = kalam_sql.scan_all_storages()?;
    if storages.is_empty() {
        info!("No storages found, creating default 'local' storage");
        let now = chrono::Utc::now().timestamp_millis();
        let default_storage = kalamdb_sql::Storage {
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
        kalam_sql.insert_storage(&default_storage)?;
        info!("Default 'local' storage created successfully");
    } else {
        info!("Found {} existing storage(s)", storages.len());
    }

    // Initialize core stores from generic backend (uses kalamdb_store::StorageBackend)
    let core = kalamdb_core::kalam_core::KalamCore::new(backend.clone())?;
    let user_table_store = core.user_table_store.clone();
    let shared_table_store = core.shared_table_store.clone();
    let stream_table_store = core.stream_table_store.clone();

    let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
    let user_table_service = Arc::new(UserTableService::new(
        kalam_sql.clone(),
        user_table_store.clone(),
    ));
    let shared_table_service = Arc::new(SharedTableService::new(
        shared_table_store.clone(),
        kalam_sql.clone(),
        config.storage.default_storage_path.clone(),
    ));
    let stream_table_service = Arc::new(StreamTableService::new(
        stream_table_store.clone(),
        kalam_sql.clone(),
    ));
    let table_deletion_service = Arc::new(TableDeletionService::new(
        user_table_store.clone(),
        shared_table_store.clone(),
        stream_table_store.clone(),
        kalam_sql.clone(),
    ));

    // DataFusion session factory and base context
    let session_factory = Arc::new(DataFusionSessionFactory::new()?);
    let session_context = Arc::new(session_factory.create_session());

    // Register system tables with DataFusion
    let system_schema = Arc::new(MemorySchemaProvider::new());
    let catalog_name = session_context
        .catalog_names()
        .first()
        .expect("No catalogs available")
        .clone();

    session_context
        .catalog(&catalog_name)
        .expect("Failed to get catalog")
        .register_schema("system", system_schema.clone())
        .expect("Failed to register system schema");

    // Register all system tables using centralized function (EntityStore-based v2 providers)
    let (jobs_provider, schema_store) =
        kalamdb_core::system_table_registration::register_system_tables(
            &system_schema,
            backend.clone(),
        )
        .expect("Failed to register system tables");

    info!(
        "System tables registered with DataFusion (catalog: {})",
        catalog_name
    );
    info!(
        "TableSchemaStore initialized with {} system table schemas",
        7
    );

    // Storage registry and SQL executor
    let storage_registry = Arc::new(StorageRegistry::new(
        kalam_sql.clone(),
        config.storage.default_storage_path.clone(),
    ));

    // Create job manager first
    let job_manager = Arc::new(TokioJobManager::new());

    // Live query manager (per-node) - use configured node_id
    let node_id = kalamdb_commons::NodeId::new(config.server.node_id.clone());
    let live_query_manager = Arc::new(LiveQueryManager::new(
        kalam_sql.clone(),
        node_id.clone(),
        Some(user_table_store.clone()),
        Some(shared_table_store.clone()),
        Some(stream_table_store.clone()),
    ));
    info!("LiveQueryManager initialized with node_id: {}", node_id);

    let sql_executor = Arc::new(
        SqlExecutor::new(
            namespace_service.clone(),
            session_context.clone(),
            user_table_service.clone(),
            shared_table_service.clone(),
            stream_table_service.clone(),
        )
        .with_table_deletion_service(table_deletion_service)
        .with_storage_registry(storage_registry)
        .with_job_manager(job_manager.clone())
        .with_live_query_manager(live_query_manager.clone())
        .with_stores(
            user_table_store.clone(),
            shared_table_store.clone(),
            stream_table_store.clone(),
            kalam_sql.clone(),
        )
        .with_password_complexity(config.auth.enforce_password_complexity)
        .with_storage_backend(backend.clone())
        .with_schema_store(schema_store), // Phase 10: schema_cache is part of unified_cache now
    );

    info!(
        "SqlExecutor initialized with DROP TABLE support, storage registry, job manager, and table registration"
    );

    let default_user_id = UserId::from("system");
    sql_executor.load_existing_tables(default_user_id).await?;
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

    // Flush scheduler
    let flush_scheduler = Arc::new(
        FlushScheduler::new(job_manager.clone(), std::time::Duration::from_secs(5))
            .with_jobs_provider(jobs_provider.clone()),
    );

    // Job executor for stream eviction
    let job_executor = Arc::new(JobExecutor::new(
        jobs_provider.clone(),
           node_id.clone(),
    ));

    // Stream eviction job and scheduler
    let stream_eviction_job = Arc::new(StreamEvictionJob::with_defaults(
        stream_table_store.clone(),
        kalam_sql.clone(),
        job_executor.clone(),
    ));

    let eviction_interval = Duration::from_secs(config.stream.eviction_interval_seconds);
    let stream_eviction_scheduler = Arc::new(StreamEvictionScheduler::new(
        stream_eviction_job,
        eviction_interval,
    ));
    info!(
        "Stream eviction scheduler initialized (interval: {} seconds)",
        config.stream.eviction_interval_seconds
    );

    // User cleanup job (scheduled background task)
    let user_cleanup_job = Arc::new(UserCleanupJob::new(
        kalam_sql.clone(),
        user_table_store.clone(),
        UserCleanupConfig {
            grace_period_days: config.user_management.deletion_grace_period_days,
        },
    ));

    let cleanup_interval =
        JobCleanupTask::parse_cron_schedule(&config.user_management.cleanup_job_schedule);
    let cleanup_job_manager = job_manager.clone();
    let cleanup_job_executor = job_executor.clone();
    let scheduled_cleanup_job = Arc::clone(&user_cleanup_job);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(cleanup_interval);

        loop {
            ticker.tick().await;

            let job_id = format!("user-cleanup-{}", chrono::Utc::now().timestamp_millis());
            let job_id_clone = job_id.clone();
            let job_executor = Arc::clone(&cleanup_job_executor);
            let job_instance = Arc::clone(&scheduled_cleanup_job);
            let grace_period = job_instance.config().grace_period_days;

            let job_future = Box::pin(async move {
                let executor_job_id = job_id;
                let cleanup_job = Arc::clone(&job_instance);
                let result = job_executor.execute_job(
                    executor_job_id,
                    "user_cleanup".to_string(),
                    None,
                    vec![format!("grace_period_days={}", grace_period)],
                    move || {
                        cleanup_job
                            .enforce()
                            .map(|count| format!("Deleted {} expired users", count))
                            .map_err(|err| err.to_string())
                    },
                );

                match result {
                    Ok(JobResult::Success(msg)) => Ok(msg),
                    Ok(JobResult::Failure(msg)) => Err(msg),
                    Err(err) => Err(err.to_string()),
                }
            });

            if let Err(e) = cleanup_job_manager
                .start_job(job_id_clone, "user_cleanup".to_string(), job_future)
                .await
            {
                log::error!("Failed to start user cleanup job: {}", e);
            }
        }
    });
    info!(
        "User cleanup job scheduled (grace period: {} days, schedule: {})",
        config.user_management.deletion_grace_period_days,
        config.user_management.cleanup_job_schedule
    );

    // Resume crash recovery jobs
    match flush_scheduler.resume_incomplete_jobs().await {
        Ok(count) if count > 0 => {
            info!(
                "Resumed {} incomplete flush jobs from previous session",
                count
            );
        }
        Ok(_) => {}
        Err(e) => {
            log::warn!("Failed to resume incomplete jobs: {}", e);
        }
    }

    flush_scheduler.start().await?;
    info!("FlushScheduler started (checking triggers every 5 seconds)");

    // Start stream eviction scheduler
    stream_eviction_scheduler.start().await?;
    info!(
        "Stream eviction scheduler started (running every {} seconds)",
        config.stream.eviction_interval_seconds
    );

    // T125-T127: Create default system user on first startup
    create_default_system_user(kalam_sql.clone()).await?;

    // Security warning: Check if remote access is enabled with empty root password
    check_remote_access_security(config, kalam_sql.clone()).await?;

    Ok(ApplicationComponents {
        session_factory,
        sql_executor,
        jwt_auth,
        rate_limiter,
        flush_scheduler,
        live_query_manager,
        stream_eviction_scheduler,
        rocks_db_adapter,
    })
}

/// Start the HTTP server and manage graceful shutdown.
pub async fn run(config: &ServerConfig, components: ApplicationComponents) -> Result<()> {
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Starting HTTP server on {}", bind_addr);
    info!("Endpoints: POST /v1/api/sql, GET /v1/ws");

    let flush_scheduler_shutdown = components.flush_scheduler.clone();
    let stream_eviction_scheduler_shutdown = components.stream_eviction_scheduler.clone();
    let shutdown_timeout_secs = config.shutdown.flush.timeout;

    let session_factory = components.session_factory.clone();
    let sql_executor = components.sql_executor.clone();
    let jwt_auth = components.jwt_auth.clone();
    let rate_limiter = components.rate_limiter.clone();
    let live_query_manager = components.live_query_manager.clone();
    let rocks_db_adapter = components.rocks_db_adapter.clone();

    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::request_logger())
            .wrap(middleware::build_cors())
            .app_data(web::Data::new(session_factory.clone()))
            .app_data(web::Data::new(sql_executor.clone()))
            .app_data(web::Data::new(jwt_auth.clone()))
            .app_data(web::Data::new(rate_limiter.clone()))
            .app_data(web::Data::new(live_query_manager.clone()))
            .app_data(web::Data::new(rocks_db_adapter.clone()))
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
                "Waiting up to {}s for active flush jobs to complete...",
                shutdown_timeout_secs
            );
            let timeout = std::time::Duration::from_secs(shutdown_timeout_secs as u64);

            match flush_scheduler_shutdown.wait_for_active_jobs(timeout).await {
                Ok(_) => info!("All flush jobs completed successfully"),
                Err(e) => log::warn!("Flush jobs did not complete within timeout: {}", e),
            }

            if let Err(e) = flush_scheduler_shutdown.stop().await {
                log::error!("Error stopping flush scheduler: {}", e);
            }

            if let Err(e) = stream_eviction_scheduler_shutdown.stop().await {
                log::error!("Error stopping stream eviction scheduler: {}", e);
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
/// * `kalam_sql` - KalamSQL adapter for system tables
///
/// # Returns
/// Result indicating success or failure
async fn create_default_system_user(kalam_sql: Arc<KalamSql>) -> Result<()> {
    use kalamdb_commons::constants::AuthConstants;
    use kalamdb_commons::system::User;

    // Check if root user already exists
    let existing_user = kalam_sql.get_user(AuthConstants::DEFAULT_SYSTEM_USERNAME);

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

            kalam_sql.insert_user(&user)?;

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
    kalam_sql: Arc<KalamSql>,
) -> Result<()> {
    use kalamdb_commons::constants::AuthConstants;

    // Check if root user exists and has empty password
    // Always show this info if root has no password, regardless of allow_remote_access setting
    if let Ok(Some(user)) = kalam_sql.get_user(AuthConstants::DEFAULT_SYSTEM_USERNAME) {
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
