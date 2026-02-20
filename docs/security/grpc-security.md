# gRPC Security and Unauthorized Access Protection

This document explains how KalamDB protects inter-node gRPC traffic, what bypass attempts are blocked, and what operators must configure.

## 1) Inter-node gRPC surfaces

KalamDB’s Raft gRPC server hosts:

- **Raft consensus RPCs** (`raft_rpc`, `client_proposal`)
- **Cluster RPCs** (`forward_sql`, `notify_followers`, `ping`, `get_node_info`)

## 2) Core anti-bypass controls

### A) Cluster identity + node identity verification

For Raft RPC paths, KalamDB checks:

- `x-kalamdb-cluster-id` metadata matches the local cluster
- `x-kalamdb-node-id` metadata is present/valid
- Node ID exists in known cluster membership
- Remote source address matches allowed peer address policy

Requests failing these checks are rejected.

### B) mTLS trust boundary (recommended and required for strong production security)

When `cluster.rpc_tls.enabled = true`:

- Server requires valid peer TLS certificates signed by the cluster CA
- Missing/invalid peer certs are rejected
- Peer/server name verification is enforced via `rpc_server_name`

This prevents unauthorized hosts from participating in inter-node RPC even if they can reach the port.

### C) Forwarded SQL has an additional auth layer

`forward_sql` requires a valid `Authorization: Bearer <jwt>` header and rejects:

- Missing auth header
- Empty/malformed credentials
- Non-Bearer credentials (e.g., Basic)
- Forged/invalid tokens

The forwarded token is authenticated and then translated into an authenticated execution session before SQL runs.

## 3) How KalamDB prevents unauthorized user data access

KalamDB uses layered controls:

1. **Authentication extraction and verification** at API boundaries (invalid tokens fail with 401/403 behavior).
2. **JWT claim validation** including signature/expiry/issuer checks.
3. **Role anti-tampering check** where claimed JWT role must match stored user role.
4. **SQL authorization checks** where privileged statements require privileged roles.
5. **Session-based execution context** so statement handling sees authenticated identity and role.

This combination prevents common abuse patterns like forged role claims, token misuse, or direct unauthenticated write attempts.

## 4) Security tests that document expected behavior

The Raft security suite (`backend/crates/kalamdb-raft/tests/cluster_rpc_security.rs`) covers:

- Rejection of missing/empty/malformed/Basic/forged Bearer headers on `forward_sql`
- Oversized payload rejection behavior
- Replay-like invalid token attempts
- Controlled behavior of unauthenticated read-only cluster RPC calls
- Sensitive field exposure checks for node info responses

These tests are important evidence that bypass paths are explicitly exercised.

## 5) Important operator note: “secured enough” depends on configuration

KalamDB has strong building blocks, but production security depends on your deployment settings.

For production, treat these as mandatory:

- Enable cluster `rpc_tls`
- Restrict Raft RPC port to cluster network only (security groups/firewall)
- Enforce TLS for client API plane (edge proxy)
- Use strong JWT secret + issuer restrictions
- Keep rate limits and request limits enabled

Without mTLS + network segmentation, cluster RPC endpoints are easier to probe and should not be considered production-grade secure.
