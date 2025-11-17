// Aggregator for smoke tests to ensure Cargo picks them up
mod common;
#[path = "smoke/smoke_test_user_table_rls.rs"]
mod smoke_test_user_table_rls;
#[path = "smoke/smoke_test_user_table_subscription.rs"]
mod smoke_test_user_table_subscription;
#[path = "smoke/smoke_test_shared_table_crud.rs"]
mod smoke_test_shared_table_crud;
#[path = "smoke/smoke_test_system_and_users.rs"]
mod smoke_test_system_and_users;
#[path = "smoke/smoke_test_stream_subscription.rs"]
mod smoke_test_stream_subscription;
#[path = "smoke/smoke_test_core_operations.rs"]
mod smoke_test_core_operations;
#[path = "smoke/smoke_test_flush_operations.rs"]
mod smoke_test_flush_operations;
#[path = "smoke/smoke_test_queries_benchmark.rs"]
mod smoke_test_queries_benchmark;
#[path = "smoke/smoke_test_dml_wide_columns.rs"]
mod smoke_test_dml_wide_columns;
#[path = "smoke/smoke_test_storage_templates.rs"]
mod smoke_test_storage_templates;
#[path = "smoke/chat_ai_example_smoke.rs"]
mod chat_ai_example_smoke;
#[path = "smoke/smoke_test_all_datatypes.rs"]
mod smoke_test_all_datatypes;
