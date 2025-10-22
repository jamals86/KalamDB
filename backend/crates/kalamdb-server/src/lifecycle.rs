//! Server lifecycle management helpers.
//!
//! This module encapsulates the heavy lifting previously handled directly
//! in `main.rs`: bootstrapping databases and services, wiring the HTTP
//! server, and coordinating graceful shutdown.

use crate::config::ServerConfig;
use crate::middleware;
use crate::routes;
use actix_web::{web, App, HttpServer};
use anyhow::Result;
use datafusion::catalog::schema::{MemorySchemaProvider, SchemaProvider};
use kalamdb_api::auth::jwt::JwtAuth;
use kalamdb_api::rate_limiter::{RateLimitConfig, RateLimiter};
use kalamdb_core::services::{
    NamespaceService, SharedTableService, StreamTableService, TableDeletionService,
    UserTableService,
};
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::storage::{RocksDbInit, StorageRegistry};
use kalamdb_core::tables::system::{
    JobsTableProvider, LiveQueriesTableProvider, NamespacesTableProvider,
    StorageLocationsTableProvider, SystemStoragesProvider, SystemTablesTableProvider,
    UsersTableProvider,
};
use kalamdb_core::{jobs::TokioJobManager, scheduler::FlushScheduler};
use kalamdb_sql::KalamSql;
use kalamdb_store::{SharedTableStore, StreamTableStore, UserTableStore};
use log::info;
use std::sync::Arc;

/// Aggregated application components that need to be shared across the
/// HTTP server and shutdown handling.
pub struct ApplicationComponents {
    pub session_factory: Arc<DataFusionSessionFactory>,
    pub sql_executor: Arc<SqlExecutor>,
    pub jwt_auth: Arc<JwtAuth>,
    pub rate_limiter: Arc<RateLimiter>,
    pub flush_scheduler: Arc<FlushScheduler>,
}

/// Initialize RocksDB, DataFusion, services, rate limiter, and flush scheduler.
pub async fn bootstrap(config: &ServerConfig) -> Result<ApplicationComponents> {
    // Initialize RocksDB
    let db_path = std::path::PathBuf::from(&config.storage.rocksdb_path);
    std::fs::create_dir_all(&db_path)?;

    let db_init = RocksDbInit::new(db_path.to_str().unwrap());
    let db = db_init.open()?;
    info!("RocksDB initialized at {}", db_path.display());

    // Initialize KalamSQL for system table access
    let kalam_sql = Arc::new(KalamSql::new(db.clone())?);
    info!("KalamSQL initialized");

    // Seed default storage if necessary
    let storages = kalam_sql.scan_all_storages()?;
    if storages.is_empty() {
        info!("No storages found, creating default 'local' storage");
        let now = chrono::Utc::now().timestamp_millis();
        let default_storage = kalamdb_sql::Storage {
            storage_id: "local".to_string(),
            storage_name: "Local Filesystem".to_string(),
            description: Some("Default local filesystem storage".to_string()),
            storage_type: "filesystem".to_string(),
            base_directory: "".to_string(),
            credentials: None,
            shared_tables_template: "{namespace}/{tableName}".to_string(),
            user_tables_template: "{namespace}/{tableName}/{userId}".to_string(),
            created_at: now,
            updated_at: now,
        };
        kalam_sql.insert_storage(&default_storage)?;
        info!("Default 'local' storage created successfully");
    } else {
        info!("Found {} existing storage(s)", storages.len());
    }

    // Initialize stores and services
    let user_table_store = Arc::new(UserTableStore::new(db.clone())?);
    let shared_table_store = Arc::new(SharedTableStore::new(db.clone())?);
    let stream_table_store = Arc::new(StreamTableStore::new(db.clone())?);

    let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
    let user_table_service = Arc::new(UserTableService::new(
        kalam_sql.clone(),
        user_table_store.clone(),
    ));
    let shared_table_service = Arc::new(SharedTableService::new(
        shared_table_store.clone(),
        kalam_sql.clone(),
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

    let users_provider = Arc::new(UsersTableProvider::new(kalam_sql.clone()));
    let namespaces_provider = Arc::new(NamespacesTableProvider::new(kalam_sql.clone()));
    let tables_provider = Arc::new(SystemTablesTableProvider::new(kalam_sql.clone()));
    let storage_locations_provider =
        Arc::new(StorageLocationsTableProvider::new(kalam_sql.clone()));
    let storages_provider = Arc::new(SystemStoragesProvider::new(kalam_sql.clone()));
    let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(kalam_sql.clone()));
    let jobs_provider = Arc::new(JobsTableProvider::new(kalam_sql.clone()));

    system_schema
        .register_table("users".to_string(), users_provider)
        .expect("Failed to register system.users table");
    system_schema
        .register_table("namespaces".to_string(), namespaces_provider)
        .expect("Failed to register system.namespaces table");
    system_schema
        .register_table("tables".to_string(), tables_provider)
        .expect("Failed to register system.tables table");
    system_schema
        .register_table("storage_locations".to_string(), storage_locations_provider)
        .expect("Failed to register system.storage_locations table");
    system_schema
        .register_table("storages".to_string(), storages_provider)
        .expect("Failed to register system.storages table");
    system_schema
        .register_table("live_queries".to_string(), live_queries_provider)
        .expect("Failed to register system.live_queries table");
    system_schema
        .register_table("jobs".to_string(), jobs_provider.clone())
        .expect("Failed to register system.jobs table");

    info!(
        "System tables registered with DataFusion (catalog: {})",
        catalog_name
    );

    // Storage registry and SQL executor
    let storage_registry = Arc::new(StorageRegistry::new(kalam_sql.clone()));
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
        .with_stores(
            user_table_store,
            shared_table_store,
            stream_table_store,
            kalam_sql.clone(),
        ),
    );

    info!(
        "SqlExecutor initialized with DROP TABLE support, storage registry, and table registration"
    );

    let default_user_id = kalamdb_core::catalog::UserId::from("system");
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
    let job_manager = Arc::new(TokioJobManager::new());
    let flush_scheduler = Arc::new(
        FlushScheduler::new(job_manager, std::time::Duration::from_secs(5))
            .with_jobs_provider(jobs_provider.clone()),
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

    Ok(ApplicationComponents {
        session_factory,
        sql_executor,
        jwt_auth,
        rate_limiter,
        flush_scheduler,
    })
}

/// Start the HTTP server and manage graceful shutdown.
pub async fn run(config: &ServerConfig, components: ApplicationComponents) -> Result<()> {
    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Starting HTTP server on {}", bind_addr);
    info!("Endpoints: POST /v1/api/sql, GET /v1/ws");

    let flush_scheduler_shutdown = components.flush_scheduler.clone();
    let shutdown_timeout_secs = config.server.flush_job_shutdown_timeout_seconds;

    let session_factory = components.session_factory.clone();
    let sql_executor = components.sql_executor.clone();
    let jwt_auth = components.jwt_auth.clone();
    let rate_limiter = components.rate_limiter.clone();

    let server = HttpServer::new(move || {
        App::new()
            .wrap(middleware::request_logger())
            .wrap(middleware::build_cors())
            .app_data(web::Data::new(session_factory.clone()))
            .app_data(web::Data::new(sql_executor.clone()))
            .app_data(web::Data::new(jwt_auth.clone()))
            .app_data(web::Data::new(rate_limiter.clone()))
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
        }
    }

    info!("Server shutdown complete");
    Ok(())
}
