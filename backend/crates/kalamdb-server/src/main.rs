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
    let catalog_store = Arc::new(CatalogStore::new(db));
    info!("CatalogStore initialized");

    // Initialize NamespaceService
    let config_path = db_path.join("conf");
    std::fs::create_dir_all(&config_path)?;
    let namespaces_json = config_path.join("namespaces.json").to_str().unwrap().to_string();
    let base_config = config_path.to_str().unwrap().to_string();
    
    let namespace_service = Arc::new(NamespaceService::new(
        namespaces_json,
        base_config,
    ));
    info!("NamespaceService initialized");

    // Initialize DataFusion session factory
    let session_factory = Arc::new(
        DataFusionSessionFactory::new()
            .expect("Failed to create DataFusion session factory")
    );
    info!("DataFusion session factory initialized");

    // Initialize SqlExecutor
    let session_context = Arc::new(session_factory.create_session());
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
