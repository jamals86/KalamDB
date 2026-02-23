# summarizer-agent

Minimal KalamDB agent example powered by SDK-level `runAgent()`.

## What it does

1. Consumes `blog.summarizer` topic events from `blog.blogs` insert/update routes.
2. Reads the changed blog row (`blog_id`, `content`, `summary`).
3. Writes an updated summary back into `blog.blogs.summary`.
4. On exhausted retries, writes a failure record into `blog.summary_failures` and only then acks.

## Runtime behavior

- Uses local SDK path: `kalam-link: file:../../link/sdks/typescript`
- Auto-builds SDK before `npm run start` via `scripts/ensure-sdk.sh`
- Uses LangChain (`@langchain/openai`) when `OPENAI_API_KEY` is set
- Falls back to deterministic local summarizer when no API key is provided

## Run

1. Start server:
   - `cd backend && cargo run`
2. Bootstrap schema/topic/sample row and env:
   - `cd examples/summarizer-agent && ./setup.sh`
3. Install deps:
   - `npm install`
4. Start agent:
   - `npm run start`

## Trigger test

```sql
UPDATE blog.blogs
SET content = 'KalamDB topics let tiny agents react to data changes and enrich rows in place.'
WHERE blog_id = <sample_blog_id_from_setup_output>;
```

Then verify:

```sql
SELECT blog_id, content, summary, created, updated
FROM blog.blogs
WHERE blog_id = <sample_blog_id_from_setup_output>;
```
