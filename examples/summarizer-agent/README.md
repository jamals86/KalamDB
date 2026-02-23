# summarizer-agent

Minimal KalamDB agent example that consumes topic events and writes summaries back to a table.

## What it does

1. Consumes `blog.summarizer` topic messages produced from `blog.blogs` insert/update events.
2. Reads the latest `blog.blogs` row.
3. Writes a generated summary into `summary`.

## Table schema

`blog.blogs`:
- `blog_id`
- `content`
- `summary`
- `created`
- `updated`

## Run

1. Start KalamDB server:
   - `cd backend && cargo run`
2. Setup table/topic/sample row:
   - `cd /Users/jamal/git/KalamDB/examples/summarizer-agent && ./setup.sh`
   - This also creates `.env.local` (same pattern as `examples/chat-with-ai`).
3. Install deps (SDK is local, not npm):
   - `npm install`
4. Start the agent:
   - `npm run start`

Dependency note:
- `kalam-link` is loaded from local path `file:../../link/sdks/typescript`.

## Trigger test

Run this SQL (using any client) to create an update event:

```sql
UPDATE blog.blogs
SET content = 'KalamDB topics let tiny agents react to data changes and enrich rows in place.'
WHERE blog_id = <sample_blog_id_from_setup_output>;
```

Then query:

```sql
SELECT blog_id, content, summary, created, updated
FROM blog.blogs
WHERE blog_id = <sample_blog_id_from_setup_output>;
```
