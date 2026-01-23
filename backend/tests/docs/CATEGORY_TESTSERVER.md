# Testserver HTTP Tests (backend/tests/testserver)

## Standard Steps
1. Start the test server and establish HTTP clients.
2. Execute the API call(s) described by the test name.
3. Assert response codes, payloads, and server-side effects.

## Tests
- test_cluster_commands_over_http — [backend/tests/testserver/cluster/test_cluster_commands_http.rs](backend/tests/testserver/cluster/test_cluster_commands_http.rs#L8)
- test_cluster_snapshot_creation_and_reuse — [backend/tests/testserver/cluster/test_cluster_snapshots_http.rs](backend/tests/testserver/cluster/test_cluster_snapshots_http.rs#L88)
- test_system_cluster_views_over_http — [backend/tests/testserver/cluster/test_cluster_views_http.rs](backend/tests/testserver/cluster/test_cluster_views_http.rs#L8)
- test_flush_table_persists_job_over_http — [backend/tests/testserver/flush/test_flush_jobs_http.rs](backend/tests/testserver/flush/test_flush_jobs_http.rs#L8)
- test_flush_policy_and_parquet_output_over_http — [backend/tests/testserver/flush/test_flush_policy_verification_http.rs](backend/tests/testserver/flush/test_flush_policy_verification_http.rs#L60)
- test_flush_concurrency_and_correctness_over_http — [backend/tests/testserver/flush/test_flush_unregistered_suite_http.rs](backend/tests/testserver/flush/test_flush_unregistered_suite_http.rs#L183)
- test_pk_uniqueness_hot_and_cold_over_http — [backend/tests/testserver/flush/test_pk_uniqueness_hot_cold_http.rs](backend/tests/testserver/flush/test_pk_uniqueness_hot_cold_http.rs#L75)
- test_shared_flush_creates_manifest_json_over_http — [backend/tests/testserver/manifest/test_manifest_flush_http_v2.rs](backend/tests/testserver/manifest/test_manifest_flush_http_v2.rs#L67)
- test_user_table_manifest_persistence_over_http — [backend/tests/testserver/manifest/test_manifest_persistence_http.rs](backend/tests/testserver/manifest/test_manifest_persistence_http.rs#L65)
- test_observability_system_tables_and_jobs_over_http — [backend/tests/testserver/observability/test_production_observability_http.rs](backend/tests/testserver/observability/test_production_observability_http.rs#L10)
- test_parameterized_dml_over_http — [backend/tests/testserver/sql/test_dml_parameters_http.rs](backend/tests/testserver/sql/test_dml_parameters_http.rs#L51)
- test_namespace_validation_over_http — [backend/tests/testserver/sql/test_namespace_validation_http.rs](backend/tests/testserver/sql/test_namespace_validation_http.rs#L10)
- test_naming_validation_over_http — [backend/tests/testserver/sql/test_naming_validation_http.rs](backend/tests/testserver/sql/test_naming_validation_http.rs#L7)
- test_quickstart_workflow_over_http — [backend/tests/testserver/sql/test_quickstart_http.rs](backend/tests/testserver/sql/test_quickstart_http.rs#L21)
- test_user_sql_commands_over_http — [backend/tests/testserver/sql/test_user_sql_commands_http.rs](backend/tests/testserver/sql/test_user_sql_commands_http.rs#L11)
- test_storage_abstraction_over_http — [backend/tests/testserver/storage/test_storage_abstraction_http.rs](backend/tests/testserver/storage/test_storage_abstraction_http.rs#L25)
- test_storage_management_over_http — [backend/tests/testserver/storage/test_storage_management_http.rs](backend/tests/testserver/storage/test_storage_management_http.rs#L12)
- test_stress_smoke_over_http — [backend/tests/testserver/stress/test_stress_and_memory_http.rs](backend/tests/testserver/stress/test_stress_and_memory_http.rs#L50)
- test_live_query_detects_deletes — [backend/tests/testserver/subscription/test_live_query_deletes.rs](backend/tests/testserver/subscription/test_live_query_deletes.rs#L9)
- test_live_query_detects_inserts — [backend/tests/testserver/subscription/test_live_query_inserts.rs](backend/tests/testserver/subscription/test_live_query_inserts.rs#L10)
- test_live_query_detects_updates — [backend/tests/testserver/subscription/test_live_query_updates.rs](backend/tests/testserver/subscription/test_live_query_updates.rs#L9)
- test_stream_ttl_eviction_from_sql_script — [backend/tests/testserver/subscription/test_stream_ttl_eviction_sql.rs](backend/tests/testserver/subscription/test_stream_ttl_eviction_sql.rs#L11)
- test_system_tables_queryable_over_http — [backend/tests/testserver/system/test_system_tables_http.rs](backend/tests/testserver/system/test_system_tables_http.rs#L21)
- test_shared_tables_lifecycle_over_http — [backend/tests/testserver/tables/test_shared_tables_http.rs](backend/tests/testserver/tables/test_shared_tables_http.rs#L11)
- test_stream_tables_over_http — [backend/tests/testserver/tables/test_stream_tables_http.rs](backend/tests/testserver/tables/test_stream_tables_http.rs#L32)
- test_user_tables_lifecycle_and_isolation_over_http — [backend/tests/testserver/tables/test_user_tables_http.rs](backend/tests/testserver/tables/test_user_tables_http.rs#L47)
- test_http_test_server_executes_sql_over_http — [backend/tests/testserver/test_http_test_server_smoke.rs](backend/tests/testserver/test_http_test_server_smoke.rs#L6)
