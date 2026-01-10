import { useState, useCallback } from 'react';
import { executeSql } from '../lib/kalam-client';

export interface ClusterNode {
  cluster_id: string;
  node_id: number;
  role: string;
  status: string;
  rpc_addr: string;
  api_addr: string;
  is_self: boolean;
  is_leader: boolean;
  groups_leading: number;
  total_groups: number;
  current_term: number | null;
  last_applied_log: number | null;
  leader_last_log_index: number | null;
  snapshot_index: number | null;
  catchup_progress_pct: number | null;
  replication_lag: number | null;
}

export interface ClusterHealth {
  healthy: boolean;
  totalNodes: number;
  activeNodes: number;
  offlineNodes: number;
  leaderNodes: number;
  followerNodes: number;
  joiningNodes: number;
  catchingUpNodes: number;
}

export function useCluster() {
  const [nodes, setNodes] = useState<ClusterNode[]>([]);
  const [health, setHealth] = useState<ClusterHealth | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchCluster = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const rows = await executeSql(`
        SELECT 
          cluster_id, node_id, role, status, rpc_addr, api_addr,
          is_self, is_leader, groups_leading, total_groups,
          current_term, last_applied_log, leader_last_log_index,
          snapshot_index, catchup_progress_pct, replication_lag
        FROM system.cluster
        ORDER BY is_leader DESC, node_id ASC
      `);
      
      const nodeList = rows.map((row) => ({
        cluster_id: String(row.cluster_id ?? ''),
        node_id: Number(row.node_id ?? 0),
        role: String(row.role ?? ''),
        status: String(row.status ?? ''),
        rpc_addr: String(row.rpc_addr ?? ''),
        api_addr: String(row.api_addr ?? ''),
        is_self: Boolean(row.is_self),
        is_leader: Boolean(row.is_leader),
        groups_leading: Number(row.groups_leading ?? 0),
        total_groups: Number(row.total_groups ?? 0),
        current_term: row.current_term !== null ? Number(row.current_term) : null,
        last_applied_log: row.last_applied_log !== null ? Number(row.last_applied_log) : null,
        leader_last_log_index: row.leader_last_log_index !== null ? Number(row.leader_last_log_index) : null,
        snapshot_index: row.snapshot_index !== null ? Number(row.snapshot_index) : null,
        catchup_progress_pct: row.catchup_progress_pct !== null ? Number(row.catchup_progress_pct) : null,
        replication_lag: row.replication_lag !== null ? Number(row.replication_lag) : null,
      }));
      
      setNodes(nodeList);

      // Calculate health metrics
      const totalNodes = nodeList.length;
      const activeNodes = nodeList.filter(n => n.status === 'active').length;
      const offlineNodes = nodeList.filter(n => n.status === 'offline').length;
      const leaderNodes = nodeList.filter(n => n.role === 'leader').length;
      const followerNodes = nodeList.filter(n => n.role === 'follower').length;
      const joiningNodes = nodeList.filter(n => n.status === 'joining').length;
      const catchingUpNodes = nodeList.filter(n => n.status === 'catching_up').length;

      setHealth({
        healthy: offlineNodes === 0 && joiningNodes === 0,
        totalNodes,
        activeNodes,
        offlineNodes,
        leaderNodes,
        followerNodes,
        joiningNodes,
        catchingUpNodes,
      });

      return nodeList;
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to fetch cluster information';
      setError(errorMessage);
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  return {
    nodes,
    health,
    isLoading,
    error,
    fetchCluster,
  };
}
