use std::time::Instant;

use sysinfo::System;

/// Snapshot of runtime/system metrics gathered from sysinfo.
pub struct RuntimeMetrics {
    pub uptime_seconds: u64,
    pub uptime_human: String,
    pub memory_bytes: Option<u64>,
    pub memory_mb: Option<u64>,
    pub cpu_usage_percent: Option<f32>,
    pub system_total_memory_mb: u64,
    pub system_used_memory_mb: u64,
    pub thread_count: Option<usize>,
    pub pid: Option<u32>,
}

impl RuntimeMetrics {
    /// Render as key/value pairs for system.stats.
    pub fn as_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();

        pairs.push(("server_uptime_seconds".to_string(), self.uptime_seconds.to_string()));
        pairs.push(("server_uptime_human".to_string(), self.uptime_human.clone()));

        if let Some(bytes) = self.memory_bytes {
            pairs.push(("memory_usage_bytes".to_string(), bytes.to_string()));
        }
        if let Some(mb) = self.memory_mb {
            pairs.push(("memory_usage_mb".to_string(), mb.to_string()));
        }
        if let Some(cpu) = self.cpu_usage_percent {
            pairs.push(("cpu_usage_percent".to_string(), format!("{:.2}", cpu)));
        }

        pairs.push((
            "system_total_memory_mb".to_string(),
            self.system_total_memory_mb.to_string(),
        ));
        pairs.push((
            "system_used_memory_mb".to_string(),
            self.system_used_memory_mb.to_string(),
        ));

        if let Some(t) = self.thread_count {
            pairs.push(("thread_count".to_string(), t.to_string()));
        }

        if let Some(pid) = self.pid {
            pairs.push(("pid".to_string(), pid.to_string()));
        }

        pairs
    }

    /// Render a concise log line for the console.
    pub fn to_log_string(&self) -> String {
        format!(
            "uptime={} mem={}MB used={}MB cpu={} pid={} threads={} sys_mem={}MB/{}MB",
            self.uptime_human,
            self.memory_mb.unwrap_or(0),
            self.system_used_memory_mb,
            self.cpu_usage_percent
                .map(|v| format!("{:.2}%", v))
                .unwrap_or_else(|| "N/A".to_string()),
            self.pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            self.thread_count
                .map(|t| t.to_string())
                .unwrap_or_else(|| "N/A".to_string()),
            self.system_used_memory_mb,
            self.system_total_memory_mb,
        )
    }
}

/// Collect runtime metrics from sysinfo using the server start time for uptime.
pub fn collect_runtime_metrics(start_time: Instant) -> RuntimeMetrics {
    let uptime_seconds = start_time.elapsed().as_secs();
    let days = uptime_seconds / 86400;
    let hours = (uptime_seconds % 86400) / 3600;
    let minutes = (uptime_seconds % 3600) / 60;
    let uptime_human = if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    };

    let mut sys = System::new_all();
    sys.refresh_all();

    let mut memory_bytes = None;
    let mut memory_mb = None;
    let mut cpu_usage_percent = None;
    #[allow(unused_mut)]
    let mut thread_count = None;
    let mut pid_num = None;

    if let Ok(pid) = sysinfo::get_current_pid() {
        if let Some(proc) = sys.process(pid) {
            pid_num = Some(proc.pid().as_u32());
            let mem_bytes = proc.memory();
            memory_bytes = Some(mem_bytes);
            memory_mb = Some(mem_bytes / 1024 / 1024);
            cpu_usage_percent = Some(proc.cpu_usage());
            #[cfg(unix)]
            {
                if let Ok(entries) = std::fs::read_dir("/proc/self/task") {
                    thread_count = Some(entries.count());
                }
            }
        }
    }

    let system_total_memory_mb = sys.total_memory() / 1024 / 1024;
    let system_used_memory_mb = sys.used_memory() / 1024 / 1024;

    RuntimeMetrics {
        uptime_seconds,
        uptime_human,
        memory_bytes,
        memory_mb,
        cpu_usage_percent,
        system_total_memory_mb,
        system_used_memory_mb,
        thread_count,
        pid: pid_num,
    }
}

// Public constants for server version info (used by compute_metrics and potentially other modules)
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUILD_DATE: &str = match option_env!("BUILD_DATE") {
    Some(v) => v,
    None => "unknown",
};
pub const GIT_BRANCH: &str = match option_env!("GIT_BRANCH") {
    Some(v) => v,
    None => "unknown",
};
pub const GIT_COMMIT_HASH: &str = match option_env!("GIT_COMMIT") {
    Some(v) => v,
    None => "unknown",
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
    metrics.push(("schema_cache_hit_rate".to_string(), format!("{:.2}%", hit_rate * 100.0)));

    // Manifest Cache Metrics
    // Manifests in hot cache (memory)
    let manifests_in_memory = ctx.manifest_service().hot_cache_len();
    metrics.push(("manifests_in_memory".to_string(), manifests_in_memory.to_string()));

    // Manifests in RocksDB (persistent cache)
    if let Ok(manifests_in_rocksdb) = ctx.manifest_service().count() {
        metrics.push(("manifests_in_rocksdb".to_string(), manifests_in_rocksdb.to_string()));
    } else {
        metrics.push(("manifests_in_rocksdb".to_string(), "0".to_string()));
    }

    // Manifest cache breakdown (shared vs user tables)
    let (shared_manifests, user_manifests, total_weight) = ctx.manifest_service().cache_stats();
    metrics.push(("manifests_shared_tables".to_string(), shared_manifests.to_string()));
    metrics.push(("manifests_user_tables".to_string(), user_manifests.to_string()));
    metrics.push(("manifests_cache_weight".to_string(), total_weight.to_string()));
    metrics.push(("manifests_max_capacity".to_string(), ctx.manifest_service().max_weighted_capacity().to_string()));

    // Node ID
    metrics.push(("node_id".to_string(), ctx.node_id().to_string()));

    // Server build/version
    metrics.push(("server_version".to_string(), SERVER_VERSION.to_string()));
    metrics.push(("server_build_date".to_string(), BUILD_DATE.to_string()));
    metrics.push(("server_git_branch".to_string(), GIT_BRANCH.to_string()));
    metrics.push(("server_git_commit".to_string(), GIT_COMMIT_HASH.to_string()));

    // Cluster info
    let config = ctx.config();
    metrics.push((
        "cluster_mode".to_string(),
        config.cluster.is_some().to_string(),
    ));
    if let Some(cluster) = &config.cluster {
        metrics.push(("cluster_id".to_string(), cluster.cluster_id.clone()));
        metrics.push(("cluster_rpc_addr".to_string(), cluster.rpc_addr.clone()));
        metrics.push(("cluster_api_addr".to_string(), cluster.api_addr.clone()));
    }

    metrics
}
