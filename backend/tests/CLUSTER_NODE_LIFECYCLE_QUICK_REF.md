# Quick Reference: Cluster Node Lifecycle Methods

## Import
```rust
use crate::common::testserver::get_cluster_server;

let cluster = get_cluster_server().await;
```

## Node State Management

### Check if node is online
```rust
if cluster.is_node_online(node_idx).await? {
    println!("Node {} is online", node_idx);
}
```

### Take node offline (simulate failure)
```rust
cluster.take_node_offline(1).await?;  // Node 1 now offline
```

### Bring node back online
```rust
cluster.bring_node_online(1).await?;  // Node 1 now online
```

### Wait for node to come online
```rust
// Polls every 100ms, timeout after 10 seconds
cluster.wait_for_node_online(1).await?;
```

## SQL Execution

### Execute on random online node
```rust
let response = cluster.execute_sql_on_random(
    "INSERT INTO ns.table (id) VALUES (1)"
).await?;
```

### Execute on all online nodes
```rust
let responses = cluster.execute_sql_on_all(
    "SELECT * FROM ns.table"
).await?;
// responses.len() == number of online nodes
```

### Execute on all with node indices
```rust
let results = cluster.execute_sql_on_all_with_indices(
    "SELECT COUNT(*) FROM ns.table"
).await?;

for (node_idx, response) in results {
    println!("Node {}: {} rows", node_idx, response.results.len());
}
```

### Verify consistency across nodes
```rust
let is_consistent = cluster.verify_data_consistency(
    "SELECT * FROM ns.table"
).await?;

assert!(is_consistent, "Data should be consistent");
```

## Common Patterns

### Test Data Loss Recovery
```rust
// Create initial state
cluster.execute_sql_on_random("CREATE TABLE test(id INT)").await?;

// Take node offline
cluster.take_node_offline(1).await?;

// Make changes while node is down
cluster.execute_sql_on_random("INSERT INTO test VALUES (1)").await?;

// Node comes back
cluster.bring_node_online(1).await?;

// Verify it synced
assert!(cluster.verify_data_consistency("SELECT * FROM test").await?);
```

### Test Read Availability During Outage
```rust
cluster.take_node_offline(1).await?;

// Should still work with 2 nodes
let response = cluster.execute_sql_on_random("SELECT 1").await?;
assert!(response.success());

// Should fail when all offline
cluster.take_node_offline(0).await?;
cluster.take_node_offline(2).await?;
let err = cluster.execute_sql_on_random("SELECT 1").await;
assert!(err.is_err());
```

### Test Partition Handling
```rust
// Simulate network partition: Node 1 separated from others
cluster.take_node_offline(1).await?;

// Majority (nodes 0, 2) continues operating
cluster.execute_sql_on_random("INSERT INTO test VALUES (1)").await?;

// Rejoin
cluster.bring_node_online(1).await?;

// Verify catch-up
tokio::time::sleep(Duration::from_millis(500)).await;
assert!(cluster.verify_data_consistency("SELECT * FROM test").await?);
```

## Error Cases

### No online nodes
```rust
cluster.take_node_offline(0).await?;
cluster.take_node_offline(1).await?;
cluster.take_node_offline(2).await?;

let err = cluster.execute_sql_on_random("SELECT 1").await;
// Error: "No online nodes available in cluster"
assert!(err.is_err());
```

### Invalid node index
```rust
let err = cluster.take_node_offline(5).await;
// Error: "Node index 5 out of range (0-2)"
assert!(err.is_err());
```

### Node won't come online
```rust
cluster.take_node_offline(1).await?;
let err = cluster.wait_for_node_online(1).await;
// Note: This will timeout because we're only simulating offline state
// In real scenario with actual server shutdown, it would wait for actual recovery
```

## Debugging

### Check all node states
```rust
for i in 0..3 {
    let is_online = cluster.is_node_online(i).await?;
    println!("Node {}: {}", i, if is_online { "ONLINE" } else { "OFFLINE" });
}
```

### Execute on specific node
```rust
let node = cluster.get_node(1)?;
let response = node.execute_sql("SELECT 1").await?;
```

### View data on each node
```rust
let results = cluster.execute_sql_on_all_with_indices(
    "SELECT * FROM ns.table"
).await?;

for (node_idx, response) in results {
    let rows = response.rows_as_maps();
    eprintln!("Node {} has {} rows:", node_idx, rows.len());
    for row in rows {
        eprintln!("  {:?}", row);
    }
}
```

## Testing Tips

1. **Always wait after schema changes:**
   ```rust
   cluster.execute_sql_on_random("CREATE TABLE ...").await?;
   tokio::time::sleep(Duration::from_millis(200)).await; // Wait for replication
   ```

2. **Use `verify_data_consistency()` after each major operation:**
   ```rust
   cluster.execute_sql_on_random("INSERT ...").await?;
   tokio::time::sleep(Duration::from_millis(100)).await;
   assert!(cluster.verify_data_consistency("SELECT * FROM table").await?);
   ```

3. **Create unique namespaces per test to avoid conflicts:**
   ```rust
   let test_ns = format!("test_ns_{}", std::process::id());
   cluster.execute_sql_on_random(&format!("CREATE NAMESPACE {}", test_ns)).await?;
   ```

4. **Take nodes offline one at a time to test different scenarios:**
   ```rust
   // Single node failure
   cluster.take_node_offline(0).await?;
   
   // Two nodes down (partition)
   cluster.take_node_offline(1).await?;
   ```

## Performance Notes

- `execute_sql_on_random()`: Fast, minimal overhead, randomized to avoid hot spots
- `execute_sql_on_all()`: Slower (3 calls in sequence), waits for all to complete
- `verify_data_consistency()`: Slowest (multiple calls + comparison), use sparingly
- Offline/online state changes are instant (no actual shutdown, just flag flip)
- Sleep between operations: RocksDB writes have slight latency, use 100-200ms between DDL/DML

## Cluster Reuse Note

The global cluster server is reused across tests. If your test modifies state:
- Use unique table/namespace names to avoid conflicts
- Be aware other tests may query your tables
- Consider using dedicated test isolation if needed
