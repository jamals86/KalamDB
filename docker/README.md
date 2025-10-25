# Docker Deployment Guide

**Feature**: 006-docker-wasm-examples  
**Purpose**: Docker containerization for KalamDB backend with CLI included

---

## Overview

This directory contains Docker configurations for deploying KalamDB in containerized environments.

### Directory Structure

```
docker/
└── backend/
    ├── Dockerfile           # Multi-stage build for kalamdb-server + kalam-cli
    ├── docker-compose.yml   # Docker Compose configuration
    ├── build-backend.sh     # Build script with verification
    └── .env.example         # Example environment variables (create as .env)
```

---

## Quick Start

### 1. Build Image

```bash
cd docker/backend
./build-backend.sh
```

This creates `kalamdb:latest` with both server and CLI binaries.

### 2. Start Container

```bash
docker-compose up -d
```

### 3. Create User

```bash
docker exec -it kalamdb kalam-cli user create --name "myuser" --role "user"
```

**Save the API key** - you'll need it for authentication!

### 4. Test

```bash
# Health check
curl http://localhost:8080/health

# Query with API key
curl -X POST http://localhost:8080/sql \
  -H "Content-Type: application/json" \
  -H "X-API-KEY: <your-api-key-here>" \
  -d '{"query": "SELECT 1"}'
```

---

## Build Options

### Custom Tag

```bash
./build-backend.sh --tag myregistry.com/kalamdb:v1.0.0
```

### No Cache

```bash
./build-backend.sh --no-cache
```

### Build and Push

```bash
./build-backend.sh --tag myregistry.com/kalamdb:v1.0.0 --push
```

---

## Environment Variables

Create a `.env` file in `docker/backend/` to override defaults:

```bash
# Server configuration
KALAMDB_SERVER_PORT=8080
KALAMDB_SERVER_HOST=0.0.0.0

# Data directory (inside container)
KALAMDB_DATA_DIR=/data

# Logging
KALAMDB_LOG_LEVEL=info
KALAMDB_LOG_FILE=/data/logs/kalamdb.log
KALAMDB_LOG_TO_CONSOLE=true

# Connection limits
KALAMDB_MAX_CONNECTIONS=100
```

**Supported log levels**: `debug`, `info`, `warn`, `error`

---

## Volume Management

### Default Volume

```bash
# Inspect volume
docker volume inspect kalamdb_data

# Backup data
docker run --rm -v kalamdb_data:/data -v $(pwd):/backup alpine tar czf /backup/kalamdb-backup.tar.gz /data

# Restore data
docker run --rm -v kalamdb_data:/data -v $(pwd):/backup alpine tar xzf /backup/kalamdb-backup.tar.gz -C /
```

### Custom Host Path

Edit `docker-compose.yml`:

```yaml
volumes:
  kalamdb_data:
    driver: local
    driver_opts:
      type: none
      o: bind
      device: /path/to/host/data  # Your custom path
```

---

## Using CLI Inside Container

### Interactive Mode

```bash
docker exec -it kalamdb kalam-cli
```

### Execute Single Command

```bash
docker exec kalamdb kalam-cli exec "SELECT * FROM system.users"
```

### Load SQL File

```bash
# Copy file into container
docker cp schema.sql kalamdb:/tmp/schema.sql

# Execute
docker exec kalamdb kalam-cli load /tmp/schema.sql
```

---

## Production Deployment

### Multi-Node Setup

Use Docker Swarm or Kubernetes for multi-node deployments:

```bash
# Docker Swarm example
docker swarm init
docker stack deploy -c docker-compose.yml kalamdb-stack
```

### TLS/SSL

Mount certificates and update environment:

```yaml
volumes:
  - ./certs:/etc/kalamdb/certs:ro
environment:
  KALAMDB_TLS_CERT: /etc/kalamdb/certs/server.crt
  KALAMDB_TLS_KEY: /etc/kalamdb/certs/server.key
```

### Resource Limits

```yaml
services:
  kalamdb:
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 4G
        reservations:
          cpus: '1'
          memory: 2G
```

---

## Troubleshooting

### Container Won't Start

```bash
# Check logs
docker-compose logs kalamdb

# Common issues:
# - Port 8080 already in use
# - Volume permission errors
# - Config file errors
```

### Data Not Persisting

```bash
# Verify volume exists
docker volume ls | grep kalamdb_data

# Check mount points
docker inspect kalamdb | grep -A 10 "Mounts"
```

### Performance Issues

```bash
# Check resource usage
docker stats kalamdb

# Increase resources in docker-compose.yml (see Production Deployment)
```

### API Key Issues

```bash
# List users (requires localhost or API key)
docker exec kalamdb kalam-cli exec "SELECT user_id, username, role FROM system.users"

# Create new user
docker exec kalamdb kalam-cli user create --name "newuser" --role "user"
```

---

## Advanced Configuration

### Custom Dockerfile

If you need to modify the build:

1. Copy `Dockerfile` to `Dockerfile.custom`
2. Make your changes
3. Build: `docker build -f Dockerfile.custom -t kalamdb:custom ../../`

### Debug Mode

Run with debug logging:

```bash
docker-compose up -d
docker-compose exec kalamdb kalam-cli exec "UPDATE system.config SET log_level='debug'"
docker-compose restart
```

### Health Check Customization

Edit `docker-compose.yml`:

```yaml
healthcheck:
  test: ["CMD", "/usr/local/bin/kalam-cli", "exec", "SELECT COUNT(*) FROM system.users"]
  interval: 10s
  timeout: 5s
  start_period: 30s
  retries: 5
```

---

## Related Documentation

- [Quick Start Guide](../../specs/006-docker-wasm-examples/quickstart.md)
- [Data Model](../../specs/006-docker-wasm-examples/data-model.md)
- [HTTP Auth Contract](../../specs/006-docker-wasm-examples/contracts/http-auth.md)
- [Backend Documentation](../../backend/README.md)

---

## Support

For issues and questions:
- GitHub Issues: https://github.com/yourusername/KalamDB/issues
- Documentation: https://kalamdb.dev/docs
