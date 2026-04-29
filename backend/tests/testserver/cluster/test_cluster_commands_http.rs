use anyhow::Result;
use kalam_client::models::ResponseStatus;

use super::test_support::http_server::start_http_test_server;

#[tokio::test]
#[ntest::timeout(60000)]
async fn test_cluster_commands_over_http() -> Result<()> {
    let server = start_http_test_server().await?;

    let result = async {
        let resp = server.execute_sql("CLUSTER LIST").await?;
        anyhow::ensure!(
            resp.status == ResponseStatus::Error,
            "CLUSTER LIST should be rejected as a CLI-only command, got {:?}",
            resp.status
        );

        let resp = server.execute_sql("SELECT cluster_id, node_id FROM system.cluster").await?;
        anyhow::ensure!(
            resp.status == ResponseStatus::Success,
            "system.cluster query failed: {:?}",
            resp.error
        );

        let resp = server.execute_sql("CLUSTER SNAPSHOT").await?;
        anyhow::ensure!(
            resp.status == ResponseStatus::Success,
            "CLUSTER SNAPSHOT failed: {:?}",
            resp.error
        );
        let snapshot_result = resp
            .results
            .first()
            .ok_or_else(|| anyhow::anyhow!("CLUSTER SNAPSHOT returned no result batch"))?;
        anyhow::ensure!(
            snapshot_result.schema.iter().any(|field| field.name == "action"),
            "CLUSTER SNAPSHOT result missing action column"
        );
        anyhow::ensure!(
            snapshot_result.schema.iter().any(|field| field.name == "group_id"),
            "CLUSTER SNAPSHOT result missing group_id column"
        );
        anyhow::ensure!(
            snapshot_result.row_count > 0,
            "CLUSTER SNAPSHOT should return at least one row"
        );

        let resp = server.execute_sql("CLUSTER REBALANCE").await?;
        anyhow::ensure!(
            resp.status == ResponseStatus::Success,
            "CLUSTER REBALANCE failed: {:?}",
            resp.error
        );

        let resp = server.execute_sql("CLUSTER CLEAR").await?;
        anyhow::ensure!(
            resp.status == ResponseStatus::Success,
            "CLUSTER CLEAR failed: {:?}",
            resp.error
        );
        let clear_result = resp
            .results
            .first()
            .ok_or_else(|| anyhow::anyhow!("CLUSTER CLEAR returned no result batch"))?;
        anyhow::ensure!(
            clear_result.schema.iter().any(|field| field.name == "snapshots_dir"),
            "CLUSTER CLEAR result missing snapshots_dir column"
        );
        anyhow::ensure!(
            clear_result.schema.iter().any(|field| field.name == "snapshots_cleared"),
            "CLUSTER CLEAR result missing snapshots_cleared column"
        );

        let resp = server.execute_sql("CLUSTER JOIN 127.0.0.1:9001").await?;
        anyhow::ensure!(
            resp.status == ResponseStatus::Error,
            "CLUSTER JOIN should return Error for malformed input, got {:?}",
            resp.status
        );

        let resp = server.execute_sql("CLUSTER LEAVE").await?;
        anyhow::ensure!(
            resp.status == ResponseStatus::Error,
            "CLUSTER LEAVE should return Error (command removed), got {:?}",
            resp.status
        );

        Ok(())
    }
    .await;

    server.shutdown().await;
    result
}
