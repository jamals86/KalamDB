use std::future::Future;
use std::pin::Pin;

use crate::benchmarks::Benchmark;
use crate::client::KalamClient;
use crate::config::Config;

/// Simulates a connection storm: N simultaneous login → SQL query → cycles.
/// Tests authentication subsystem, connection pool, and JWT overhead.
pub struct ConnectionStormBench;

impl Benchmark for ConnectionStormBench {
    fn name(&self) -> &str {
        "connection_storm"
    }
    fn category(&self) -> &str {
        "Load"
    }
    fn description(&self) -> &str {
        "N simultaneous login + SQL + cycles (connection setup overhead)"
    }

    fn setup<'a>(
        &'a self,
        client: &'a KalamClient,
        config: &'a Config,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            client
                .sql_ok(&format!("CREATE NAMESPACE IF NOT EXISTS {}", config.namespace))
                .await?;
            let _ = client
                .sql(&format!("DROP TABLE IF EXISTS {}.storm_bench", config.namespace))
                .await;
            client
                .sql_ok(&format!(
                    "CREATE TABLE {}.storm_bench (id INT PRIMARY KEY, val TEXT)",
                    config.namespace
                ))
                .await?;
            // Seed a few rows
            client
                .sql_ok(&format!(
                    "INSERT INTO {}.storm_bench (id, val) VALUES (1, 'a'), (2, 'b'), (3, 'c')",
                    config.namespace
                ))
                .await?;
            Ok(())
        })
    }

    fn run<'a>(
        &'a self,
        client: &'a KalamClient,
        config: &'a Config,
        _iteration: u32,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let conc = config.concurrency;
            let mut handles = Vec::new();
            let urls = client.urls();

            for i in 0..conc {
                let url = urls[(i as usize) % urls.len()].clone();
                let user = config.user.clone();
                let pass = config.password.clone();
                let ns = config.namespace.clone();

                handles.push(tokio::spawn(async move {
                    // Each task does a fresh login → query → independently
                    // Retry on 429 (rate limiting) with exponential backoff
                    let mut delay = std::time::Duration::from_millis(200);
                    let fresh = loop {
                        match KalamClient::login_single(&url, &user, &pass).await {
                            Ok(c) => break c,
                            Err(e) if e.contains("429") || e.contains("rate_limited") => {
                                tokio::time::sleep(delay).await;
                                delay = std::cmp::min(delay * 2, std::time::Duration::from_secs(5));
                            },
                            Err(e) => return Err(format!("Login storm failed: {}", e)),
                        }
                    };
                    fresh
                        .sql_ok(&format!("SELECT * FROM {}.storm_bench", ns))
                        .await
                        .map_err(|e| format!("Storm query failed: {}", e))?;
                    Ok::<(), String>(())
                }));
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
                .sql(&format!("DROP TABLE IF EXISTS {}.storm_bench", config.namespace))
                .await;
            Ok(())
        })
    }
}
