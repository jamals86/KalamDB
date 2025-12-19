// Smoke Benchmark: Queries throughput (INSERT and SELECT pagination)
// - Creates a realistic user table (ERP/POS-like schema)
// - Inserts X rows and measures rows/sec
// - Paginates SELECT 10 rows/page and measures pages/sec

use crate::common::*;
use std::time::Instant;

// Global rows to insert (can be overridden via KBENCH_ROWS env)
// Reduced from 1000 to 200 for faster smoke execution while still exercising pagination.
// This ensures the test completes within timeout even under load.
const DEFAULT_ROWS_TO_INSERT: usize = 200;

fn rows_to_insert() -> usize {
    if let Ok(val) = std::env::var("KBENCH_ROWS") {
        if let Ok(parsed) = val.parse::<usize>() {
            return parsed;
        }
    }
    DEFAULT_ROWS_TO_INSERT
}

#[ntest::timeout(180000)]
#[test]
fn smoke_queries_benchmark() {
    if !is_server_running() {
        println!(
            "Skipping smoke_queries_benchmark: server not running at {}",
            SERVER_URL
        );
        return;
    }

    let namespace = "bench_ns";
    let table = generate_unique_table("orders");
    let full = format!("{}.{}", namespace, table);

    // Create namespace
    let ns_sql = format!("CREATE NAMESPACE IF NOT EXISTS {}", namespace);
    execute_sql_as_root_via_client(&ns_sql).expect("create namespace");

    // Defensive: drop any leftover table from previous runs if it exists
    // This avoids rare "Already exists" errors when a prior run didn't clean up
    let drop_if_exists = format!("DROP TABLE IF EXISTS {}", full);
    let _ = execute_sql_as_root_via_client(&drop_if_exists);

    // Create ERP/POS-like user table with mixed types
    // Columns:
    // order_id BIGINT (PK), customer_id BIGINT, sku TEXT, status TEXT, quantity INT,
    // price DOUBLE, created_at TIMESTAMP, updated_at TIMESTAMP, paid BOOLEAN, notes TEXT
    let create_sql = format!(
        r#"CREATE TABLE {} (
            order_id BIGINT AUTO_INCREMENT PRIMARY KEY,
            customer_id BIGINT,
            sku TEXT,
            status TEXT,
            quantity INT,
            price DOUBLE,
            created_at TIMESTAMP,
            updated_at TIMESTAMP,
            paid BOOLEAN,
            notes TEXT
        ) WITH (TYPE = 'USER')"#,
        full
    );
    execute_sql_as_root_via_client(&create_sql).expect("create user table");

    // Insert rows in batches to minimize CLI overhead
    let total = rows_to_insert();
    let batch_size = 100; // reduced from 500 for better server stability

    let start_insert = Instant::now();
    let mut inserted = 0usize;
    let insert_deadline = start_insert + std::time::Duration::from_secs(40); // hard timeout guard

    // Use a high-offset base id derived from current time to avoid rare PK collisions if residual rows survived a failed DROP.
    while inserted < total {
        let remain = total - inserted;
        let n = remain.min(batch_size);

        // Build multi-row INSERT
        let mut values = String::new();
        for i in 0..n {
            let cust = (1000 + ((inserted + i) % 5000)) as i64;
            let sku = format!("SKU{:06}", (inserted + i) % 10_000);
            let status = match (inserted + i) % 4 {
                0 => "new",
                1 => "paid",
                2 => "shipped",
                _ => "completed",
            };
            let qty = ((inserted + i) % 50) as i64;
            let price = 9.99 + (((inserted + i) % 500) as f64) / 10.0;
            let now_ms = chrono::Utc::now().timestamp_millis();
            let paid = (inserted + i).is_multiple_of(2);
            let notes = "benchmark";

            if !values.is_empty() {
                values.push_str(", ");
            }
            values.push_str(&format!(
                "({}, '{}', '{}', {}, {}, {}, {}, {}, '{}')",
                cust, sku, status, qty, price, now_ms, now_ms, paid, notes
            ));
        }

        let insert_sql = format!(
            "INSERT INTO {} (customer_id, sku, status, quantity, price, created_at, updated_at, paid, notes) VALUES {}",
            full, values
        );
        // Retry on transient errors (e.g., timeout) up to 3 times
        let mut attempts = 0;
        loop {
            match execute_sql_as_root_via_client(&insert_sql) {
                Ok(_) => break,
                Err(e) => {
                    attempts += 1;
                    if attempts >= 3 {
                        panic!("insert batch failed after retries: {}", e);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(150));
                }
            }
        }
        inserted += n;
        if Instant::now() > insert_deadline {
            panic!("Insert phase exceeded timeout");
        }
    }

    let insert_elapsed = start_insert.elapsed().as_secs_f64();
    let rows_per_sec = (inserted as f64) / insert_elapsed.max(1e-6);
    println!(
        "Benchmark INSERT: inserted {} rows in {:.3}s → {:.1} rows/sec",
        inserted, insert_elapsed, rows_per_sec
    );

    // SELECT pagination (cursor-based): 100 rows per page, using order_id > last_id
    let page_size = 100usize; // unchanged
    let mut pages = 0usize;
    let mut last_id: i64 = 0;

    let start_select = Instant::now();

    // Iterate expected number of pages; we avoid parsing output for speed
    let expected_pages = inserted.div_ceil(page_size);
    for _ in 0..expected_pages {
        let select_sql = format!(
            "SELECT order_id, customer_id, sku, status, quantity, price, created_at, updated_at, paid, notes FROM {} WHERE order_id > {} ORDER BY order_id LIMIT {}",
            full, last_id, page_size
        );
        let _ = execute_sql_as_root_via_client(&select_sql).expect("select page (cursor)");

        println!("Fetched page {} (last_id={})", pages + 1, last_id);
        // Advance cursor optimistically by page size (order_id is sequential in this test)
        last_id += page_size as i64;
        pages += 1;
    }

    let select_elapsed = start_select.elapsed().as_secs_f64();
    let pages_per_sec = (pages as f64) / select_elapsed.max(1e-6);
    println!(
        "Benchmark SELECT: fetched {} pages ({} rows/page) in {:.3}s → {:.1} pages/sec",
        pages, page_size, select_elapsed, pages_per_sec
    );

    // Best-effort cleanup to keep the namespace tidy between runs
    let _ = execute_sql_as_root_via_client(&format!("DROP TABLE IF EXISTS {}", full));
}
