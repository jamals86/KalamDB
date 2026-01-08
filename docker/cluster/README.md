# KalamDB 3-Node Cluster Setup

This directory contains Docker Compose configuration for running a 3-node KalamDB cluster with Raft consensus for testing distributed operations.

## Quick Start

```bash
# Start the Docker cluster (from project root)
./scripts/cluster.sh --docker start

# Check status
./scripts/cluster.sh --docker status

# View logs
./scripts/cluster.sh --docker logs

# Open shell in a node
./scripts/cluster.sh --docker shell 1

# Stop cluster
./scripts/cluster.sh --docker stop

# Clean (removes all data)
./scripts/cluster.sh --docker clean
```

## Building from Local Source

To test local code changes in the cluster:

```bash
# From the project root:
./scripts/cluster.sh --docker build

# This builds a Docker image from your local code and tags it as jamals86/kalamdb:latest
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Docker Network: kalamdb-cluster                 │
├─────────────────┬─────────────────┬─────────────────────────────────┤
│                 │                 │                                 │
│  ┌───────────┐  │  ┌───────────┐  │  ┌───────────┐                  │
│  │  Node 1   │  │  │  Node 2   │  │  │  Node 3   │                  │
│  │           │  │  │           │  │  │           │                  │
│  │ HTTP:8081 │◄─┼──┤ HTTP:8082 │◄─┼──┤ HTTP:8083 │  (Client Access) │
│  │ Raft:9091 │◄─┼──┤ Raft:9092 │◄─┼──┤ Raft:9093 │  (Raft Consensus)│
│  │           │  │  │           │  │  │           │                  │
│  └───────────┘  │  └───────────┘  │  └───────────┘                  │
│        │        │        │        │        │                        │
│        └────────┴────────┴────────┴────────┘                        │
│                    Raft Replication                                 │
└─────────────────────────────────────────────────────────────────────┘
```

## Port Mapping

| Node | HTTP API | Raft gRPC | Container Name |
|------|----------|-----------|----------------|
| 1    | 8081     | 9091      | kalamdb-node1  |
| 2    | 8082     | 9092      | kalamdb-node2  |
| 3    | 8083     | 9093      | kalamdb-node3  |

## Cluster Configuration

Each node has its own `server{N}.toml` configuration file:

- **server1.toml** - Node 1 configuration (node_id = 1)
- **server2.toml** - Node 2 configuration (node_id = 2)
- **server3.toml** - Node 3 configuration (node_id = 3)

All nodes share:
- Same `cluster_id`: `kalamdb-docker-cluster`
- Same `jwt_secret` for authentication
- Same member list
- Shared storage volume for `shared` tables (`/data/storage`)

### Sharding Configuration

For testing, the cluster uses reduced sharding:
- **4 user data shards** (production default: 32)
- **1 shared data shard**

Total Raft groups: 3 (meta) + 4 (user) + 1 (shared) = **8 groups**

## Commands

### Start Cluster
```bash
./cluster.sh start
```

Starts all 3 nodes and waits for health checks.

### Stop Cluster
```bash
./cluster.sh stop
```

Stops all nodes but preserves data volumes.

### View Status
```bash
./cluster.sh status
```

Shows health status and connection URLs for all nodes.

### View Logs
```bash
# All nodes
./cluster.sh logs

# Specific node
./cluster.sh logs 1
./cluster.sh logs 2
./cluster.sh logs 3
```

### Open Shell
```bash
./cluster.sh shell 1    # Shell in node 1
```

### Run Consistency Test
```bash
./cluster.sh test
```

This test:
1. Creates a namespace on Node 1
2. Verifies it's replicated to Nodes 2 and 3
3. Creates the shared table on all nodes
4. Inserts data on Node 3
5. Flushes the table on Node 3
6. Reads data from Node 1
7. Cleans up

### Clean Up
```bash
./cluster.sh clean
```

Stops cluster and **removes all data volumes**.

## Testing with CLI

Connect to any node using the kalam-cli:

```bash
# Connect to Node 1
kalam-cli -u http://localhost:8081

# Connect to Node 2
kalam-cli -u http://localhost:8082

# Connect to Node 3
kalam-cli -u http://localhost:8083
```

## Testing with curl

```bash
# Create namespace on Node 1
curl -X POST http://localhost:8081/v1/api/sql \
  -H "Content-Type: application/json" \
  -d '{"sql": "CREATE NAMESPACE test_cluster"}'

# Query on Node 2 (should see the namespace)
curl -X POST http://localhost:8082/v1/api/sql \
  -H "Content-Type: application/json" \
  -d '{"sql": "SELECT * FROM system.namespaces"}'

# Insert on Node 3
curl -X POST http://localhost:8083/v1/api/sql \
  -H "Content-Type: application/json" \
  -d '{"sql": "CREATE TABLE test_cluster.items (id BIGINT PRIMARY KEY, name TEXT) WITH (TYPE = '\''SHARED'\'')"}'
```

## Debugging

### Check Raft Logs
```bash
# View Raft-specific logs with debug level
RUST_LOG=info,kalamdb_raft=debug ./cluster.sh start
```

### Inspect Container
```bash
docker exec -it kalamdb-node1 /bin/bash
```

### Check RocksDB Data
```bash
docker exec kalamdb-node1 ls -la /data/rocksdb
```

### Network Inspection
```bash
docker network inspect cluster_kalamdb-cluster
```

## Fault Tolerance Testing

### Simulate Node Failure
```bash
# Stop node 2
docker stop kalamdb-node2

# Cluster should continue with nodes 1 and 3
./cluster.sh status

# Write data (should work - majority still available)
curl -X POST http://localhost:8081/v1/api/sql \
  -H "Content-Type: application/json" \
  -d '{"sql": "INSERT INTO test.data (value) VALUES ('\''during_failure'\'')"}'

# Restart node 2
docker start kalamdb-node2

# Node 2 should catch up automatically
sleep 5
./cluster.sh status
```

### Simulate Network Partition
```bash
# Isolate node 3 from the network
docker network disconnect cluster_kalamdb-cluster kalamdb-node3

# Reconnect
docker network connect cluster_kalamdb-cluster kalamdb-node3
```

## Requirements

- Docker 20.10+
- Docker Compose v2+
- ~1.5GB RAM (500MB per node)
- Ports 8081-8083 and 9091-9093 available

## Troubleshooting

### Nodes Not Starting
```bash
# Check container logs
docker logs kalamdb-node1

# Check if ports are in use
lsof -i :8081
lsof -i :9091
```

### Data Not Replicating
1. Check all nodes are healthy: `./cluster.sh status`
2. Check Raft logs: `./cluster.sh logs 1 | grep -i raft`
3. Verify cluster configuration matches in all server*.toml files

### Reset Everything
```bash
./cluster.sh clean
./cluster.sh start
```
