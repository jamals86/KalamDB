#!/bin/bash
# KalamDB 3-Node Cluster Management Script
# 
# Usage:
#   ./cluster.sh start    - Start the 3-node cluster
#   ./cluster.sh stop     - Stop the cluster (preserves data)
#   ./cluster.sh restart  - Restart the cluster
#   ./cluster.sh status   - Check cluster health status
#   ./cluster.sh logs     - View logs from all nodes
#   ./cluster.sh logs N   - View logs from node N (1, 2, or 3)
#   ./cluster.sh clean    - Stop cluster and remove all data
#   ./cluster.sh shell N  - Open shell in node N
#   ./cluster.sh sql N    - Run SQL REPL connected to node N
#   ./cluster.sh test     - Run consistency test across all nodes

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Node ports
NODE1_HTTP=8081
NODE2_HTTP=8082
NODE3_HTTP=8083

print_header() {
    echo ""
    echo -e "${BLUE}╔═══════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║           KalamDB 3-Node Cluster Management                       ║${NC}"
    echo -e "${BLUE}╚═══════════════════════════════════════════════════════════════════╝${NC}"
    echo ""
}

check_docker() {
    if ! command -v docker &> /dev/null; then
        echo -e "${RED}Error: Docker is not installed${NC}"
        exit 1
    fi
    if ! docker info &> /dev/null; then
        echo -e "${RED}Error: Docker daemon is not running${NC}"
        exit 1
    fi
}

build_image() {
    print_header
    echo -e "${YELLOW}Building KalamDB Docker image from local source...${NC}"
    echo ""
    
    # Go to project root (two directories up from docker/cluster)
    PROJ_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
    echo "Project root: $PROJ_ROOT"
    
    # Build the image
    echo "Building image (this may take several minutes)..."
    docker build -f "$PROJ_ROOT/docker/backend/Dockerfile" -t jamals86/kalamdb:latest "$PROJ_ROOT"
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Image built successfully${NC}"
        echo ""
        echo "Image: jamals86/kalamdb:latest"
        docker images jamals86/kalamdb:latest --format "Size: {{.Size}}, Created: {{.CreatedSince}}"
    else
        echo -e "${RED}✗ Image build failed${NC}"
        exit 1
    fi
}

start_cluster() {
    print_header
    echo -e "${GREEN}Starting 3-node KalamDB cluster...${NC}"
    echo ""
    
    # Pull latest image if not present
    echo "Checking for kalamdb image..."
    if ! docker image inspect jamals86/kalamdb:latest &> /dev/null; then
        echo -e "${YELLOW}Pulling kalamdb image...${NC}"
        docker pull jamals86/kalamdb:latest
    fi
    
    # Start cluster
    docker compose up -d
    
    echo ""
    echo -e "${GREEN}Cluster starting...${NC}"
    echo ""
    echo "Waiting for nodes to be healthy..."
    
    # Wait for health checks
    for i in {1..30}; do
        node1_ok=$(curl -sf http://localhost:$NODE1_HTTP/v1/api/healthcheck 2>/dev/null && echo "1" || echo "0")
        node2_ok=$(curl -sf http://localhost:$NODE2_HTTP/v1/api/healthcheck 2>/dev/null && echo "1" || echo "0")
        node3_ok=$(curl -sf http://localhost:$NODE3_HTTP/v1/api/healthcheck 2>/dev/null && echo "1" || echo "0")
        
        if [[ "$node1_ok" == "1" && "$node2_ok" == "1" && "$node3_ok" == "1" ]]; then
            echo ""
            echo -e "${GREEN}✓ All nodes are healthy!${NC}"
            break
        fi
        
        echo -n "."
        sleep 2
    done
    
    echo ""
    show_status
}

stop_cluster() {
    print_header
    echo -e "${YELLOW}Stopping cluster (data preserved)...${NC}"
    docker compose down
    echo -e "${GREEN}✓ Cluster stopped${NC}"
}

restart_cluster() {
    print_header
    echo -e "${YELLOW}Restarting cluster...${NC}"
    docker compose restart
    echo -e "${GREEN}✓ Cluster restarted${NC}"
    sleep 5
    show_status
}

clean_cluster() {
    print_header
    echo -e "${RED}WARNING: This will delete all cluster data!${NC}"
    read -p "Are you sure? (y/N) " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        docker compose down -v
        echo -e "${GREEN}✓ Cluster stopped and data removed${NC}"
    else
        echo "Cancelled"
    fi
}

show_status() {
    print_header
    echo -e "${BLUE}Cluster Status:${NC}"
    echo ""
    
    # Check each node
    for node in 1 2 3; do
        port=$((8080 + node))
        container="kalamdb-node${node}"
        
        # Check if container is running
        if docker ps --format '{{.Names}}' | grep -q "^${container}$"; then
            # Check health
            if curl -sf "http://localhost:${port}/v1/api/healthcheck" &> /dev/null; then
                echo -e "  Node $node (port $port): ${GREEN}● Healthy${NC}"
            else
                echo -e "  Node $node (port $port): ${YELLOW}○ Starting...${NC}"
            fi
        else
            echo -e "  Node $node (port $port): ${RED}✗ Not running${NC}"
        fi
    done
    
    echo ""
    echo -e "${BLUE}Docker containers:${NC}"
    docker compose ps
    echo ""
    echo -e "${BLUE}Connection URLs:${NC}"
    echo "  Node 1: http://localhost:$NODE1_HTTP"
    echo "  Node 2: http://localhost:$NODE2_HTTP"
    echo "  Node 3: http://localhost:$NODE3_HTTP"
    echo ""
}

show_logs() {
    node=$1
    if [[ -n "$node" ]]; then
        echo -e "${BLUE}Logs for Node $node:${NC}"
        docker compose logs -f "kalamdb-node${node}"
    else
        echo -e "${BLUE}Logs for all nodes:${NC}"
        docker compose logs -f
    fi
}

open_shell() {
    node=$1
    if [[ -z "$node" || ! "$node" =~ ^[123]$ ]]; then
        echo -e "${RED}Usage: $0 shell <1|2|3>${NC}"
        exit 1
    fi
    echo -e "${BLUE}Opening shell in Node $node...${NC}"
    docker exec -it "kalamdb-node${node}" /bin/bash
}

run_sql() {
    node=$1
    if [[ -z "$node" || ! "$node" =~ ^[123]$ ]]; then
        echo -e "${RED}Usage: $0 sql <1|2|3>${NC}"
        exit 1
    fi
    port=$((8080 + node))
    echo -e "${BLUE}Connecting to Node $node (port $port)...${NC}"
    echo -e "${YELLOW}Note: Use your local kalam-cli or run SQL via curl:${NC}"
    echo ""
    echo "  # Using kalam-cli (if installed locally):"
    echo "  kalam-cli -u http://localhost:$port"
    echo ""
    echo "  # Using curl:"
    echo "  curl -X POST http://localhost:$port/v1/api/sql \\"
    echo "    -H 'Content-Type: application/json' \\"
    echo "    -d '{\"sql\": \"SELECT * FROM system.namespaces\"}'"
    echo ""
}

# Execute SQL on a node via docker exec (uses localhost auth from inside container)
run_sql_on_node() {
    local node=$1
    local sql=$2
    docker exec "kalamdb-node${node}" curl -sf -X POST "http://localhost:8080/v1/api/sql" \
        -u "root:" \
        -H "Content-Type: application/json" \
        -d "{\"sql\": \"$sql\"}" 2>/dev/null
}

run_test() {
    print_header
    echo -e "${BLUE}Running cluster consistency test...${NC}"
    echo ""
    
    # Check all nodes are healthy
    for node in 1 2 3; do
        port=$((8080 + node))
        if ! curl -sf "http://localhost:${port}/v1/api/healthcheck" &> /dev/null; then
            echo -e "${RED}Error: Node $node is not healthy${NC}"
            exit 1
        fi
    done
    echo -e "${GREEN}✓ All nodes healthy${NC}"
    
    # Create namespace on Node 1
    NS_NAME="cluster_test_$(date +%s)"
    echo ""
    echo "Creating namespace '$NS_NAME' on Node 1..."
    RESULT=$(run_sql_on_node 1 "CREATE NAMESPACE $NS_NAME")
    if echo "$RESULT" | grep -q '"status":"success"'; then
        echo -e "${GREEN}✓ Namespace created on Node 1${NC}"
    else
        echo -e "${RED}✗ Failed to create namespace on Node 1${NC}"
        echo "Response: $RESULT"
        exit 1
    fi
    
    # Wait for replication
    echo "Waiting for Raft replication..."
    sleep 3
    
    # Verify on Node 2
    echo "Checking namespace on Node 2..."
    RESULT2=$(run_sql_on_node 2 "SELECT namespace_id FROM system.namespaces WHERE namespace_id = '$NS_NAME'")
    if echo "$RESULT2" | grep -q "$NS_NAME"; then
        echo -e "${GREEN}✓ Namespace visible on Node 2${NC}"
    else
        echo -e "${RED}✗ Namespace NOT visible on Node 2${NC}"
        echo "Response: $RESULT2"
    fi
    
    # Verify on Node 3
    echo "Checking namespace on Node 3..."
    RESULT3=$(run_sql_on_node 3 "SELECT namespace_id FROM system.namespaces WHERE namespace_id = '$NS_NAME'")
    if echo "$RESULT3" | grep -q "$NS_NAME"; then
        echo -e "${GREEN}✓ Namespace visible on Node 3${NC}"
    else
        echo -e "${RED}✗ Namespace NOT visible on Node 3${NC}"
        echo "Response: $RESULT3"
    fi
    
    # Create table on Node 2
    echo ""
    echo "Creating table on Node 2..."
    RESULT=$(run_sql_on_node 2 "CREATE TABLE $NS_NAME.test_data (id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(), value TEXT) WITH (TYPE = 'SHARED')")
    if echo "$RESULT" | grep -q '"status":"success"'; then
        echo -e "${GREEN}✓ Table created on Node 2${NC}"
    else
        echo -e "${YELLOW}⚠ Table creation result: $RESULT${NC}"
    fi

    # Insert on Node 3
    echo "Inserting data on Node 3..."
    RESULT=$(run_sql_on_node 3 "INSERT INTO $NS_NAME.test_data (value) VALUES ('test_from_node3')")
    if echo "$RESULT" | grep -q '"status":"success"'; then
        echo -e "${GREEN}✓ Data inserted on Node 3${NC}"
    else
        echo -e "${YELLOW}⚠ Insert result: $RESULT${NC}"
    fi
    
    # Wait for replication
    sleep 2
    
    # Read on Node 1
    echo "Reading data on Node 1..."
    RESULT1=$(run_sql_on_node 1 "SELECT value FROM $NS_NAME.test_data WHERE value = 'test_from_node3'")
    if echo "$RESULT1" | grep -q "test_from_node3"; then
        echo -e "${GREEN}✓ Data visible on Node 1${NC}"
    else
        echo -e "${RED}✗ Data NOT visible on Node 1${NC}"
        echo "Response: $RESULT1"
    fi
    
    # Cleanup
    echo ""
    echo "Cleaning up test namespace..."
    run_sql_on_node 1 "DROP NAMESPACE $NS_NAME CASCADE" > /dev/null 2>&1
    echo -e "${GREEN}✓ Cleanup complete${NC}"
    
    echo ""
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo -e "${GREEN}  Cluster consistency test completed!${NC}"
    echo -e "${GREEN}═══════════════════════════════════════════════════════════════════${NC}"
    echo ""
}

show_help() {
    print_header
    echo "Usage: $0 <command> [options]"
    echo ""
    echo "Commands:"
    echo "  build     Build Docker image from local source"
    echo "  start     Start the 3-node cluster"
    echo "  stop      Stop the cluster (preserves data)"
    echo "  restart   Restart the cluster"
    echo "  status    Show cluster health status"
    echo "  logs      View logs from all nodes"
    echo "  logs N    View logs from node N (1, 2, or 3)"
    echo "  clean     Stop cluster and remove all data"
    echo "  shell N   Open bash shell in node N"
    echo "  sql N     Show how to connect SQL to node N"
    echo "  test      Run cluster consistency test"
    echo "  help      Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 build        # Build from local source"
    echo "  $0 start        # Start the cluster"
    echo "  $0 logs 1       # View logs for node 1"
    echo "  $0 shell 2      # Open shell in node 2"
    echo "  $0 test         # Test data replication"
    echo ""
}

# Main command dispatcher
check_docker

case "${1:-help}" in
    start)
        start_cluster
        ;;
    stop)
        stop_cluster
        ;;
    restart)
        restart_cluster
        ;;
    status)
        show_status
        ;;
    logs)
        show_logs "$2"
        ;;
    clean)
        clean_cluster
        ;;
    shell)
        open_shell "$2"
        ;;
    sql)
        run_sql "$2"
        ;;
    test)
        run_test
        ;;
    build)
        build_image
        ;;
    help|--help|-h)
        show_help
        ;;
    *)
        echo -e "${RED}Unknown command: $1${NC}"
        show_help
        exit 1
        ;;
esac
