# summarizer-agent

Minimal, production-style KalamDB agent that listens to a topic and writes AI summaries back to the same table.

## What this example teaches

- How to consume topic events with SDK-level `runAgent()`
- How to read the changed row and enrich it (`summary`)
- How to handle retries and a final failure sink table
- How to keep agent code small while using local `kalam-link` SDK (`file:../../link/sdks/typescript`)

## Data flow

1. `blog.blogs` row is inserted or updated.
2. Topic route publishes an event to `blog.summarizer`.
3. Agent consumes the event.
4. Agent reads `blog.blogs` by `blog_id`.
5. Agent writes `summary` + `updated` back to `blog.blogs`.
6. If retries are exhausted, agent writes error details to `blog.summary_failures`.

## Project structure

- `src/agent.ts`: thin entrypoint (env, client, `runAgent()` wiring).
- `src/summarizer-runtime.ts`: core logic (Gemini adapter, row processing, failure sink handling).
- `setup.sql`: namespace/table/topic definitions + sample row.
- `setup.sh`: bootstrap script for schema + topic + env file generation.

## Prerequisites

- KalamDB server running on `http://localhost:8080`
- Node.js 18+
- Gemini API key (optional; fallback summarizer works without it)

## Quick start

1. Start KalamDB server:

```bash
cd backend
cargo run
```

2. Bootstrap schema/topic/sample row/env:

```bash
cd examples/summarizer-agent
./setup.sh
```

3. Install dependencies:

```bash
npm install
```

4. Add API key to `.env.local` (optional but recommended):

```dotenv
GEMINI_API_KEY=your_key_here
GEMINI_MODEL=gemini-2.5-flash
```

5. Start the agent:

```bash
npm run start
```

## Verify end-to-end

1. Update a blog row:

```sql
UPDATE blog.blogs
SET content = 'KalamDB topics let tiny agents react to data changes and enrich rows in place.'
WHERE blog_id = <sample_blog_id_from_setup_output>;
```

2. Verify summary was written:

```sql
SELECT blog_id, content, summary, created, updated
FROM blog.blogs
WHERE blog_id = <sample_blog_id_from_setup_output>;
```

## Runtime logs you will see

- `[event]`: consumed message metadata (topic/partition/offset/run_key)
- `[wake]`: row-level processing started
- `[process]`: summarization started
- `[summarized]`: summary written to `blog.blogs`
- `[skip]`: message intentionally ignored (invalid id, no change, empty content)
- `[retry]`: retry attempt after handler error
- `[failed-sink]`: failure persisted to `blog.summary_failures`

## Important implementation details

- Loop prevention: runtime tracks a fingerprint of processed content per `blog_id` to avoid summary-update loops.
- Stable AI output: Gemini temperature is `0`.
- Failure sink upsert pattern: uses `UPDATE` then `INSERT` fallback.
  - KalamDB currently does not support `INSERT ... ON CONFLICT ... DO UPDATE`.
  - This is why the example avoids `ON CONFLICT` SQL syntax.

## Environment variables

- `KALAMDB_URL` (default: `http://localhost:8080`)
- `KALAMDB_USERNAME` (default: `root`)
- `KALAMDB_PASSWORD` (default: `kalamdb123`)
- `KALAMDB_TOPIC` (default: `blog.summarizer`)
- `KALAMDB_GROUP` (default: `blog-summarizer-agent`)
- `KALAMDB_SYSTEM_PROMPT`
- `GEMINI_API_KEY` or `GOOGLE_GENERATIVE_AI_API_KEY`
- `GEMINI_MODEL` (default: `gemini-2.5-flash`)
