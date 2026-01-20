pub mod auth_helper;
pub mod cluster;
pub mod consolidated_helpers;
pub mod fixtures;
pub mod flush;
pub mod flush_helpers;
pub mod http_server;
pub mod jobs;
pub mod query_helpers;
pub mod query_result_ext;
pub mod test_server;

// Re-export commonly used helpers
pub use consolidated_helpers::{
    assert_error_contains, assert_min_row_count, assert_no_duplicates, assert_query_has_results,
    assert_query_success, assert_row_count, assert_success, create_test_users, create_user_and_client,
    drain_initial_data, ensure_user_exists, find_files_recursive, get_column_index, get_count,
    get_count_value, get_first_i64, get_first_string, get_i64_value, get_response_rows,
    get_rows, get_string_value, json_to_i64, run_parallel_users, unique_namespace, unique_table,
    wait_for_ack, assert_manifest_exists, assert_parquet_exists_and_nonempty,
};
pub use query_helpers::get_count_value as query_helpers_get_count_value;
pub use query_result_ext::QueryResultTestExt;
pub use test_server::TestServer;
