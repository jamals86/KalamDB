use std::future::Future;
use std::pin::Pin;

use crate::benchmarks::Benchmark;
use crate::client::KalamClient;
use crate::config::Config;

/// Runs concurrent reads and writes against the same table simultaneously.
/// Tests how write contention affects read latency and vice versa.
pub struct MixedReadWriteBench;

impl Benchmark for MixedReadWriteBench {
    fn name(&self) -> &str {
        "mixed_read_write"
    }
    fn category(&self) -> &str {
        "Load"
    }
    fn description(&self) -> &str {
        "50/50 concurrent reads + writes on same table (contention test)"
    }

    fn setup<'a>(
        &'a self,
        client: &'a KalamClient,
        config: &'a Config,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            client
                .sql_ok(&format!(
                    "CREATE NAMESPACE IF NOT EXISTS {}",
                    config.namespace
                ))
                .await?;
            let _ = client
                .sql(&format!(
                    "DROP TABLE IF EXISTS {}.mixed_bench",
                    config.namespace
                ))
                .await;
            client
                .sql_ok(&format!(
                    "CREATE TABLE {}.mixed_bench (id INT PRIMARY KEY, counter INT, label TEXT)",
                    config.namespace
                ))
                .await?;

            // Seed 200 rows
            for chunk in 0..4 {
                let mut values = Vec::new();
                for i in 0..50 {
                    let id = chunk * 50 + i;
                    values.push(format!("({}, 0, 'item_{}')", id, id));
                }
                client
                    .sql_ok(&format!(
                        "INSERT INTO {}.mixed_bench (id, counter, label) VALUES {}",
                        config.namespace,
                        values.join(", ")
                    ))
                    .await?;
            }
            Ok(())
        })
    }

    fn run<'a>(
        &'a self,
        client: &'a KalamClient,
        config: &'a Config,
        iteration: u32,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let half = config.concurrency / 2;
            let mut handles = Vec::new();

            // Writers: INSERT new rows
            for i in 0..half {
                let c = client.clone();
                let ns = config.namespace.clone();
                let id = 10_000 + iteration * config.concurrency + i;
                handles.push(tokio::spawn(async move {
                    c.sql_ok(&format!(
                        "INSERT INTO {}.mixed_bench (id, counter, label) VALUES ({}, {}, 'new_{}')",
                        ns, id, id, id
                    ))
                    .await
                }));
            }

            // Readers: SELECT with different patterns
            for i in 0..half {
                let c = client.clone();
                let ns = config.namespace.clone();
                let query = match i % 3 {
                    0 => format!("SELECT * FROM {}.mixed_bench WHERE id < 50", ns),
                    1 => format!("SELECT COUNT(*) FROM {}.mixed_bench", ns),
                    _ => format!(
                        "SELECT * FROM {}.mixed_bench ORDER BY id DESC LIMIT 20",
                        ns
                    ),
                };
                handles.push(tokio::spawn(
                    async move { c.sql_ok(&query).await },
                ));
            }

            for h in handles {
                h.await.map_err(|e| format!("Join: {}", e))??;
            }
            Ok(())
        })
    }

    fn teardown<'a>(
        &'a self,
        client: &'a KalamClient,
        config: &'a Config,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let _ = client
                .sql(&format!(
                    "DROP TABLE IF EXISTS {}.mixed_bench",
                    config.namespace
                ))
                .await;
            Ok(())
        })
    }
}
