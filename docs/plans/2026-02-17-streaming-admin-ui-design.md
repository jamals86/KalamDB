# Streaming Admin UI Design (Redpanda-Inspired)

Date: 2026-02-17  
Status: Approved (brainstorming validation complete)  
Scope: Add a new top-level `Streaming` module in existing Admin UI (`ui/`)

## 1. Validated Decisions

- Add a dedicated top-level route/module: `Streaming` (not inside SQL Studio tabs).
- MVP is read-only and observe-first:
  - No offset reset/rewind/replay actions in UI.
  - No topic mutation actions in UI.
- Backend access strategy is hybrid:
  - Metadata and offset diagnostics via SQL/system tables.
  - Message browsing via topics HTTP consume endpoint.
- Do not add role-specific behavior to this module; treat it as a pure UI surface.

## 2. Goals and Non-Goals

### Goals

- Make Kafka/Redpanda-style operators feel at home when inspecting topics, consumer groups, and offsets.
- Provide fast topic/message debugging workflows with minimal clicks.
- Preserve SQL Studio as analyst workspace while establishing `Streaming` as operator/debug workspace.
- Keep v1 safe and deterministic with explicit non-mutating behavior.

### Non-Goals (MVP)

- Offset mutation controls (reset, rewind, replay).
- Topic lifecycle mutation controls (`CREATE/ALTER/DROP/CLEAR`) inside Streaming pages.
- Backend authorization/role UX in this module.

## 3. Information Architecture

New top-level routes:

- `/streaming/topics`
- `/streaming/topics/:topicId`
- `/streaming/groups`
- `/streaming/offsets`

Positioning:

- SQL Studio remains query-authoring and table analysis.
- Streaming is specialized operational inspection/debugging.
- Each Streaming page includes "Open in SQL Studio" snippet actions, but never auto-executes SQL.

## 4. UX Surface (Redpanda-Inspired)

### Topics

- Searchable/sortable topic inventory.
- Columns: topic id, partitions, source route count, last activity indicator.
- Row actions: open details, copy identifiers, open SQL snippet.

### Topic Detail + Message Inspector

- Topic summary header (topic metadata + source bindings).
- Message browser controls:
  - Partition selector
  - Start selector (`Latest`, `Earliest`, explicit `Offset`)
  - Limit selector
  - Refresh/poll trigger
- Message table and side inspector:
  - `offset`, `timestamp`, `partition`, `op`, `key`, `username`, `payload`
  - Decode modes: `auto-json`, `text`, `base64`, raw fallback preview
  - Copy-first workflows for fast debugging handoff

### Consumer Groups

- Group-oriented list with topic/partition coverage and lag status indicators.
- Drill-through to related offsets/topic detail context.

### Offsets

- Partition-level offset diagnostics view:
  - committed offset
  - latest observed offset/high-water context
  - estimated lag status (read-only)

## 5. Frontend Architecture

Feature-first addition under existing UI:

- `ui/src/features/streaming/`
  - route containers/pages
  - API adapters/services
  - local UI slice/selectors
  - decode helpers
  - tests

Recommended page components:

- `StreamingLayout`
- `TopicsPage`
- `TopicDetailPage`
- `ConsumerGroupsPage`
- `OffsetsPage`
- `MessageInspectorPanel`

State strategy:

- RTK Query for server data fetch/caching.
- Thin local UI state for filters/selections/view preferences.
- Keep SQL Studio state and Streaming state separate.

## 6. Data Flow

### Metadata/Diagnostics (SQL path)

- Query system tables for topic/group/offset snapshots (`system.topics`, `system.topic_offsets`).
- Normalize SQL rows into view models for tables/cards.

### Message Inspection (HTTP path)

- Use `POST /v1/api/topics/consume` with selected `topic`, `partition`, `start`, and `limit`.
- Render consume lifecycle explicitly:
  - `idle`, `loading`, `loaded`, `no_messages`, `failed`.
- Treat no-message window as neutral operational state, not error.

## 7. Error Handling and Guardrails

Panel-scoped failure handling:

- `RequestFailed`: endpoint or network failure.
- `NoDataWindow`: consume returned no messages in poll window.
- `ParseError`: selected decode mode failed; show raw fallback.
- `StateMismatch`: stale/inconsistent metadata graph.

Guardrails:

- No side-effecting actions from Streaming in v1.
- No hidden auto-write behavior.
- SQL snippets can be generated and opened in SQL Studio, but not auto-run.

## 8. Testing Strategy

### Unit

- Payload decode utilities.
- View-model normalization for topics/groups/offsets.
- Consume request parameter composition.

### Integration (service layer)

- SQL metadata retrieval mapping.
- Consume endpoint success/no-message/error behavior.
- SQL snippet generation consistency.

### Component

- Topics filtering/sorting.
- Topic detail lifecycle transitions.
- Offsets/groups rendering with partial or stale data.

### E2E Smoke

- Navigate topics -> detail -> consume from `Earliest`.
- Switch decode modes and inspect/copy message fields.
- Jump to SQL Studio via generated snippet action.

## 9. Delivery Phases

1. Route + nav + Streaming shell + Topics list.
2. Topic detail + message inspector + decode modes.
3. Consumer groups + offsets diagnostics pages.
4. UX polish (copy actions, empty/loading/error ergonomics, saved filters).

## 10. v1 Success Criteria

- Operator can answer these within ~30 seconds:
  - "Are producers writing to this topic?"
  - "Are consumer groups progressing offsets?"
  - "What does a recent message look like at a specific partition/offset?"

## 11. Reference UX Inspirations

- Redpanda Console navigation and streaming diagnostics workflows:
  - https://github.com/redpanda-data/console
  - https://docs.redpanda.com/current/console/ui/
  - https://docs.redpanda.com/current/console/ui/edit-topic-configuration/
  - https://docs.redpanda.com/current/console/ui/rewind-and-replay/
