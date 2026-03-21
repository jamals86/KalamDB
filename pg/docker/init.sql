-- KalamDB PostgreSQL Extension — Remote Initialization Script
-- Runs automatically on first container start via docker-entrypoint-initdb.d

-- 1. Install the extension
CREATE EXTENSION IF NOT EXISTS pg_kalam;

-- 2. Create the remote foreign server pointing to the host KalamDB gRPC port.
CREATE SERVER IF NOT EXISTS kalam_server
	FOREIGN DATA WRAPPER pg_kalam
	OPTIONS (
		host 'host.docker.internal',
		port '9188',
		auth_header 'Bearer kalamdb-local-dev-2026-02-23-strong-jwt-secret-32plus'
	);
