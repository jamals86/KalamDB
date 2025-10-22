// KalamDB Server
//
// Main server binary for KalamDB - table-centric architecture with DataFusion

mod config;
mod logging;

use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpServer};
use anyhow::Result;
use datafusion::catalog::schema::{MemorySchemaProvider, SchemaProvider};
use kalamdb_api::auth::jwt::JwtAuth;
use kalamdb_api::rate_limiter::RateLimiter;
use kalamdb_api::routes;
use kalamdb_core::services::{
    NamespaceService, SharedTableService, StreamTableService, TableDeletionService,
    UserTableService,
};
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::storage::RocksDbInit;
use kalamdb_core::tables::system::{
    JobsTableProvider, LiveQueriesTableProvider, NamespacesTableProvider,
    StorageLocationsTableProvider, SystemStoragesProvider, SystemTablesTableProvider,
    UsersTableProvider,
};
use kalamdb_store::{SharedTableStore, StreamTableStore, UserTableStore};
use log::info;
use std::sync::Arc;

#[actix_web::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = match config::ServerConfig::from_file("config.toml") {
        Ok(cfg) => cfg,
        Err(_) => {
            eprintln!("Warning: config.toml not found, using defaults");
            config::ServerConfig::default()
        }
    };

    // Initialize logging
    logging::init_logging(
        &config.logging.level,
        &config.logging.file_path,
        config.logging.log_to_console,
    )?;

    info!(
        "Starting KalamDB Server v{} (Table-centric architecture)",
        env!("CARGO_PKG_VERSION")
    );
    info!(
        "Configuration loaded: host={}, port={}",
        config.server.host, config.server.port
    );

    // Initialize RocksDB using path from config
    let db_path = std::path::PathBuf::from(&config.storage.rocksdb_path);
    std::fs::create_dir_all(&db_path)?;

    let db_init = RocksDbInit::new(db_path.to_str().unwrap());
    let db = db_init.open()?;
    info!("RocksDB initialized at {}", db_path.display());

    // Initialize KalamSQL for system table operations
    let kalam_sql = Arc::new(kalamdb_sql::KalamSql::new(db.clone())?);
    info!("KalamSQL initialized");

    // T164: Create default 'local' storage if system.storages is empty
    let storages = kalam_sql.scan_all_storages()?;
    if storages.is_empty() {
        info!("No storages found, creating default 'local' storage");
        let now = chrono::Utc::now().timestamp_millis();
        let default_storage = kalamdb_sql::Storage {
            storage_id: "local".to_string(),
            storage_name: "Local Filesystem".to_string(),
            description: Some("Default local filesystem storage".to_string()),
            storage_type: "filesystem".to_string(),
            base_directory: "".to_string(), // Empty means use default_storage_path from config
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

    // Initialize NamespaceService (uses KalamSQL for RocksDB persistence)
    let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
    info!("NamespaceService initialized");

    // Initialize system table providers (all use KalamSQL for RocksDB operations)
    let users_provider = Arc::new(UsersTableProvider::new(kalam_sql.clone()));
    let namespaces_provider = Arc::new(NamespacesTableProvider::new(kalam_sql.clone()));
    let tables_provider = Arc::new(SystemTablesTableProvider::new(kalam_sql.clone()));
    let storage_locations_provider =
        Arc::new(StorageLocationsTableProvider::new(kalam_sql.clone()));
    let storages_provider = Arc::new(SystemStoragesProvider::new(kalam_sql.clone()));
    let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(kalam_sql.clone()));
    let jobs_provider = Arc::new(JobsTableProvider::new(kalam_sql.clone()));
    info!("System table providers initialized (users, namespaces, tables, storage_locations, storages, live_queries, jobs)");

    // Initialize DataFusion session factory
    let session_factory = Arc::new(
        DataFusionSessionFactory::new().expect("Failed to create DataFusion session factory"),
    );
    info!("DataFusion session factory initialized");

    // Initialize DataFusion session
    let session_context = Arc::new(session_factory.create_session());

    // Create "system" schema in DataFusion
    // DataFusion's default catalog is named "datafusion", but we need to handle potential errors
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
    info!(
        "System schema created in DataFusion (catalog: {})",
        catalog_name
    );

    // Register system table providers with DataFusion (in the system schema)
    system_schema
        .register_table("users".to_string(), users_provider.clone())
        .expect("Failed to register system.users table");
    system_schema
        .register_table("namespaces".to_string(), namespaces_provider.clone())
        .expect("Failed to register system.namespaces table");
    system_schema
        .register_table("tables".to_string(), tables_provider.clone())
        .expect("Failed to register system.tables table");
    system_schema
        .register_table(
            "storage_locations".to_string(),
            storage_locations_provider.clone(),
        )
        .expect("Failed to register system.storage_locations table");
    system_schema
        .register_table("storages".to_string(), storages_provider.clone())
        .expect("Failed to register system.storages table");
    system_schema
        .register_table("live_queries".to_string(), live_queries_provider.clone())
        .expect("Failed to register system.live_queries table");
    system_schema
        .register_table("jobs".to_string(), jobs_provider.clone())
        .expect("Failed to register system.jobs table");
    info!("System tables registered with DataFusion (system.users, system.namespaces, system.tables, system.storage_locations, system.storages, system.live_queries, system.jobs)");

    // Initialize table stores
    let user_table_store = Arc::new(UserTableStore::new(db.clone())?);
    let shared_table_store = Arc::new(SharedTableStore::new(db.clone())?);
    let stream_table_store = Arc::new(StreamTableStore::new(db.clone())?);
    info!("Table stores initialized (user, shared, stream)");

    // Initialize table services
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
    info!("Table services initialized (user, shared, stream)");

    // Initialize TableDeletionService for DROP TABLE support
    let table_deletion_service = Arc::new(TableDeletionService::new(
        user_table_store.clone(),
        shared_table_store.clone(),
        stream_table_store.clone(),
        kalam_sql.clone(),
    ));
    info!("TableDeletionService initialized");

    // Initialize StorageRegistry for template validation
    let storage_registry = Arc::new(kalamdb_core::storage::StorageRegistry::new(kalam_sql.clone()));
    info!("StorageRegistry initialized");

    // Initialize SqlExecutor with builder pattern
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
            user_table_store.clone(),
            shared_table_store.clone(),
            stream_table_store.clone(),
            kalam_sql.clone(),
        ),
    );
    info!("SqlExecutor initialized with DROP TABLE support, storage registry, and table registration");

    // Load existing tables from system_tables and register with DataFusion
    let default_user_id = kalamdb_core::catalog::UserId::from("system");
    sql_executor
        .load_existing_tables(default_user_id)
        .await
        .expect("Failed to load existing tables");
    info!("Existing tables loaded and registered with DataFusion");

    // Initialize JWT authentication
    // Note: For production, use RS256 with proper key management
    // For development/testing, we use HS256 with a symmetric key
    let jwt_secret = "kalamdb-dev-secret-key-change-in-production".to_string();
    // Use HS256 (HMAC with SHA-256) - requires jsonwebtoken crate
    use jsonwebtoken::Algorithm;
    let jwt_auth = Arc::new(JwtAuth::new(jwt_secret, Algorithm::HS256));
    info!("JWT authentication initialized (HS256)");

    // Initialize rate limiter with config values
    let rate_limit_config = kalamdb_api::rate_limiter::RateLimitConfig {
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

    // T156: Initialize FlushScheduler for automatic table flushing
    use kalamdb_core::jobs::TokioJobManager;
    use kalamdb_core::scheduler::FlushScheduler;
    
    let job_manager = Arc::new(TokioJobManager::new());
    let flush_scheduler = Arc::new(
        FlushScheduler::new(
            job_manager,
            std::time::Duration::from_secs(5), // Check triggers every 5 seconds
        )
        .with_jobs_provider(jobs_provider.clone())
    );
    
    // Resume incomplete jobs from previous session (crash recovery)
    match flush_scheduler.resume_incomplete_jobs().await {
        Ok(count) => {
            if count > 0 {
                info!("Resumed {} incomplete flush jobs from previous session", count);
            }
        }
        Err(e) => {
            log::warn!("Failed to resume incomplete jobs: {}", e);
        }
    }
    
    // Start the flush scheduler
    flush_scheduler.start().await
        .expect("Failed to start flush scheduler");
    info!("FlushScheduler started (checking triggers every 5 seconds)");

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Starting HTTP server on {}", bind_addr);
    info!("Endpoints: POST /api/sql, GET /ws");

    // Clone flush_scheduler for shutdown handler
    let flush_scheduler_shutdown = flush_scheduler.clone();
    let shutdown_timeout_secs = config.server.flush_job_shutdown_timeout_seconds;

    // Start HTTP server
    let server = HttpServer::new(move || {
        // Configure CORS for web browser clients
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .supports_credentials()
            .max_age(3600);

        App::new()
            .wrap(middleware::Logger::default())
            .wrap(cors)
            .app_data(web::Data::new(session_factory.clone()))
            .app_data(web::Data::new(sql_executor.clone()))
            .app_data(web::Data::new(jwt_auth.clone()))
            .app_data(web::Data::new(rate_limiter.clone()))
            .configure(routes::configure_routes)
    })
    .bind(&bind_addr)?
    .workers(if config.server.workers == 0 {
        num_cpus::get()
    } else {
        config.server.workers
    })
    .run();

    // Get server handle for graceful shutdown
    let server_handle = server.handle();
    
    // Spawn server task
    let server_task = tokio::spawn(server);
    
    // Wait for Ctrl+C or SIGTERM
    tokio::select! {
        result = server_task => {
            if let Err(e) = result {
                log::error!("Server task failed: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received Ctrl+C, initiating graceful shutdown...");
            
            // Stop accepting new connections
            server_handle.stop(true).await;
            
            // Wait for active flush jobs to complete (T158h, T158i)
            info!("Waiting up to {}s for active flush jobs to complete...", shutdown_timeout_secs);
            let timeout = std::time::Duration::from_secs(shutdown_timeout_secs as u64);
            
            match flush_scheduler_shutdown.wait_for_active_jobs(timeout).await {
                Ok(_) => {
                    info!("All flush jobs completed successfully");
                }
                Err(e) => {
                    log::warn!("Flush jobs did not complete within timeout: {}", e);
                }
            }
            
            // Stop the scheduler
            if let Err(e) = flush_scheduler_shutdown.stop().await {
                log::error!("Error stopping flush scheduler: {}", e);
            }
        }
    }

    info!("Server shutdown complete");
    Ok(())
}
