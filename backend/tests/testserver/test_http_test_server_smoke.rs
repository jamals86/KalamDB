//! Smoke test for the near-production HTTP server wiring.
//!
//! Lives under `tests/testserver` to group all HTTP-server-backed tests.

#[path = "../common/testserver/mod.rs"]
mod test_support;

#[tokio::test]
async fn test_http_test_server_executes_sql_over_http() {
    let server = test_support::http_server::get_global_server().await;
    let response = server.execute_sql("SELECT 1").await.expect("execute_sql failed");
    assert_eq!(response.status.to_string(), "success");
}
