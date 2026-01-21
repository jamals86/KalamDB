# Disabled Tests

## Standard Steps
1. Review why the test is disabled.
2. Re-enable in a stable environment.
3. Verify the test passes and update expectations.

## Tests
- test_cluster_basic_crud_operations — [backend/tests/cluster/test_cluster_basic_operations.rs.disabled](backend/tests/cluster/test_cluster_basic_operations.rs.disabled#L10)
- test_cluster_node_offline_and_recovery — [backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled](backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled#L21)
- test_cluster_all_nodes_offline — [backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled](backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled#L196)
- test_cluster_execute_on_all_skips_offline — [backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled](backend/tests/cluster/test_cluster_node_recovery_and_sync.rs.disabled#L249)
- test_user_table_insert_select_minimal — [backend/tests/test_user_table_debug.rs.disabled](backend/tests/test_user_table_debug.rs.disabled#L11)
- test_user_table_subscription_debug — [backend/tests/test_user_table_debug.rs.disabled](backend/tests/test_user_table_debug.rs.disabled#L78)
