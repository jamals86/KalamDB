// Re-export runtime metrics from kalamdb-observability
pub use kalamdb_observability::{
    collect_runtime_metrics, RuntimeMetrics, BUILD_DATE, GIT_BRANCH, GIT_COMMIT_HASH,
    SERVER_VERSION,
};

/// Compute all server metrics from the application context.
///
/// Returns a vector of (metric_name, metric_value) pairs covering:
/// - Runtime metrics (uptime, memory, CPU, threads)
/// - Entity counts (users, namespaces, tables, jobs, storages, live queries)
/// - Connection metrics (active connections, subscriptions)
/// - Schema cache metrics (size, hit rate)
/// - Manifest cache metrics (memory, RocksDB, breakdown)
/// - Server metadata (version, node ID, cluster info)
pub fn compute_metrics(ctx: &crate::app_context::AppContext) -> Vec<(String, String)> {
    let mut metrics = Vec::new();

    // Runtime metrics from sysinfo (shared with console logging)
    let runtime = collect_runtime_metrics(ctx.server_start_time());
    metrics.extend(runtime.as_pairs());

    // Count entities from system tables
    // Users count
    if let Ok(batch) = ctx.system_tables().users().scan_all_users() {
        metrics.push(("total_users".to_string(), batch.num_rows().to_string()));
    } else {
        metrics.push(("total_users".to_string(), "0".to_string()));
    }

    // Namespaces count
    if let Ok(namespaces) = ctx.system_tables().namespaces().scan_all() {
        metrics.push(("total_namespaces".to_string(), namespaces.len().to_string()));
    } else {
        metrics.push(("total_namespaces".to_string(), "0".to_string()));
    }

    // Tables count
    if let Ok(tables) = ctx.system_tables().tables().scan_all() {
        metrics.push(("total_tables".to_string(), tables.len().to_string()));
    } else {
        metrics.push(("total_tables".to_string(), "0".to_string()));
    }

    // Jobs count
    if let Ok(jobs) = ctx.system_tables().jobs().list_jobs() {
        metrics.push(("total_jobs".to_string(), jobs.len().to_string()));
    } else {
        metrics.push(("total_jobs".to_string(), "0".to_string()));
    }

    // Storages count
    if let Ok(batch) = ctx.system_tables().storages().scan_all_storages() {
        metrics.push(("total_storages".to_string(), batch.num_rows().to_string()));
    } else {
        metrics.push(("total_storages".to_string(), "0".to_string()));
    }

    // Live queries count
    if let Ok(batch) = ctx.system_tables().live_queries().scan_all_live_queries() {
        metrics.push(("total_live_queries".to_string(), batch.num_rows().to_string()));
    } else {
        metrics.push(("total_live_queries".to_string(), "0".to_string()));
    }

    // Active WebSocket connections
    let active_connections = ctx.connection_registry().connection_count();
    metrics.push(("active_connections".to_string(), active_connections.to_string()));

    // Active Subscriptions
    let active_subscriptions = ctx.connection_registry().subscription_count();
    metrics.push(("active_subscriptions".to_string(), active_subscriptions.to_string()));

    // Schema cache size and stats
    let cache_size = ctx.schema_registry().len();
    metrics.push(("schema_cache_size".to_string(), cache_size.to_string()));

    // Schema cache hit rate
    let (_, hits, misses, hit_rate) = ctx.schema_registry().stats();
    metrics.push(("schema_cache_hits".to_string(), hits.to_string()));
    metrics.push(("schema_cache_misses".to_string(), misses.to_string()));
    metrics.push((
        "schema_cache_hit_rate".to_string(),
        format!("{:.2}%", hit_rate * 100.0),
    ));

    // Manifest Cache Metrics
    // Manifests in hot cache (memory)
    // let manifests_in_memory = ctx.manifest_service().hot_cache_len();
    // metrics.push(("manifests_in_memory".to_string(), manifests_in_memory.to_string()));

    // // Manifests in RocksDB (persistent cache)
    // if let Ok(manifests_in_rocksdb) = ctx.manifest_service().count() {
    //     metrics.push(("manifests_in_rocksdb".to_string(), manifests_in_rocksdb.to_string()));
    // } else {
    //     metrics.push(("manifests_in_rocksdb".to_string(), "0".to_string()));
    // }

    // Manifest cache breakdown (shared vs user tables)
    // let (shared_manifests, user_manifests, total_weight) = ctx.manifest_service().cache_stats();
    // metrics.push(("manifests_shared_tables".to_string(), shared_manifests.to_string()));
    // metrics.push(("manifests_user_tables".to_string(), user_manifests.to_string()));
    // metrics.push(("manifests_cache_weight".to_string(), total_weight.to_string()));
    // metrics.push((
    //     "manifests_max_capacity".to_string(),
    //     ctx.manifest_service().max_weighted_capacity().to_string(),
    // ));

    // Node ID
    metrics.push(("node_id".to_string(), ctx.node_id().to_string()));

    // Server build/version
    metrics.push(("server_version".to_string(), SERVER_VERSION.to_string()));
    metrics.push(("server_build_date".to_string(), BUILD_DATE.to_string()));
    metrics.push(("server_git_branch".to_string(), GIT_BRANCH.to_string()));
    metrics.push(("server_git_commit".to_string(), GIT_COMMIT_HASH.to_string()));

    // Cluster info
    let config = ctx.config();
    metrics.push(("cluster_mode".to_string(), config.cluster.is_some().to_string()));
    if let Some(cluster) = &config.cluster {
        metrics.push(("cluster_id".to_string(), cluster.cluster_id.clone()));
        metrics.push(("cluster_rpc_addr".to_string(), cluster.rpc_addr.clone()));
        metrics.push(("cluster_api_addr".to_string(), cluster.api_addr.clone()));
    }

    metrics
}
