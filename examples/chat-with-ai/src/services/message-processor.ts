#!/usr/bin/env tsx
/**
 * AI Message Processor — Node.js background service using kalam-link WASM
 *
 * Demonstrates how to build a production-style message processing service
 * that connects to KalamDB via the kalam-link SDK (WASM-based, not HTTP).
 *
 * Key concepts covered:
 *   1. Loading WASM in Node.js (fs.readFileSync → wasmUrl buffer)
 *   2. Polyfilling browser APIs (WebSocket via `ws` package)
 *   3. Type-safe topic consumption with auto-ack
 *   4. SQL queries through the WASM client (query, insert, queryAll)
 *   5. Graceful shutdown with consumer.stop()
 *   6. Idempotent message handling (duplicate detection)
 *
 * Architecture:
 *   ┌──────────────┐     CDC INSERT      ┌─────────────────────┐
 *   │ chat.messages │  ───────────────▶   │ chat.ai-processing  │
 *   │   (table)     │                     │      (topic)        │
 *   └──────────────┘                      └────────┬────────────┘
 *                                                  │ consume
 *                                         ┌────────▼────────────┐
 *                                         │  message-processor  │
 *                                         │   (this service)    │
 *                                         └────────┬────────────┘
 *                                                  │ INSERT AI reply
 *                                         ┌────────▼────────────┐
 *                                         │ chat.messages       │
 *                                         └─────────────────────┘
 *
 * Usage:
 *   npm run service                     # uses defaults (localhost:8080)
 *   KALAMDB_URL=http://host:port npm run service
 */

// ─── Node.js polyfills for WASM browser APIs ─────────────────────────────────
// The kalam-link WASM binary is compiled for the web platform and expects
// browser globals (WebSocket, window.fetch). We polyfill them here so the
// same binary works seamlessly in Node.js.
import { WebSocket } from 'ws';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

// WASM glue code uses `new WebSocket(url)` — provide Node.js implementation
(globalThis as any).WebSocket = WebSocket;

// WASM glue code references `window.fetch` for HTTP calls
(globalThis as any).window = {
  location: { protocol: 'http:', hostname: 'localhost', port: '8080', href: 'http://localhost:8080' },
  fetch: globalThis.fetch,
};

// ─── SDK imports (after polyfills) ───────────────────────────────────────────
import {
  createClient,
  Auth,
  parseRows,
  type KalamDBClient,
  type ConsumeMessage,
  type ConsumeContext,
  type ConsumerHandle,
  type QueryResponse,
} from 'kalam-link';

// ─── Configuration ──────────────────────────────────────────────────────────
const KALAMDB_URL     = process.env.KALAMDB_URL      || 'http://localhost:8080';
const ADMIN_USERNAME  = process.env.KALAMDB_USERNAME  || 'admin';
const ADMIN_PASSWORD  = process.env.KALAMDB_PASSWORD  || 'kalamdb123';
const TOPIC_NAME      = 'chat.ai-processing';
const CONSUMER_GROUP  = 'ai-processor-service';
const BATCH_SIZE      = 10;

// ─── Type-safe message interfaces ───────────────────────────────────────────

/** Shape of a message consumed from the `chat.ai-processing` topic. */
interface TopicMessage {
  id: string;
  client_id?: string;
  conversation_id: string;
  sender: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  status: string;
  created_at: string;
}

/** Row returned by `SELECT COUNT(*) as cnt ...` */
interface CountRow extends Record<string, unknown> {
  cnt: number;
}

/** Row returned from chat.messages table */
interface ChatMessage {
  id: string;
  conversation_id: string;
  sender: string;
  role: string;
  content: string;
  status: string;
  created_at: string;
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/**
 * Resolve the absolute path to the kalam-link WASM binary.
 * Works whether invoked from project root or from the services/ directory.
 */
function resolveWasmPath(): string {
  // __dirname equivalent for ESM / tsx
  const thisDir = typeof __dirname !== 'undefined'
    ? __dirname
    : path.dirname(fileURLToPath(import.meta.url));

  // Relative from examples/chat-with-ai/src/services/ → link/sdks/typescript/dist/wasm/
  const wasmPath = path.resolve(thisDir, '../../../../link/sdks/typescript/dist/wasm/kalam_link_bg.wasm');
  if (!fs.existsSync(wasmPath)) {
    console.error(`WASM file not found at: ${wasmPath}`);
    console.error('Build it first:  cd link/sdks/typescript && bash build.sh');
    process.exit(1);
  }
  return wasmPath;
}

/**
 * Simulate an AI response. Replace this with a real LLM call (OpenAI, etc.)
 * when building a production service.
 */
async function generateAIResponse(userMessage: string): Promise<string> {
  await new Promise(r => setTimeout(r, 600 + Math.random() * 400));

  const replies = [
    "That's a thoughtful question. Let me walk you through a fuller answer so it is clear and actionable. First, identify the goal and the constraints. Next, break the problem into smaller steps and validate assumptions early. Finally, iterate with a quick test and refine based on what you observe.",
    "I understand. Here is a longer explanation with context and a concrete path forward. Start by outlining the core requirement, then map the data flow and where the bottlenecks are. After that, pick the smallest change that delivers the biggest improvement.",
    "Great point. If we expand on it: consider tradeoffs, failure modes, and how you will verify the outcome. A simple checklist helps: input validation, timing, data consistency, and observability. Then you can scale the solution with confidence.",
    "Thanks for sharing your thoughts. I would approach it in three phases: analyze the current behavior, introduce a small change with guardrails, and measure impact. That keeps the system stable while still moving quickly.",
    "That makes sense. Let me elaborate a bit: identify the source of truth, make sure the metadata travels with the data, and ensure each step is idempotent. This reduces bugs and prevents duplicate processing.",
    "I see where you're coming from. Another perspective is to prioritize clarity and traceability: log key events, use explicit identifiers, and keep the UI in sync with the backend state.",
    "Excellent observation. This reminds me of a pattern that works well: optimistic UI updates with a server-side reconciliation step that merges by a stable client id.",
    "I appreciate your question. The answer lies in balancing correctness and latency. You can start with a straightforward implementation, then tighten the loop with progress feedback and better state merging.",
  ];

  const reply = replies[Math.floor(Math.random() * replies.length)];
  const preview = userMessage.length > 50 ? userMessage.slice(0, 50) + '…' : userMessage;
  return `${reply} (responding to: "${preview}")`;
}

/**
 * Escape a string for safe inclusion in a SQL single-quoted literal.
 */
function sqlEscape(s: string): string {
  return s.replace(/'/g, "''");
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function setAiTyping(client: KalamDBClient, conversationId: string, state: 'thinking' | 'typing' | 'finished') {
  const isTyping = state !== 'finished';
  await client.query(
    `DELETE FROM chat.typing_indicators WHERE conversation_id = ${conversationId} AND user_name = 'AI Assistant'`
  ).catch(() => {});
  const sql = `INSERT INTO chat.typing_indicators (conversation_id, user_name, is_typing, state) VALUES (${conversationId}, 'AI Assistant', ${isTyping}, '${state}')`;
  await client.query(sql).catch(() => {});
}

async function clearAiTyping(client: KalamDBClient, conversationId: string) {
  const sql = `UPDATE chat.typing_indicators SET is_typing = false, state = 'finished', updated_at = NOW() WHERE conversation_id = ${conversationId} AND user_name = 'AI Assistant'`;
  await client.query(sql).catch(() => {});
}

// ─── Message handler ────────────────────────────────────────────────────────

/**
 * Process a single consumed message.
 *
 * Steps:
 *  1. Decode the topic payload into a typed `TopicMessage`
 *  2. Skip non-user messages (assistant/system)
 *  3. Check for duplicate AI responses (idempotency)
 *  4. Generate an AI reply
 *  5. Insert the reply into `chat.messages`
 */
async function processMessage(
  client: KalamDBClient,
  msg: ConsumeMessage,
  _ctx: ConsumeContext,
): Promise<void> {
  // ── 1. Parse payload ────────────────────────────────────────────────
  // The topic payload may be a CDC event with row data nested inside,
  // or a direct row object depending on topic configuration.
  const raw = msg.value as unknown as Record<string, any>;

  // CDC payloads wrap the row data — extract accordingly:
  //   { row: { id, conversation_id, ... }, op: "insert", ... }
  // Direct payloads are the row itself:
  //   { id, conversation_id, sender, role, content, ... }
  const data = (raw.row ?? raw) as TopicMessage;

  if (!data.content) {
    console.log(`   ⊘ [offset=${msg.offset}] Skipping message with no content`);
    return;
  }

  const preview = data.content.length > 60 ? data.content.slice(0, 60) + '…' : data.content;
  console.log(`📩 [offset=${msg.offset}] ${data.role} message ${data.id}: "${preview}"`);

  // ── 2. Filter: only process user messages ───────────────────────────
  if (data.role !== 'user') {
    console.log(`   ⊘ Skipping ${data.role} message`);
    return;
  }

  // ── 3. Idempotency: skip if AI already replied ──────────────────────
  if (data.client_id) {
    const minResp = await client.query(
      `SELECT MIN(id) as min_id FROM chat.messages
       WHERE conversation_id = ${data.conversation_id}
         AND role = 'user'
         AND client_id = '${sqlEscape(data.client_id)}'`
    );
    const minRows = parseRows<Record<string, unknown>>(minResp);
    const minId = minRows[0]?.min_id ? Number(minRows[0].min_id) : Number(data.id);
    if (Number(data.id) !== minId) {
      console.log(`   ⊘ Skipping duplicate file row for client_id ${data.client_id}`);
      return;
    }
  }

  const countResp = await client.query(
    `SELECT COUNT(*) as cnt FROM chat.messages
     WHERE conversation_id = ${data.conversation_id}
       AND role = 'assistant'
       AND id > ${data.id}`
  );
  const rows = parseRows<CountRow>(countResp);
  if (rows.length > 0 && Number(rows[0].cnt) > 0) {
    console.log(`   ⊘ AI response already exists for message ${data.id}`);
    return;
  }

  // ── 4. Generate AI reply ────────────────────────────────────────────
  await setAiTyping(client, data.conversation_id, 'thinking');
  await sleep(600 + Math.random() * 700);
  await setAiTyping(client, data.conversation_id, 'typing');
  const aiReply = await generateAIResponse(data.content);
  await sleep(Math.min(3500, 1000 + aiReply.length * 8));

  // ── 5. Insert reply ─────────────────────────────────────────────────
  await client.query(
    `INSERT INTO chat.messages (conversation_id, sender, role, content, status)
     VALUES (${data.conversation_id}, 'AI Assistant', 'assistant', '${sqlEscape(aiReply)}', 'sent')`
  );
  await clearAiTyping(client, data.conversation_id);
  console.log(`   ✓ AI response inserted for message ${data.id}`);
}

// ─── Main ───────────────────────────────────────────────────────────────────

async function main(): Promise<void> {
  console.log('🤖 AI Message Processor Service');
  console.log('================================');
  console.log(`KalamDB URL:      ${KALAMDB_URL}`);
  console.log(`Topic:            ${TOPIC_NAME}`);
  console.log(`Consumer Group:   ${CONSUMER_GROUP}`);
  console.log(`Batch Size:       ${BATCH_SIZE}`);
  console.log('');

  // ── Load WASM binary from disk ────────────────────────────────────────
  const wasmPath = resolveWasmPath();
  const wasmBytes = fs.readFileSync(wasmPath);
  console.log(`📦 WASM loaded: ${wasmPath} (${(wasmBytes.length / 1024).toFixed(0)} KB)`);

  // ── Create kalam-link client (WASM-based) ─────────────────────────────
  const client = createClient({
    url: KALAMDB_URL,
    auth: Auth.basic(ADMIN_USERNAME, ADMIN_PASSWORD),
    wasmUrl: wasmBytes,  // Pass buffer so WASM init skips fetch()
  });

  // ── Authenticate (Basic Auth → JWT) ───────────────────────────────────
  console.log('Authenticating...');
  const loginResp = await client.login();
  console.log(`✓ Authenticated as ${loginResp.user.username} (role: ${loginResp.user.role})`);

  // ── Quick health check: verify topic exists ───────────────────────────
  const topicCheck = await client.query(
    `SELECT * FROM system.topics WHERE name = '${TOPIC_NAME}'`
  );
  if (!topicCheck.results?.[0]?.row_count) {
    console.error(`✗ Topic "${TOPIC_NAME}" not found. Run the setup script first: bash setup.sh`);
    process.exit(1);
  }
  console.log(`✓ Topic "${TOPIC_NAME}" exists`);
  console.log('');
  console.log('🚀 Service started — waiting for messages...');
  console.log('   Press Ctrl+C to stop');
  console.log('');

  // ── Start consumer loop ───────────────────────────────────────────────
  const consumer: ConsumerHandle = client.consumer({
    topic: TOPIC_NAME,
    group_id: CONSUMER_GROUP,
    batch_size: BATCH_SIZE,
    auto_ack: true,
    start: 'earliest',
  });

  // Wire up graceful shutdown
  const shutdown = () => {
    console.log('\n👋 Shutting down...');
    consumer.stop();
  };
  process.on('SIGINT',  shutdown);
  process.on('SIGTERM', shutdown);

  // Run consumes messages in a loop until consumer.stop() is called
  await consumer.run(async (msg, ctx) => {
    await processMessage(client, msg, ctx);
  });

  console.log('Service stopped.');
}

// ── Entry point ─────────────────────────────────────────────────────────────
main().catch((err) => {
  console.error('Fatal error:', err);
  process.exit(1);
});