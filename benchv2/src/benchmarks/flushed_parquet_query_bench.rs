use std::future::Future;
use std::pin::Pin;

use crate::benchmarks::Benchmark;
use crate::client::KalamClient;
use crate::config::Config;

/// Queries a shared table backed by 20 flushed Parquet files (~200K rows).
/// Setup: insert 10K rows + flush, repeated 20 times, then measure query latency.
/// This is a single_run benchmark since setup is expensive.
pub struct FlushedParquetQueryBench;

const PARQUET_FILES: u32 = 20;
const ROWS_PER_FLUSH: u32 = 10_000;
const BATCH_SIZE: u32 = 50;

impl Benchmark for FlushedParquetQueryBench {
    fn name(&self) -> &str {
        "flushed_parquet_query"
    }
    fn category(&self) -> &str {
        "Storage"
    }
    fn description(&self) -> &str {
        "SELECT from a shared table with 20 flushed Parquet files (200K rows)"
    }

    fn single_run(&self) -> bool {
        // Setup is very expensive (20 flush cycles), run once
        false
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
                .sql(&format!("DROP TABLE IF EXISTS {}.parquet_bench", config.namespace))
                .await;
            client
                .sql_ok(&format!(
                    "CREATE TABLE {}.parquet_bench (id INT PRIMARY KEY, category TEXT, amount INT, description TEXT)",
                    config.namespace
                ))
                .await?;

            // Insert 10K rows then flush — 20 times to create 20 Parquet files
            let mut global_id = 0u32;
            for file_num in 0..PARQUET_FILES {
                print!("\r    Loading Parquet file {}/{}...", file_num + 1, PARQUET_FILES);

                // Insert ROWS_PER_FLUSH in batches
                for _ in 0..(ROWS_PER_FLUSH / BATCH_SIZE) {
                    let mut values = Vec::new();
                    for _ in 0..BATCH_SIZE {
                        values.push(format!(
                            "({}, 'cat_{}', {}, 'desc_for_{}')",
                            global_id,
                            global_id % 100,
                            global_id * 7 % 10000,
                            global_id
                        ));
                        global_id += 1;
                    }
                    client
                        .sql_ok(&format!(
                            "INSERT INTO {}.parquet_bench (id, category, amount, description) VALUES {}",
                            config.namespace,
                            values.join(", ")
                        ))
                        .await?;
                }

                // Flush to create a Parquet file
                client
                    .sql_ok(&format!("STORAGE FLUSH TABLE {}.parquet_bench", config.namespace))
                    .await?;

                // Wait for flush job to complete
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            println!("\r    Loaded {} Parquet files ({} total rows)    ", PARQUET_FILES, global_id);
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
            // Run different query patterns across iterations
            match iteration % 5 {
                0 => {
                    // COUNT(*) across all parquet files
                    let resp = client
                        .sql_ok(&format!(
                            "SELECT COUNT(*) as cnt FROM {}.parquet_bench",
                            config.namespace
                        ))
                        .await?;
                    // Verify we get the expected count
                    if let Some(result) = resp.results.first() {
                        if let Some(rows) = &result.rows {
                            if let Some(first) = rows.first() {
                                if let Some(cnt) = first.first() {
                                    let count = cnt.as_i64().unwrap_or(0);
                                    if count < (PARQUET_FILES * ROWS_PER_FLUSH) as i64 {
                                        return Err(format!(
                                            "Expected >= {} rows, got {}",
                                            PARQUET_FILES * ROWS_PER_FLUSH,
                                            count
                                        ));
                                    }
                                }
                            }
                        }
                    }
                },
                1 => {
                    // Filter scan across all files
                    client
                        .sql_ok(&format!(
                            "SELECT * FROM {}.parquet_bench WHERE category = 'cat_42'",
                            config.namespace
                        ))
                        .await?;
                },
                2 => {
                    // Aggregation with GROUP BY
                    client
                        .sql_ok(&format!(
                            "SELECT category, SUM(amount) as total FROM {}.parquet_bench GROUP BY category ORDER BY total DESC LIMIT 10",
                            config.namespace
                        ))
                        .await?;
                },
                3 => {
                    // Point lookup (should hit specific parquet file)
                    client
                        .sql_ok(&format!(
                            "SELECT * FROM {}.parquet_bench WHERE id = 99999",
                            config.namespace
                        ))
                        .await?;
                },
                _ => {
                    // Range scan with ORDER BY + LIMIT
                    client
                        .sql_ok(&format!(
                            "SELECT * FROM {}.parquet_bench WHERE amount > 5000 ORDER BY amount DESC LIMIT 100",
                            config.namespace
                        ))
                        .await?;
                },
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
                .sql(&format!("DROP TABLE IF EXISTS {}.parquet_bench", config.namespace))
                .await;
            Ok(())
        })
    }
}
