# KalamDB Durable Topics (RocksDB) – Design Spec

This spec adds **durable, internal topics** to KalamDB (Kafka-ish) using the **same RocksDB backend**.

Goal: when rows are inserted/updated/deleted in tables, KalamDB can **append change events** into one or more **topics** that **agents/services** consume with **durable offsets**.

---

## 1) Mental model

**Table write → ChangeEvent → TopicRouter → TopicLog (RocksDB) → Consumers read (by offset) → ACK updates offsets**

* UI/WebSocket subscriptions: low-latency push.
* Topics: **durable pull** with replay.

---

## 2) Core requirements

* **Durable log** per topic
* **Multiple consumer groups**
* **At-least-once delivery**
* Start consuming from:

  * **LATEST** (tail)
  * **EARLIEST** (from beginning)
  * **AT offset/seq** (exact)
  * **AFTER offset/seq** (next)

Future-ready:

* **Topic partitions** (scale throughput)
* Each partition has its own **offset stream**

### MVP decision (now)

* Implement **1 partition only** at first (`partition_id = 0`).
* Still keep **partition_id in keys / APIs / system tables** so adding more partitions later is additive (no rewrite).

---

## 3) Storage layout in RocksDB

Use separate Column Families (CF) or key prefixes (either works; CF is cleaner).

### 3.1 `topic_logs` (append-only)

**Key (partitioned-ready)**

```
topic/<topic_id>/<partition_id>/<offset>
```

**Value** (binary envelope recommended; JSON only for debugging)

#### Payload modes

To keep topics lightweight for agent triggers, the topic entry should support a payload mode per route:

* `payload = key` (**recommended default for agent triggers**) → store only a reference (primary key)
* `payload = full` (optional) → store the full row snapshot
* `payload = diff` (future) → store only changed columns

#### Envelope fields (common)

* topic_id
* partition_id
* offset
* message_id (optional; can be Snowflake for uniqueness)
* source_table
* op: INSERT | UPDATE | DELETE
* ts
* payload_mode: key|full|diff

#### When `payload = key` (recommended)

Store a **reference** to the changed row, not the full row:

* pk: { ... }  (e.g. user_id, thread_id, seq)
* optional: small debug preview (first N chars) but not required

Consumer behavior for `payload = key`:

* consume topic message → **read the row from the user table** by PK → process → write results → ACK

> Note: `offset` is **monotonic per (topic,partition)**. This is your Kafka-like index.

### 3.2 `topic_offsets` (durable consumer progress)

**Key**

```
offset/<topic_id>/<group_id>/<partition_id>
```

**Value**

```
last_acked_offset (u64)
```

Optional extras:

* last_delivered_offset
* updated_at
* consumer_id (only if you implement member management)

### 3.3 `topic_meta`

**Key**

```
meta/<topic_id>
```

**Value**

* partitions_count (default 1)
* retention policy
* partitioning rule (future)

### 3.4 `topic_routes`

**Key**

```
route/<topic_id>/<table_id>/<op>
```

**Value**

* filter_expr (optional)
* partition_key_expr (optional)

---

## 4) Partitioning plan (future-ready)

### 4.1 Partitions

A topic has N partitions:

* partition_id: `0..N-1`
* each partition is an **independent log** with its own offsets

### 4.2 How to choose partition

Partition function (simple + scalable):

* `partition_id = hash(partition_key) % partitions_count`

Partition key options (choose one per route/topic):

* `user_id` (keeps user ordering)
* `table primary key`
* `namespace_id`

### 4.3 Consumer offsets with partitions

Offsets are tracked per group **per partition**:

* group `ai-service` ack partition 0 independently from partition 1.

### 4.4 Ordering guarantee

* Ordered **within a partition**.
* No global ordering guarantee across partitions (same as Kafka).

---

## 5) SQL surface (simple + familiar)

### 5.1 Create topic

```
CREATE TOPIC app.new_messages;
```

Partitioned topic (future-ready, optional now):

```
CREATE TOPIC app.new_messages
PARTITIONS 8;
```

### 5.2 Route table changes into a topic

Default route (recommended): store only a key/reference in the topic log.

```
ALTER TOPIC app.new_messages
ADD SOURCE chat.messages
ON INSERT
WITH (payload = 'key');
```

Optional: store full row snapshot (bigger topic log, fewer reads on consume).

```
ALTER TOPIC app.new_messages
ADD SOURCE chat.messages
ON INSERT
WITH (payload = 'full');
```

With partitioning key (optional now, useful later):

```
ALTER TOPIC app.new_messages
ADD SOURCE chat.messages
ON INSERT
PARTITION BY (user_id)
WITH (payload = 'key');
```

### 5.3 Consume

Start positions:

* From latest (tail)

```
CONSUME FROM app.new_messages
GROUP 'ai-service'
FROM LATEST
LIMIT 100;
```

* From earliest

```
CONSUME FROM app.new_messages
GROUP 'ai-service'
FROM EARLIEST
LIMIT 100;
```

* From a specific offset

```
CONSUME FROM app.new_messages
GROUP 'ai-service'
FROM OFFSET 12345
LIMIT 100;
```

Partitioned consume (future; either explicit or automatic fan-out):

```
CONSUME FROM app.new_messages
GROUP 'ai-service'
PARTITION 3
FROM OFFSET 120
LIMIT 100;
```

### 5.4 Ack

Ack a single partition offset:

```
ACK app.new_messages
GROUP 'ai-service'
PARTITION 3
UPTO OFFSET 220;
```

Ack multiple partitions (nice-to-have):

```
ACK app.new_messages
GROUP 'ai-service'
UPTO (
  PARTITION 0 OFFSET 991,
  PARTITION 1 OFFSET 410
);
```

---

## 6) API surface (SDK-friendly)

### 6.1 Consume request

* topic
* group_id
* start:

  * Latest
  * Earliest
  * Offset(u64)
* limit
* optional partition selection

### 6.2 Consume response

* messages[] each with:

  * topic_id
  * partition_id (MVP always 0)
  * offset
  * message_id (optional)
  * payload

### 6.3 Ack request

* topic
* group_id
* partition_id (MVP always 0)
* upto_offset

### 6.4 Where the consumer code should live (server vs SDK / WASM)

**Truth check:** WASM (especially in browsers) cannot safely/typically access RocksDB files directly.

So the clean split is:

* **Server (KalamDB engine)**: owns RocksDB, appends to `topic_logs`, reads logs, manages offsets.
* **Client SDK / link library (Rust)**: provides a nice `consume()` / `ack()` API that calls KalamDB over RPC/HTTP.
* Compile the same SDK to **WASM** so you can use it from JS/TS (browser or node) *as a network client*.

This gives you “use it everywhere” without trying to embed RocksDB in WASM.

---

## 7) Write path integration (where to hook)

Wherever KalamDB already produces a **ChangeEvent** for subscriptions:

* Call `TopicRouter::on_change(event)`

TopicRouter:

1. Loads matching routes for (table_id, op)
2. Optional filter check
3. Compute partition_id (default 0)
4. Allocate next offset
5. Append to `topic_logs`

---

## 8) Offset allocation (single-node now, cluster later)

### 8.1 Single-node

Maintain `next_offset` per (topic,partition):

* Store a counter key:

  * `counter/<topic>/<partition> -> last_offset`
* Use atomic update:

  * read last_offset
  * new_offset = last_offset + 1
  * write log entry at new_offset
  * update counter

### 8.2 Cluster (future)

Two safe options:

**Option A (recommended): leader-per-partition**

* Each topic partition has a leader node
* All appends for that partition go through its leader
* Replicate via Raft (or your existing multi-raft groups)

**Option B: allocate offsets via Raft/consensus service**

* More centralized; less ideal.

---

## 9) Retention & cleanup

Without retention, logs grow forever.

Per topic:

* retention_seconds (time based)
* retention_max_bytes (size based)

Cleanup job:

* delete keys by prefix range:

  * `topic/<topic>/<partition>/<offset>` where offset maps to timestamp if you store ts
* simplest: store ts inside value + also maintain coarse index per day if needed later.

---

## 10) The big question: Snowflake SeqId vs Kafka-like offsets?

### Recommendation: **use Kafka-like offsets (u64) as the primary cursor**, not Snowflake.

Why?

* Consumers need an index that is:

  * **monotonic**
  * **dense enough** for scanning
  * **per-partition ordered**
* Kafka offsets are perfect for this: `(partition_id, offset)`.

Snowflake IDs are great for:

* **global uniqueness**
* approximate time ordering
  But they are NOT ideal as the only cursor because:
* they’re not guaranteed strictly sequential per topic-partition
* they can introduce gaps and more complex “seek” semantics

### Best of both worlds (suggested)

Store **both**:

* `offset` (u64) → consumer cursor + iteration
* `message_id` (Snowflake) → global unique ID for dedupe/tracing/correlation

So the message identity is:

* ordering/cursor: `(topic, partition, offset)`
* global id: `message_id`

---

## 11) Minimal MVP checklist

MVP (no partitions yet):

* topic_logs keys: `topic/<topic>/0/<offset>`
* topic_offsets keys: `offset/<topic>/<group>/0`
* CREATE TOPIC
* ALTER TOPIC ADD SOURCE ...
* CONSUME ... FROM LATEST|EARLIEST|OFFSET
* ACK
* retention_seconds + cleanup job

Then scale-up:

* partitions
* partition key
* per-partition leader (cluster)

---

## 12) Example end-to-end

1. Create topic:

```
CREATE TOPIC app.new_messages PARTITIONS 4;
```

2. Route inserts:

```
ALTER TOPIC app.new_messages
ADD SOURCE chat.messages
ON INSERT
PARTITION BY (user_id);
```

3. Agent consumes from latest:

```
CONSUME FROM app.new_messages
GROUP 'ai-service'
FROM LATEST
LIMIT 100;
```

4. Agent acks:

```
ACK app.new_messages
GROUP 'ai-service'
PARTITION 2
UPTO OFFSET 551;
```
