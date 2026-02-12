export const SYSTEM_CLUSTER_QUERY = `
  SELECT
    cluster_id, node_id, role, status, rpc_addr, api_addr,
    is_self, is_leader, groups_leading, total_groups,
    current_term, last_applied_log, leader_last_log_index,
    snapshot_index, catchup_progress_pct, replication_lag,
    hostname, version, memory_mb, os, arch
  FROM system.cluster
  ORDER BY is_leader DESC, node_id ASC
`;
