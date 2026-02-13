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
 *   6. Streaming AI replies with live typing status
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
 *   npm run service
 *
 * Required env vars:
 *   KALAMDB_USERNAME, KALAMDB_PASSWORD, GEMINI_API_KEY
 */
import 'dotenv/config';

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
  type ConsumeContext,
  type ConsumerHandle,
  type Username,
} from 'kalam-link';
import {
  generateConversationTitle,
  generateAIResponse,
  generateAIResponseStream,
  type ConversationTurn,
} from './ai-agent';
import { loadServiceConfig } from './service-config';

// ─── Configuration ──────────────────────────────────────────────────────────
const SERVICE_CONFIG = loadServiceConfig();
const KALAMDB_URL = SERVICE_CONFIG.kalamdbUrl;
const SERVICE_USERNAME = SERVICE_CONFIG.kalamdbUsername;
const SERVICE_PASSWORD = SERVICE_CONFIG.kalamdbPassword;
const TOPIC_NAME = SERVICE_CONFIG.topicName;
const CONSUMER_GROUP = SERVICE_CONFIG.consumerGroup;
const BATCH_SIZE = SERVICE_CONFIG.batchSize;

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
 * Keep `window.location` aligned with the configured URL because the WASM
 * runtime expects browser-like globals when running under Node.js.
 */
function configureWasmWindow(kalamdbUrl: string): void {
  const parsedUrl = new URL(kalamdbUrl);
  (globalThis as any).window = {
    location: {
      protocol: parsedUrl.protocol,
      hostname: parsedUrl.hostname,
      port: parsedUrl.port,
      href: parsedUrl.href,
    },
    fetch: globalThis.fetch,
  };
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

async function setAiTyping(client: KalamDBClient, conversationId: string, state: 'thinking' | 'typing' | 'finished', ownerUser: Username) {
  await setAiTypingWithState(client, conversationId, state, ownerUser);
}

async function setAiTypingWithState(
  client: KalamDBClient,
  conversationId: string,
  state: 'thinking' | 'typing' | 'finished',
  ownerUser: Username,
  tokenCount?: number,
) {
  const isTyping = state !== 'finished';
  const stateValue = state === 'typing' && typeof tokenCount === 'number'
    ? `typing:${tokenCount}`
    : state;
  const insertSql = `INSERT INTO chat.typing_indicators (conversation_id, user_name, is_typing, state) VALUES (${conversationId}, 'AI Assistant', ${isTyping}, '${stateValue}')`;
  console.log(`   [DEBUG] Inserting typing indicator (${state}) for conversation ${conversationId} as '${ownerUser}'`);
  console.log(`   [SQL] ${insertSql}`);
  try {
    const resp = await client.executeAsUser(insertSql, ownerUser);
    console.log(`   ✓ setAiTyping INSERT succeeded (state='${state}') - response:`, resp.results?.[0]);
  } catch (err) {
    console.error(`   ❌ setAiTyping INSERT failed (state='${state}'):`, err);
    console.error(`   [SQL] ${insertSql}`);
  }
}

async function clearAiTyping(client: KalamDBClient, conversationId: string, ownerUser: Username) {
  console.log(`   [DEBUG] Clearing typing indicator for conversation ${conversationId} as '${ownerUser}'`);
  await setAiTypingWithState(client, conversationId, 'finished', ownerUser);
}

function createServiceClientId(): string {
  return `ai-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

async function insertAssistantDraft(
  client: KalamDBClient,
  conversationId: string,
  ownerUser: Username,
  clientId: string,
): Promise<void> {
  const insertSql = `INSERT INTO chat.messages (client_id, conversation_id, sender, role, content, status)
     VALUES ('${sqlEscape(clientId)}', ${conversationId}, 'AI Assistant', 'assistant', '', 'sent')`;
  await client.executeAsUser(insertSql, ownerUser);
}

async function updateAssistantDraft(
  client: KalamDBClient,
  conversationId: string,
  ownerUser: Username,
  clientId: string,
  content: string,
): Promise<void> {
  const updateSql = `UPDATE chat.messages
     SET content = '${sqlEscape(content)}'
     WHERE conversation_id = ${conversationId}
       AND role = 'assistant'
       AND client_id = '${sqlEscape(clientId)}'`;
  await client.executeAsUser(updateSql, ownerUser);
}

async function fetchConversationHistory(
  client: KalamDBClient,
  conversationId: string,
  ownerUser: Username,
): Promise<ConversationTurn[]> {
  const historySql = `SELECT role, content
     FROM chat.messages
     WHERE conversation_id = ${conversationId}
     ORDER BY created_at DESC
     LIMIT ${SERVICE_CONFIG.aiContextWindowMessages}`;

  const historyResp = await client.executeAsUser(historySql, ownerUser);
  const historyRows = parseRows<Record<string, unknown>>(historyResp);

  return historyRows
    .map((row) => ({
      role: String(row.role),
      content: String(row.content ?? ''),
    }))
    .filter(
      (row): row is ConversationTurn =>
        (row.role === 'user' || row.role === 'assistant' || row.role === 'system') &&
        row.content.trim().length > 0,
    )
    .reverse();
}

async function updateConversationTitle(
  client: KalamDBClient,
  conversationId: string,
  ownerUser: Username,
  title: string,
): Promise<void> {
  const updateSql = `UPDATE chat.conversations
     SET title = '${sqlEscape(title)}', updated_at = NOW()
     WHERE id = ${conversationId}`;
  await client.executeAsUser(updateSql, ownerUser);
}

// ─── Message handler ────────────────────────────────────────────────────────

/**
 * Process a single consumed message.
 *
 * Steps:
 *  1. Decode the topic payload into a typed `TopicMessage`
 *  2. Skip non-user messages (assistant/system)
 *  3. Generate an AI reply with streaming updates
 *  4. Update conversation title
 */
async function processMessage(
  client: KalamDBClient,
  ctx: ConsumeContext,
): Promise<void> {
  const msg = ctx.message;
  // The username of who triggered this event (from ConsumeContext)
  const eventOwner = ctx.username;
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
    return;
  }

  const preview = data.content.length > 60 ? data.content.slice(0, 60) + '…' : data.content;

  // ── 2. Filter: only process user messages ───────────────────────────
  if (data.role !== 'user') {
    return;
  }
  console.log(`📩 [offset=${msg.offset}] Processing user message ${data.id}: "${preview}"`);

  // ── 3. Resolve conversation owner ───────────────────────────────
  // Prefer the username from ConsumeContext (set by the backend),
  // fall back to querying the conversation's created_by field.
  let conversationOwner: Username;
  if (eventOwner) {
    conversationOwner = eventOwner;
    console.log(`   👤 Event owner (from context): ${conversationOwner}`);
  } else {
    const convResp = await client.query(
      `SELECT created_by FROM chat.conversations WHERE id = ${data.conversation_id}`
    );
    const convRows = parseRows<Record<string, unknown>>(convResp);
    if (convRows.length === 0) {
      console.log(`   ⊘ Conversation ${data.conversation_id} not found`);
      return;
    }
    conversationOwner = String(convRows[0].created_by) as Username;
    console.log(`   👤 Conversation owner (from query): ${conversationOwner}`);
  }

  // ── 4. Generate AI reply ────────────────────────────────────────────
  await setAiTyping(client, data.conversation_id, 'thinking', conversationOwner);
  await sleep(350);
  await setAiTyping(client, data.conversation_id, 'typing', conversationOwner);
  let aiReply: string;
  const assistantClientId = createServiceClientId();
  let lastPersistedContent = '';
  let lastPersistedAt = 0;
  let lastTypingAt = 0;

  const persistDraft = async (content: string, force: boolean = false): Promise<void> => {
    if (!content && !force) {
      return;
    }
    if (!force && content === lastPersistedContent) {
      return;
    }
    const now = Date.now();
    if (!force && now - lastPersistedAt < 140) {
      return;
    }
    await updateAssistantDraft(
      client,
      data.conversation_id,
      conversationOwner,
      assistantClientId,
      content,
    );
    lastPersistedContent = content;
    lastPersistedAt = now;
  };

  const publishTypingProgress = async (tokenCount: number, force: boolean = false): Promise<void> => {
    const now = Date.now();
    if (!force && now - lastTypingAt < 400) {
      return;
    }
    await setAiTypingWithState(
      client,
      data.conversation_id,
      'typing',
      conversationOwner,
      tokenCount,
    );
    lastTypingAt = now;
  };

  await insertAssistantDraft(
    client,
    data.conversation_id,
    conversationOwner,
    assistantClientId,
  );

  try {
    const history = await fetchConversationHistory(
      client,
      data.conversation_id,
      conversationOwner,
    );
    const streamedResponse = await generateAIResponseStream({
      userMessage: data.content,
      history,
      config: SERVICE_CONFIG,
      onTextDelta: async (_delta, aggregateText) => {
        await persistDraft(aggregateText);
      },
      onTokenProgress: async (tokenCount) => {
        await publishTypingProgress(tokenCount);
      },
    });
    aiReply = streamedResponse.text;
    await persistDraft(aiReply, true);
    await publishTypingProgress(streamedResponse.tokenCount, true);
  } catch (error) {
    console.error(`   ❌ Gemini streaming failed for message ${data.id}:`, error);
    try {
      aiReply = await generateAIResponse({
        userMessage: data.content,
        history: await fetchConversationHistory(client, data.conversation_id, conversationOwner),
        config: SERVICE_CONFIG,
      });
    } catch {
      aiReply = 'I ran into a temporary AI service issue. Please try sending that again.';
    }
    await persistDraft(aiReply, true);
  }

  try {
    const latestHistory = await fetchConversationHistory(
      client,
      data.conversation_id,
      conversationOwner,
    );
    const generatedTitle = await generateConversationTitle({
      userMessage: data.content,
      assistantReply: aiReply,
      history: latestHistory,
      config: SERVICE_CONFIG,
    });
    if (generatedTitle.trim().length > 0) {
      await updateConversationTitle(
        client,
        data.conversation_id,
        conversationOwner,
        generatedTitle,
      );
      console.log(`   ✓ Updated conversation title: "${generatedTitle}"`);
    }
  } catch (titleError) {
    console.warn(`   ⚠️ Title generation/update skipped for conversation ${data.conversation_id}:`, titleError);
  }

  // ── 5. Finalize typing status ─────
  await clearAiTyping(client, data.conversation_id, conversationOwner);
  console.log(`   ✓ AI response fully processed for message ${data.id}`);
}

// ─── Main ───────────────────────────────────────────────────────────────────

async function main(): Promise<void> {
  console.log('🤖 AI Message Processor Service');
  console.log('================================');
  console.log(`KalamDB URL:      ${KALAMDB_URL}`);
  console.log(`Gemini model:     ${SERVICE_CONFIG.geminiModel}`);
  console.log(`Topic:            ${TOPIC_NAME}`);
  console.log(`Consumer Group:   ${CONSUMER_GROUP}`);
  console.log(`Batch Size:       ${BATCH_SIZE}`);
  console.log('');

  // ── Load WASM binary from disk ────────────────────────────────────────
  const wasmPath = resolveWasmPath();
  const wasmBytes = fs.readFileSync(wasmPath);
  configureWasmWindow(KALAMDB_URL);
  console.log(`📦 WASM loaded: ${wasmPath} (${(wasmBytes.length / 1024).toFixed(0)} KB)`);

  // ── Create kalam-link client (WASM-based) ─────────────────────────────
  const client = createClient({
    url: KALAMDB_URL,
    auth: Auth.basic(SERVICE_USERNAME, SERVICE_PASSWORD),
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
    batch_size: 1,
    auto_ack: false,
    start: 'latest',
  });

  // Wire up graceful shutdown
  const shutdown = () => {
    console.log('\n👋 Shutting down...');
    consumer.stop();
  };
  process.on('SIGINT',  shutdown);
  process.on('SIGTERM', shutdown);

  // Run consumes messages in a loop until consumer.stop() is called
  await consumer.run(async (ctx) => {
    await ctx.ack();
    await processMessage(client, ctx);
  });

  console.log('Service stopped.');
}

// ── Entry point ─────────────────────────────────────────────────────────────
main().catch((err) => {
  console.error('Fatal error:', err);
  process.exit(1);
});
