// KalamDB Server
//
// Main server binary for KalamDB - table-centric architecture with DataFusion

mod config;
mod logging;

use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpServer};
use anyhow::Result;
use datafusion::catalog::schema::{MemorySchemaProvider, SchemaProvider};
use kalamdb_api::routes;
use kalamdb_core::services::{
    NamespaceService, SharedTableService, StreamTableService, TableDeletionService,
    UserTableService,
};
use kalamdb_core::sql::datafusion_session::DataFusionSessionFactory;
use kalamdb_core::sql::executor::SqlExecutor;
use kalamdb_core::storage::RocksDbInit;
use kalamdb_core::tables::system::{
    JobsTableProvider, LiveQueriesTableProvider, StorageLocationsTableProvider, UsersTableProvider,
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

    // Initialize RocksDB
    let db_path = std::env::temp_dir().join("kalamdb_data");
    std::fs::create_dir_all(&db_path)?;

    let db_init = RocksDbInit::new(db_path.to_str().unwrap());
    let db = db_init.open()?;
    info!("RocksDB initialized at {}", db_path.display());

    // Initialize KalamSQL for system table operations
    let kalam_sql = Arc::new(kalamdb_sql::KalamSql::new(db.clone())?);
    info!("KalamSQL initialized");

    // Initialize NamespaceService (uses KalamSQL for RocksDB persistence)
    let namespace_service = Arc::new(NamespaceService::new(kalam_sql.clone()));
    info!("NamespaceService initialized");

    // Initialize system table providers (all use KalamSQL for RocksDB operations)
    let users_provider = Arc::new(UsersTableProvider::new(kalam_sql.clone()));
    let storage_locations_provider =
        Arc::new(StorageLocationsTableProvider::new(kalam_sql.clone()));
    let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(kalam_sql.clone()));
    let jobs_provider = Arc::new(JobsTableProvider::new(kalam_sql.clone()));
    info!("System table providers initialized (users, storage_locations, live_queries, jobs)");

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
        .register_table(
            "storage_locations".to_string(),
            storage_locations_provider.clone(),
        )
        .expect("Failed to register system.storage_locations table");
    system_schema
        .register_table("live_queries".to_string(), live_queries_provider.clone())
        .expect("Failed to register system.live_queries table");
    system_schema
        .register_table("jobs".to_string(), jobs_provider.clone())
        .expect("Failed to register system.jobs table");
    info!("System tables registered with DataFusion (system.users, system.storage_locations, system.live_queries, system.jobs)");

    // Initialize table stores
    let user_table_store = Arc::new(UserTableStore::new(db.clone())?);
    let shared_table_store = Arc::new(SharedTableStore::new(db.clone())?);
    let stream_table_store = Arc::new(StreamTableStore::new(db.clone())?);
    info!("Table stores initialized (user, shared, stream)");

    // Initialize table services
    let user_table_service = Arc::new(UserTableService::new(kalam_sql.clone(), user_table_store.clone()));
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
        .with_stores(
            user_table_store.clone(),
            shared_table_store.clone(),
            stream_table_store.clone(),
            kalam_sql.clone(),
        ),
    );
    info!("SqlExecutor initialized with DROP TABLE support and table registration");

    // Load existing tables from system_tables and register with DataFusion
    let default_user_id = kalamdb_core::catalog::UserId::from("system");
    sql_executor
        .load_existing_tables(default_user_id)
        .await
        .expect("Failed to load existing tables");
    info!("Existing tables loaded and registered with DataFusion");

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
