//! Smoke test for the near-production HTTP server wiring.
//!
//! Lives under `tests/testserver` to group all HTTP-server-backed tests.

#[path = "../common/testserver/mod.rs"]
mod test_support;

use tokio::time::Duration;

#[tokio::test]
async fn test_http_test_server_executes_sql_over_http() {
    test_support::http_server::with_http_test_server_timeout(Duration::from_secs(20), |server| {
        Box::pin(async move {
        let response = server.execute_sql("SELECT 1").await?;
        assert_eq!(response.status.to_string(), "success");
        Ok(())
        })
    })
    .await
    .expect("with_http_test_server_timeout");
}
