# Cluster Tests (backend/tests/cluster)

## Standard Steps
1. Arrange multi-node cluster fixtures.
2. Execute the cluster operation described by the test name.
3. Assert cluster state and replication behavior.

## Tests
- test_cluster_basic_crud_operations — [backend/tests/cluster/test_cluster_basic_operations.rs.disabled](backend/tests/cluster/test_cluster_basic_operations.rs.disabled#L10) (Disabled)
- test_cluster_node_offline_and_recovery — [backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled](backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled#L21) (Disabled)
- test_cluster_all_nodes_offline — [backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled](backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled#L196) (Disabled)
- test_cluster_execute_on_all_skips_offline — [backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled](backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled#L249) (Disabled)
