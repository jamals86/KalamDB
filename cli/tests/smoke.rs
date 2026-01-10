// Aggregator for smoke tests to ensure Cargo picks them up
#[path = "smoke/smoke_test_00_parallel_query_burst.rs"]
mod smoke_test_00_parallel_query_burst;
#[path = "smoke/smoke_test_websocket_capacity.rs"]
mod smoke_test_websocket_capacity;
#[path = "smoke/chat_ai_example_smoke.rs"]
mod chat_ai_example_smoke;
mod common;
#[path = "smoke/smoke_test_all_datatypes.rs"]
mod smoke_test_all_datatypes;
#[path = "smoke/smoke_test_core_operations.rs"]
mod smoke_test_core_operations;
#[path = "smoke/smoke_test_custom_functions.rs"]
mod smoke_test_custom_functions;
#[path = "smoke/smoke_test_ddl_alter.rs"]
mod smoke_test_ddl_alter;
#[path = "smoke/smoke_test_dml_extended.rs"]
mod smoke_test_dml_extended;
#[path = "smoke/smoke_test_dml_wide_columns.rs"]
mod smoke_test_dml_wide_columns;
#[path = "smoke/smoke_test_flush_manifest.rs"]
mod smoke_test_flush_manifest;
#[path = "smoke/smoke_test_flush_operations.rs"]
mod smoke_test_flush_operations;
#[path = "smoke/smoke_test_flush_pk_integrity.rs"]
mod smoke_test_flush_pk_integrity;
#[path = "smoke/smoke_test_queries_benchmark.rs"]
mod smoke_test_queries_benchmark;
#[path = "smoke/smoke_test_shared_table_crud.rs"]
mod smoke_test_shared_table_crud;
#[path = "smoke/smoke_test_storage_templates.rs"]
mod smoke_test_storage_templates;
#[path = "smoke/smoke_test_stream_subscription.rs"]
mod smoke_test_stream_subscription;
#[path = "smoke/smoke_test_system_and_users.rs"]
mod smoke_test_system_and_users;
#[path = "smoke/smoke_test_system_tables_extended.rs"]
mod smoke_test_system_tables_extended;
#[path = "smoke/smoke_test_timing_output.rs"]
mod smoke_test_timing_output;
#[path = "smoke/smoke_test_user_table_rls.rs"]
mod smoke_test_user_table_rls;
#[path = "smoke/smoke_test_user_table_subscription.rs"]
mod smoke_test_user_table_subscription;
#[path = "smoke/smoke_test_insert_throughput.rs"]
mod smoke_test_insert_throughput;
#[path = "smoke/smoke_test_int64_precision.rs"]
mod smoke_test_int64_precision;
#[path = "smoke/smoke_test_subscription_advanced.rs"]
mod smoke_test_subscription_advanced;
#[path = "smoke/smoke_test_batch_control.rs"]
mod smoke_test_batch_control;
#[path = "smoke/smoke_test_as_user_impersonation.rs"]
mod smoke_test_as_user_impersonation;
#[path = "smoke/smoke_test_cli_commands.rs"]
mod smoke_test_cli_commands;
#[path = "smoke/smoke_test_cluster_operations.rs"]
mod smoke_test_cluster_operations;
#[path = "smoke/smoke_test_schema_history.rs"]
mod smoke_test_schema_history;
#[path = "smoke/smoke_test_alter_with_data.rs"]
mod smoke_test_alter_with_data;
