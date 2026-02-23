import { config as loadEnv } from 'dotenv';
import { ChatOpenAI } from '@langchain/openai';
import {
  Auth,
  createClient,
  createLangChainAdapter,
  runAgent,
  type AgentContext,
  type AgentLLMAdapter,
} from 'kalam-link';

loadEnv({ path: '.env.local', quiet: true });
loadEnv({ quiet: true });

type BlogRow = {
  blog_id: string;
  content: string;
  summary: string | null;
};

type BlogFailureRow = {
  blog_id: string;
  error: string;
};

function normalizeUrlForNode(url: string): string {
  const parsed = new URL(url);
  if (parsed.hostname === 'localhost') {
    parsed.hostname = '127.0.0.1';
  }
  return parsed.toString().replace(/\/$/, '');
}

const KALAMDB_URL = normalizeUrlForNode(process.env.KALAMDB_URL ?? 'http://127.0.0.1:8080');
const KALAMDB_USERNAME = process.env.KALAMDB_USERNAME ?? 'root';
const KALAMDB_PASSWORD = process.env.KALAMDB_PASSWORD ?? 'kalamdb123';
const TOPIC = process.env.KALAMDB_TOPIC ?? 'blog.summarizer';
const GROUP = process.env.KALAMDB_GROUP ?? 'blog-summarizer-agent';
const SYSTEM_PROMPT = process.env.KALAMDB_SYSTEM_PROMPT
  ?? 'Write one concise sentence summarizing the blog content. Preserve key facts and avoid hallucinations.';
const OPENAI_API_KEY = process.env.OPENAI_API_KEY?.trim();
const OPENAI_MODEL = process.env.OPENAI_MODEL?.trim() || 'gpt-4o-mini';

const client = createClient({
  url: KALAMDB_URL,
  auth: Auth.basic(KALAMDB_USERNAME, KALAMDB_PASSWORD),
});

function fallbackSummary(content: string): string {
  const compact = content.replace(/\s+/g, ' ').trim();
  if (!compact) {
    return '';
  }
  const words = compact.split(' ');
  const excerpt = words.slice(0, 24).join(' ');
  return words.length > 24 ? `${excerpt}...` : excerpt;
}

function toBlogId(value: unknown): string | null {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return String(value);
  }
  if (typeof value === 'string' && value.trim() !== '') {
    return value.trim();
  }
  return null;
}

function buildLlmAdapter(): AgentLLMAdapter | undefined {
  if (!OPENAI_API_KEY) {
    return undefined;
  }

  const model = new ChatOpenAI({
    apiKey: OPENAI_API_KEY,
    model: OPENAI_MODEL,
    temperature: 0.1,
  });

  return createLangChainAdapter(model);
}

async function summarizeContent(ctx: AgentContext<Record<string, unknown>>, content: string): Promise<string> {
  const compact = content.replace(/\s+/g, ' ').trim();
  if (!compact) {
    return '';
  }

  if (!ctx.llm) {
    return fallbackSummary(compact);
  }

  const prompt = `Summarize the following blog post in one short sentence:\n\n${compact}`;
  const generated = (await ctx.llm.complete(prompt)).trim();
  return generated || fallbackSummary(compact);
}

async function updateSummary(
  ctx: AgentContext<Record<string, unknown>>,
  blogId: string,
): Promise<void> {
  const row = await ctx.queryOne<BlogRow>(
    'SELECT blog_id, content, summary FROM blog.blogs WHERE blog_id = $1',
    [blogId],
  );

  if (!row || !row.content) {
    return;
  }

  const nextSummary = await summarizeContent(ctx, row.content);
  if (!nextSummary) {
    return;
  }

  const currentSummary = (row.summary ?? '').trim();
  if (currentSummary === nextSummary) {
    return;
  }

  await ctx.sql(
    'UPDATE blog.blogs SET summary = $1, updated = NOW() WHERE blog_id = $2',
    [nextSummary, blogId],
  );

  console.log(`[summarized] blog_id=${blogId}`);
}

async function main(): Promise<void> {
  const llmAdapter = buildLlmAdapter();
  const abortController = new AbortController();

  const stop = (): void => {
    abortController.abort();
  };

  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);

  console.log(`summarizer-agent starting (topic=${TOPIC}, group=${GROUP})`);
  if (llmAdapter) {
    console.log(`summarizer-agent using LangChain model: ${OPENAI_MODEL}`);
  } else {
    console.log('summarizer-agent using fallback summarizer (set OPENAI_API_KEY for LangChain)');
  }

  try {
    await runAgent<Record<string, unknown>>({
      client,
      name: 'summarizer-agent',
      topic: TOPIC,
      groupId: GROUP,
      start: 'earliest',
      batchSize: 20,
      timeoutSeconds: 30,
      systemPrompt: SYSTEM_PROMPT,
      llm: llmAdapter,
      stopSignal: abortController.signal,
      retry: {
        maxAttempts: 3,
        initialBackoffMs: 250,
        maxBackoffMs: 1500,
        multiplier: 2,
      },
      onRow: async (ctx, row): Promise<void> => {
        const blogId = toBlogId(row.blog_id);
        if (!blogId) {
          return;
        }
        await updateSummary(ctx, blogId);
      },
      onFailed: async (ctx): Promise<void> => {
        const failedBlogId = toBlogId((ctx.row as BlogFailureRow).blog_id) ?? 'unknown';
        const errorText = String(ctx.error instanceof Error ? ctx.error.message : ctx.error ?? 'unknown');

        await ctx.sql(
          `
          INSERT INTO blog.summary_failures (run_key, blog_id, error, created, updated)
          VALUES ($1, $2, $3, NOW(), NOW())
          ON CONFLICT (run_key)
          DO UPDATE SET error = EXCLUDED.error, updated = NOW()
          `,
          [ctx.runKey, failedBlogId, errorText.slice(0, 4000)],
        );
      },
      ackOnFailed: true,
      onRetry: ({ attempt, maxAttempts, runKey, error }) => {
        console.warn(
          `[retry] run_key=${runKey} attempt=${attempt}/${maxAttempts} error=${String(error)}`,
        );
      },
      onError: ({ runKey, error }) => {
        console.error(`[agent-error] run_key=${runKey} error=${String(error)}`);
      },
    });
  } finally {
    process.off('SIGINT', stop);
    process.off('SIGTERM', stop);
    await client.disconnect();
  }
}

main().catch((error) => {
  console.error('summarizer-agent failed:', error);
  process.exit(1);
});
