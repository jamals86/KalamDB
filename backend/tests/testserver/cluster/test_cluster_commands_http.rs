use anyhow::Result;
use kalam_link::models::ResponseStatus;

use super::test_support::http_server::start_http_test_server;

#[tokio::test]
#[ntest::timeout(60000)]
async fn test_cluster_commands_over_http() -> Result<()> {
    let server = start_http_test_server().await?;

    let result = async {
        let resp = server.execute_sql("CLUSTER LIST").await?;
        anyhow::ensure!(resp.status == ResponseStatus::Success, "CLUSTER LIST failed: {:?}", resp.error);

        let resp = server.execute_sql("CLUSTER FLUSH").await?;
        anyhow::ensure!(resp.status == ResponseStatus::Success, "CLUSTER FLUSH failed: {:?}", resp.error);

        let resp = server.execute_sql("CLUSTER CLEAR").await?;
        anyhow::ensure!(resp.status == ResponseStatus::Success, "CLUSTER CLEAR failed: {:?}", resp.error);

        let resp = server.execute_sql("CLUSTER JOIN 127.0.0.1:9001").await?;
        anyhow::ensure!(resp.status == ResponseStatus::Success, "CLUSTER JOIN failed: {:?}", resp.error);

        let resp = server.execute_sql("CLUSTER LEAVE").await?;
        anyhow::ensure!(resp.status == ResponseStatus::Success, "CLUSTER LEAVE failed: {:?}", resp.error);

        Ok(())
    }
    .await;

    server.shutdown().await;
    result
}
