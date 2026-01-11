//! Smoke test for the near-production HTTP server wiring.
//!
//! This uses a shared `tests/test_support` helper so individual test files don't
//! have to duplicate bootstrap + server startup.

use serde_json::json;

#[path = "test_support/mod.rs"]
mod test_support;

#[tokio::test]
async fn test_http_test_server_executes_sql_over_http() {
    test_support::http_server::with_http_test_server(|server| Box::pin(async move {
        // SQL endpoint requires Authorization. For localhost, root/system users accept empty password.
        let auth_header =
            test_support::http_server::HttpTestServer::basic_auth_header("root", "");

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v1/api/sql", server.base_url))
            .header("Authorization", auth_header)
            .json(&json!({ "sql": "SELECT 1" }))
            .send()
            .await
            .expect("Failed to send SQL request");

        assert!(resp.status().is_success(), "Expected 2xx, got {}", resp.status());
        Ok(())
    }))
    .await
    .expect("with_http_test_server");
}
