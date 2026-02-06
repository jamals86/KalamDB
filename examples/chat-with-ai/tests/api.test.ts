/**
 * API Integration Tests for Chat-with-AI
 * 
 * Tests that verify conversations and messages can be loaded and created
 * through the KalamDB server. Runs against a live server instance.
 * 
 * Prerequisites: 
 * - KalamDB server running on http://localhost:8080
 * - Database initialized via setup.sh
 * 
 * Run: npx tsx tests/api.test.ts
 */

const KALAMDB_URL = process.env.KALAMDB_SERVER_URL || 'http://localhost:8080';
const ADMIN_USER = 'admin';
const ADMIN_PASS = process.env.KALAMDB_ROOT_PASSWORD || 'kalamdb123';
const USERNAME = 'demo-user';
const PASSWORD = 'demo123';
const SERVICE_USER = 'ai-service';
const SERVICE_PASS = 'service123';

interface TestResult {
  name: string;
  passed: boolean;
  error?: string;
  duration: number;
}

const results: TestResult[] = [];

async function getToken(username: string, password: string): Promise<string> {
  const response = await fetch(`${KALAMDB_URL}/v1/api/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password }),
  });
  const data = await response.json();
  if (!data.access_token) {
    throw new Error(`Login failed for ${username}: ${JSON.stringify(data)}`);
  }
  return data.access_token;
}

async function executeSQL(token: string, sql: string): Promise<any> {
  const response = await fetch(`${KALAMDB_URL}/v1/api/sql`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${token}`,
    },
    body: JSON.stringify({ sql }),
  });
  return response.json();
}

function parseRows<T>(result: any): T[] {
  if (!result.results?.[0]?.rows || !result.results[0].schema) return [];
  const schema = result.results[0].schema;
  return result.results[0].rows.map((row: any[]) => {
    const obj: Record<string, any> = {};
    schema.forEach((field: any) => { obj[field.name] = row[field.index]; });
    return obj as T;
  });
}

async function test(name: string, fn: () => Promise<void>) {
  const start = Date.now();
  try {
    await fn();
    results.push({ name, passed: true, duration: Date.now() - start });
    console.log(`  ✅ ${name} (${Date.now() - start}ms)`);
  } catch (error) {
    const msg = error instanceof Error ? error.message : String(error);
    results.push({ name, passed: false, error: msg, duration: Date.now() - start });
    console.log(`  ❌ ${name}: ${msg} (${Date.now() - start}ms)`);
  }
}

function assert(condition: boolean, message: string) {
  if (!condition) throw new Error(message);
}

// ============================================================================
// Tests
// ============================================================================

async function runTests() {
  console.log('\n🧪 Chat-with-AI API Integration Tests\n');
  console.log(`Server: ${KALAMDB_URL}\n`);

  let adminToken: string;
  let userToken: string;
  let serviceToken: string;

  // --- Authentication ---

  await test('Login as admin', async () => {
    adminToken = await getToken(ADMIN_USER, ADMIN_PASS);
    assert(adminToken.length > 10, 'Token should be a valid JWT string');
  });

  await test('Login as demo-user', async () => {
    userToken = await getToken(USERNAME, PASSWORD);
    assert(userToken.length > 10, 'Token should be a valid JWT string');
  });

  await test('Login as ai-service', async () => {
    serviceToken = await getToken(SERVICE_USER, SERVICE_PASS);
    assert(serviceToken.length > 10, 'Token should be a valid JWT string');
  });

  // --- Conversations ---

  await test('Load conversations list', async () => {
    const result = await executeSQL(userToken!, 
      'SELECT * FROM chat.conversations ORDER BY updated_at DESC'
    );
    assert(result.status === 'success', `Expected success but got: ${JSON.stringify(result.error)}`);
    assert(Array.isArray(result.results), 'Results should be an array');
    const schema = result.results[0].schema;
    const fieldNames = schema.map((f: any) => f.name);
    assert(fieldNames.includes('id'), 'Schema should include id field');
    assert(fieldNames.includes('title'), 'Schema should include title field');
    assert(fieldNames.includes('created_by'), 'Schema should include created_by field');
  });

  let testConvId: string;

  await test('Create a new conversation', async () => {
    const title = `Test Conversation ${Date.now()}`;
    const result = await executeSQL(adminToken!,
      `INSERT INTO chat.conversations (title, created_by) VALUES ('${title}', 'demo-user')`
    );
    assert(result.status === 'success', `Insert failed: ${JSON.stringify(result.error)}`);

    // Fetch the created conversation as demo-user (read access)
    const fetchResult = await executeSQL(userToken!,
      `SELECT * FROM chat.conversations WHERE title = '${title}' ORDER BY created_at DESC LIMIT 1`
    );
    const convs = parseRows<{ id: string; title: string }>(fetchResult);
    assert(convs.length === 1, `Expected 1 conversation, got ${convs.length}`);
    assert(convs[0].title === title, `Title mismatch: ${convs[0].title}`);
    testConvId = convs[0].id;
  });

  // --- Messages ---

  await test('Load messages for conversation (empty)', async () => {
    const result = await executeSQL(userToken!,
      `SELECT * FROM chat.messages WHERE conversation_id = ${testConvId} ORDER BY created_at ASC`
    );
    assert(result.status === 'success', `Query failed: ${JSON.stringify(result.error)}`);
    const messages = parseRows<any>(result);
    assert(messages.length === 0, `Expected 0 messages, got ${messages.length}`);
  });

  await test('Send a user message', async () => {
    const content = `Hello AI! Test message at ${new Date().toISOString()}`;
    const result = await executeSQL(adminToken!,
      `INSERT INTO chat.messages (conversation_id, sender, role, content, status) VALUES (${testConvId}, 'demo-user', 'user', '${content}', 'sent')`
    );
    assert(result.status === 'success', `Insert failed: ${JSON.stringify(result.error)}`);
  });

  await test('Verify message appears in conversation', async () => {
    const result = await executeSQL(userToken!,
      `SELECT * FROM chat.messages WHERE conversation_id = ${testConvId} ORDER BY created_at ASC`
    );
    const messages = parseRows<{ sender: string; role: string; content: string }>(result);
    assert(messages.length >= 1, `Expected at least 1 message, got ${messages.length}`);
    assert(messages[0].role === 'user', `First message should be from user, got ${messages[0].role}`);
    assert(messages[0].sender === 'demo-user', `Sender should be demo-user, got ${messages[0].sender}`);
  });

  await test('AI service can insert reply', async () => {
    const reply = 'This is a test AI reply';
    const result = await executeSQL(serviceToken!,
      `INSERT INTO chat.messages (conversation_id, sender, role, content, status) VALUES (${testConvId}, 'ai-assistant', 'assistant', '${reply}', 'sent')`
    );
    assert(result.status === 'success', `Insert failed: ${JSON.stringify(result.error)}`);
  });

  await test('User can see AI reply (SHARED table)', async () => {
    const result = await executeSQL(userToken!,
      `SELECT * FROM chat.messages WHERE conversation_id = ${testConvId} ORDER BY created_at ASC`
    );
    const messages = parseRows<{ sender: string; role: string; content: string }>(result);
    assert(messages.length >= 2, `Expected at least 2 messages, got ${messages.length}`);
    const aiMsg = messages.find(m => m.role === 'assistant');
    assert(aiMsg !== undefined, 'User should see AI assistant reply');
    assert(aiMsg!.sender === 'ai-assistant', `AI sender should be ai-assistant, got ${aiMsg!.sender}`);
  });

  // --- Typing Indicators ---

  await test('Set typing indicator', async () => {
    const result = await executeSQL(adminToken!,
      `INSERT INTO chat.typing_indicators (conversation_id, user_name, is_typing) VALUES (${testConvId}, 'demo-user', true)`
    );
    assert(result.status === 'success', `Insert failed: ${JSON.stringify(result.error)}`);
  });

  await test('Query typing indicators', async () => {
    const result = await executeSQL(userToken!,
      `SELECT * FROM chat.typing_indicators WHERE conversation_id = ${testConvId} AND is_typing = true`
    );
    const indicators = parseRows<{ user_name: string; is_typing: boolean }>(result);
    assert(indicators.length >= 1, `Expected at least 1 typing indicator, got ${indicators.length}`);
    assert(indicators[0].user_name === 'demo-user', `Expected demo-user typing`);
  });

  await test('Clear typing indicator', async () => {
    await executeSQL(adminToken!,
      `DELETE FROM chat.typing_indicators WHERE conversation_id = ${testConvId} AND user_name = 'demo-user'`
    );
    const result = await executeSQL(userToken!,
      `SELECT * FROM chat.typing_indicators WHERE conversation_id = ${testConvId} AND user_name = 'demo-user'`
    );
    const indicators = parseRows<any>(result);
    assert(indicators.length === 0, `Expected 0 typing indicators after delete, got ${indicators.length}`);
  });

  // --- Topic Consume (kalam-link SDK pattern) ---

  await test('Consume from topic endpoint', async () => {
    const response = await fetch(`${KALAMDB_URL}/v1/api/topics/consume`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${serviceToken!}`,
      },
      body: JSON.stringify({
        topic_id: 'chat.new_messages',
        group_id: 'test-consumer-' + Date.now(),
        start: 'Latest',
        limit: 5,
        partition_id: 0,
      }),
    });
    assert(response.ok, `Consume request failed: ${response.status} ${await response.text()}`);
  });

  // --- Cleanup ---

  await test('Clean up test conversation', async () => {
    // Delete messages first
    await executeSQL(adminToken!,
      `DELETE FROM chat.messages WHERE conversation_id = ${testConvId}`
    );
    // Delete conversation
    await executeSQL(adminToken!,
      `DELETE FROM chat.conversations WHERE id = ${testConvId}`
    );
  });

  // --- Summary ---

  console.log('\n' + '='.repeat(60));
  const passed = results.filter(r => r.passed).length;
  const failed = results.filter(r => !r.passed).length;
  console.log(`\n📊 Results: ${passed} passed, ${failed} failed, ${results.length} total\n`);
  
  if (failed > 0) {
    console.log('Failed tests:');
    results.filter(r => !r.passed).forEach(r => {
      console.log(`  ❌ ${r.name}: ${r.error}`);
    });
    process.exit(1);
  }
}

runTests().catch(err => {
  console.error('\n💥 Test runner error:', err);
  process.exit(1);
});
