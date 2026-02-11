let db = ...; // Assuming db is initialized elsewhere
let backend = Arc::new(RocksDBBackend::new(Arc::new(db))); // Updated
