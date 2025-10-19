// KalamDB Server
//
// Main server binary for KalamDB - table-centric architecture with DataFusion

mod config;
mod logging;

use actix_web::{middleware, web, App, HttpServer};
use actix_cors::Cors;
use anyhow::Result;
use kalamdb_api::routes;
use kalamdb_core::catalog::CatalogStore;
use kalamdb_core::services::NamespaceService;
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::storage::RocksDbInit;
use kalamdb_core::tables::system::{
    UsersTableProvider, StorageLocationsTableProvider,
    LiveQueriesTableProvider, JobsTableProvider,
};
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

    info!("Starting KalamDB Server v{} (Table-centric architecture)", env!("CARGO_PKG_VERSION"));
    info!("Configuration loaded: host={}, port={}", config.server.host, config.server.port);

    // Initialize RocksDB
    let db_path = std::env::temp_dir().join("kalamdb_data");
    std::fs::create_dir_all(&db_path)?;
    
    let db_init = RocksDbInit::new(db_path.to_str().unwrap());
    let db = db_init.open()?;
    info!("RocksDB initialized at {}", db_path.display());

    // Initialize CatalogStore
    let catalog_store = Arc::new(CatalogStore::new(db.clone()));
    info!("CatalogStore initialized");

    // Initialize KalamSQL for system table operations
    let kalam_sql = Arc::new(kalamdb_sql::KalamSql::new(db)?);
    info!("KalamSQL initialized");

    // Initialize NamespaceService (uses KalamSQL for RocksDB persistence)
    let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
    info!("NamespaceService initialized");

    // Initialize system table providers (all use KalamSQL for RocksDB operations)
    let users_provider = Arc::new(UsersTableProvider::new(kalam_sql.clone()));
    let storage_locations_provider = Arc::new(StorageLocationsTableProvider::new(kalam_sql.clone()));
    let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(kalam_sql.clone()));
    let jobs_provider = Arc::new(JobsTableProvider::new(kalam_sql.clone()));
    info!("System table providers initialized (users, storage_locations, live_queries, jobs)");

    // Initialize DataFusion session factory
    let session_factory = Arc::new(
        DataFusionSessionFactory::new()
            .expect("Failed to create DataFusion session factory")
    );
    info!("DataFusion session factory initialized");

    // Initialize DataFusion session and register system tables
    let session_context = Arc::new(session_factory.create_session());
    
    // Register system table providers with DataFusion
    session_context
        .register_table("system.users", users_provider.clone())
        .expect("Failed to register system.users table");
    session_context
        .register_table("system.storage_locations", storage_locations_provider.clone())
        .expect("Failed to register system.storage_locations table");
    session_context
        .register_table("system.live_queries", live_queries_provider.clone())
        .expect("Failed to register system.live_queries table");
    session_context
        .register_table("system.jobs", jobs_provider.clone())
        .expect("Failed to register system.jobs table");
    info!("System tables registered with DataFusion (system.users, system.storage_locations, system.live_queries, system.jobs)");

    // Initialize SqlExecutor
    let sql_executor = Arc::new(SqlExecutor::new(
        namespace_service.clone(),
        session_context.clone(),
    ));
    info!("SqlExecutor initialized");

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Starting HTTP server on {}", bind_addr);
    info!("Endpoints: POST /api/sql, GET /ws");

    // Start HTTP server
    HttpServer::new(move || {
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
            .configure(routes::configure_routes)
    })
    .bind(&bind_addr)?
    .workers(if config.server.workers == 0 {
        num_cpus::get()
    } else {
        config.server.workers
    })
    .run()
    .await?;

    info!("Server shutdown complete");
    Ok(())
}
