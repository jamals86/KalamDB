# Cluster Formation and Leadership

This document describes how KalamDB forms a cluster using OpenRaft-based multi-Raft groups, how leaders are determined, and how nodes join and leave the cluster.

## Multi-Raft Leadership Model
- Leadership is per Raft group, not per node. Each `GroupId` (Meta, DataUserShard[i], DataSharedShard[i]) elects its own leader.
- The Meta group leader is treated as the control-plane entry point. API write forwarding and background jobs consult Meta leadership to avoid duplicate writes/jobs, but data groups may be led by different nodes.
- Proposals are routed to the leader of the specific group. Followers forward to the group leader and wait for local apply to preserve read-your-writes.

## Bootstrap (First Node)
1. The first node starts all Raft groups (Meta + configured data shards + shared shards).
2. It initializes each group with a single-voter membership containing only itself.
3. Because it is the only voter, it becomes leader for every group after elections stabilize.
4. The node then waits for configured peers to be reachable before attempting membership changes.

## Adding Peers (Cluster Formation)
All membership changes happen from the node that currently leads every group (typically the first node right after bootstrap):
1. Wait for each peer's RPC endpoint to be reachable (avoid flooding logs with failed replications).
2. For each group (Meta, every User shard, every Shared shard):
   - Add the peer as a learner.
   - Wait for the learner to catch up to the leader's last log index or snapshot index (bounded by `replication_timeout`).
3. After catchup, promote the learner to a voter in every group.
4. At the end of this process, the peer is a full voter across all groups.

## Steady State Behavior
- Each group maintains its own leader; leadership can differ across groups as elections occur.
- Reads/writes:
  - Meta commands go to the Meta leader; followers forward requests.
  - Data commands go to the respective shard leader; followers forward requests.
- Background jobs run only on the Meta leader to prevent duplication; jobs are claimed via Meta Raft commands.
- SQL API write requests are forwarded to the Meta leader; reads can be served locally when safe.

## Graceful Shutdown / Leadership Transfer
- Before shutdown, a node attempts best-effort leadership transfer by triggering re-election (OpenRaft 0.9 has no explicit transfer API). If it was leading a group, other voters will elect a new leader after it leaves.

## Alignment with OpenRaft Multi-Raft
- Each Raft group is a distinct OpenRaft instance with its own log, state machine, and network factory.
- Group IDs map to unique `u64` values for OpenRaft; storage and snapshots are per group.
- Forwarding, membership changes, and metrics are all scoped to the individual group, matching OpenRaft's multi-raft KV example design.

## Operational Checklist
- Bootstrap: start first node, call cluster initialization once.
- Peer join: ensure the acting node currently leads all groups; add each peer as learner -> wait -> promote across every group.
- API/Jobs: treat Meta leader as control-plane entry; data shards may be led elsewhere.
- Monitoring: track per-group metrics; do not assume a single node leads all groups after formation.
