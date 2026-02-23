import { config as loadEnv } from 'dotenv';
import {
  Auth,
  createClient,
  type ConsumeContext,
  type ConsumeMessage,
} from 'kalam-link';

loadEnv({ path: '.env.local', quiet: true });
loadEnv({ quiet: true });

type BlogRow = {
  blog_id: string;
  content: string;
  summary: string | null;
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

const client = createClient({
  url: KALAMDB_URL,
  auth: Auth.basic(KALAMDB_USERNAME, KALAMDB_PASSWORD),
});

function summarize(content: string): string {
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

function readBlogId(message: ConsumeMessage): string | null {
  const payload = message.value as unknown;
  if (!payload || typeof payload !== 'object') {
    return null;
  }

  const envelope = payload as Record<string, unknown>;
  const row = (envelope.row && typeof envelope.row === 'object')
    ? (envelope.row as Record<string, unknown>)
    : envelope;

  return toBlogId(row.blog_id);
}

async function syncSummary(blogId: string): Promise<void> {
  const row = await client.queryOne<BlogRow>(
    'SELECT blog_id, content, summary FROM blog.blogs WHERE blog_id = $1',
    [blogId],
  );

  if (!row || !row.content) {
    return;
  }

  const nextSummary = summarize(row.content);
  if (!nextSummary) {
    return;
  }

  const currentSummary = (row.summary ?? '').trim();
  if (currentSummary === nextSummary) {
    return;
  }

  await client.query(
    'UPDATE blog.blogs SET summary = $1, updated = NOW() WHERE blog_id = $2',
    [nextSummary, blogId],
  );

  console.log(`[summarized] blog_id=${blogId}`);
}

async function main(): Promise<void> {
  console.log(`summarizer-agent starting (topic=${TOPIC}, group=${GROUP})`);

  const consumer = client.consumer({
    topic: TOPIC,
    group_id: GROUP,
    start: 'earliest',
    batch_size: 20,
    auto_ack: true,
  });

  const shutdown = async (): Promise<void> => {
    console.log('stopping summarizer-agent...');
    consumer.stop();
    await client.disconnect();
    process.exit(0);
  };

  process.on('SIGINT', () => {
    void shutdown();
  });
  process.on('SIGTERM', () => {
    void shutdown();
  });

  await consumer.run(async (ctx: ConsumeContext) => {
    const blogId = readBlogId(ctx.message);
    if (blogId === null) {
      return;
    }
    await syncSummary(blogId);
  });
}

main().catch((error) => {
  console.error('summarizer-agent failed:', error);
  process.exit(1);
});
