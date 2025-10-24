/// Simple Query Example
///
/// This example demonstrates basic SQL query execution using kalam-link.
/// It shows how to:
/// - Create a client
/// - Connect to the server
/// - Execute SQL queries
/// - Handle responses
///
/// Run with:
/// ```bash
/// cargo run --example simple_query
/// ```
use kalam_link::{KalamLinkClient, KalamLinkError};

#[tokio::main]
async fn main() -> Result<(), KalamLinkError> {
    println!("🚀 Kalam-Link Simple Query Example\n");

    // Create client with default configuration
    let client = KalamLinkClient::builder()
        .base_url("http://localhost:3000")
        .build()?;

    println!("📡 Connected to localhost:3000");

    // Example 1: Create namespace
    println!("\n1️⃣ Creating namespace 'example'...");
    let response = client.execute_query("CREATE NAMESPACE example").await?;
    println!("   Status: {}", response.status);

    // Example 2: Create table
    println!("\n2️⃣ Creating table 'example.users'...");
    let create_table_sql = r#"
        CREATE USER TABLE example.users (
            id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
            username TEXT NOT NULL,
            email TEXT,
            created_at TIMESTAMP DEFAULT NOW()
        )
    "#;
    let response = client.execute_query(create_table_sql).await?;
    println!("   Status: {}", response.status);

    // Example 3: Insert data
    println!("\n3️⃣ Inserting users...");
    let insert_sql = r#"
        INSERT INTO example.users (username, email) VALUES 
            ('alice', 'alice@example.com'),
            ('bob', 'bob@example.com'),
            ('charlie', 'charlie@example.com')
    "#;
    let response = client.execute_query(insert_sql).await?;
    println!("   Status: {}", response.status);

    // Example 4: Query data
    println!("\n4️⃣ Querying all users...");
    let response = client.execute_query("SELECT * FROM example.users").await?;

    println!("   Status: {}", response.status);
    if let Some(exec_time) = response.execution_time_ms {
        println!("   Execution time: {} ms", exec_time);
    }

    if !response.results.is_empty() {
        let result = &response.results[0];
        println!("   Found {} records:", result.row_count);
        if let Some(rows) = &result.rows {
            for (i, row) in rows.iter().enumerate() {
                println!("   {}. {:?}", i + 1, row);
            }
        }
    }

    // Example 5: Query with filter
    println!("\n5️⃣ Querying user 'alice'...");
    let response = client
        .execute_query("SELECT username, email FROM example.users WHERE username = 'alice'")
        .await?;

    println!("   Status: {}", response.status);
    if let Some(first_result) = response.results.first() {
        if let Some(rows) = &first_result.rows {
            println!("   Result: {:?}", rows.first());
        }
    }

    // Example 6: Update data
    println!("\n6️⃣ Updating alice's email...");
    let response = client
        .execute_query(
            "UPDATE example.users SET email = 'alice.updated@example.com' WHERE username = 'alice'",
        )
        .await?;
    println!("   Status: {}", response.status);

    // Example 7: Delete data
    println!("\n7️⃣ Deleting user 'charlie'...");
    let response = client
        .execute_query("DELETE FROM example.users WHERE username = 'charlie'")
        .await?;
    println!("   Status: {}", response.status);

    // Example 8: Query system tables
    println!("\n8️⃣ Querying system tables...");
    let response = client
        .execute_query(
            "SELECT table_name, table_type FROM system.tables WHERE namespace = 'example'",
        )
        .await?;

    println!("   Status: {}", response.status);
    if let Some(result) = response.results.first() {
        if let Some(rows) = &result.rows {
            println!("   Tables in 'example' namespace:");
            for row in rows {
                println!("   - {:?}", row);
            }
        }
    }

    // Example 9: Error handling
    println!("\n9️⃣ Handling errors...");
    let result = client
        .execute_query("SELECT * FROM nonexistent.table")
        .await;

    match result {
        Ok(_) => println!("   Unexpected success"),
        Err(e) => println!("   ✅ Caught error: {}", e),
    }

    // Example 10: Cleanup
    println!("\n🔟 Cleaning up...");
    let response = client.execute_query("DROP TABLE example.users").await?;
    println!("   Status: {}", response.status);

    let response = client.execute_query("DROP NAMESPACE example").await?;
    println!("   Status: {}", response.status);

    println!("\n✅ Example completed successfully!");

    Ok(())
}
