import assert from 'node:assert/strict';
import test from 'node:test';

import { createClient } from '../dist/src/index.js';

function createFakeWasmClient({ subscribeError } = {}) {
  let connected = false;
  let nextSubscriptionId = 0;
  const callbacks = new Map();

  return {
    connectCalls: 0,
    subscribeCalls: 0,
    setAuthProvider() {},
    setWsLazyConnect() {},
    onConnect() {},
    onDisconnect() {},
    onError() {},
    onReceive() {},
    onSend() {},
    isConnected() {
      return connected;
    },
    async connect() {
      this.connectCalls += 1;
      await new Promise((resolve) => setTimeout(resolve, 10));
      connected = true;
    },
    async subscribeWithSql(_sql, _optionsJson, callback) {
      this.subscribeCalls += 1;
      if (subscribeError) {
        throw new Error(subscribeError);
      }
      nextSubscriptionId += 1;
      const subscriptionId = `sub-${nextSubscriptionId}`;
      callbacks.set(subscriptionId, callback);
      return subscriptionId;
    },
    async liveQueryRowsWithSql(_sql, _optionsJson, callback) {
      this.subscribeCalls += 1;
      if (subscribeError) {
        throw new Error(subscribeError);
      }
      nextSubscriptionId += 1;
      const subscriptionId = `sub-${nextSubscriptionId}`;
      callbacks.set(subscriptionId, callback);
      return subscriptionId;
    },
    async unsubscribe(subscriptionId) {
      callbacks.delete(subscriptionId);
    },
    async disconnect() {
      connected = false;
    },
    emit(subscriptionId, event) {
      const callback = callbacks.get(subscriptionId);
      if (!callback) {
        throw new Error(`No callback registered for ${subscriptionId}`);
      }
      callback(JSON.stringify(event));
    },
    getSubscriptions() {
      return '[]';
    },
  };
}

test('multiple subscriptions on one client share one websocket connection', async () => {
  const client = createClient({
    url: 'http://127.0.0.1:8080',
    authProvider: async () => ({ type: 'none' }),
  });

  const fakeWasmClient = createFakeWasmClient();

  client.initialized = true;
  client.wasmClient = fakeWasmClient;

  const [unsubscribeMessages, unsubscribeEvents] = await Promise.all([
    client.subscribeWithSql('SELECT * FROM chat_demo.messages', () => {}),
    client.subscribeWithSql('SELECT * FROM chat_demo.agent_events', () => {}),
  ]);

  assert.equal(fakeWasmClient.connectCalls, 1);
  assert.equal(fakeWasmClient.subscribeCalls, 2);
  assert.equal(client.getSubscriptionCount(), 2);

  await unsubscribeMessages();
  await unsubscribeEvents();

  assert.equal(client.getSubscriptionCount(), 0);
});

test('failed subscriptions do not leak local subscription state', async () => {
  const client = createClient({
    url: 'http://127.0.0.1:8080',
    authProvider: async () => ({ type: 'none' }),
  });

  const fakeWasmClient = createFakeWasmClient({
    subscribeError: 'Subscription failed (NOT_FOUND): table missing',
  });

  client.initialized = true;
  client.wasmClient = fakeWasmClient;

  await assert.rejects(
    client.subscribeWithSql('SELECT * FROM missing.table', () => {}),
    /Subscription failed \(NOT_FOUND\): table missing/,
  );

  assert.equal(fakeWasmClient.connectCalls, 1);
  assert.equal(fakeWasmClient.subscribeCalls, 1);
  assert.equal(client.getSubscriptionCount(), 0);
});

test('subscribeWithSql normalizes websocket rows into RowData cells', async () => {
  const client = createClient({
    url: 'http://127.0.0.1:8080',
    authProvider: async () => ({ type: 'none' }),
  });

  const fakeWasmClient = createFakeWasmClient();
  client.initialized = true;
  client.wasmClient = fakeWasmClient;

  const events = [];
  const unsubscribe = await client.subscribeWithSql('SELECT * FROM chat_demo.messages', (event) => {
    events.push(event);
  });

  fakeWasmClient.emit('sub-1', {
    type: 'initial_data_batch',
    subscription_id: 'sub-1',
    rows: [{ id: '1', content: 'hello', created_at: '123' }],
    batch_control: { batch_num: 1, has_more: false, status: 'ready', last_seq_id: null },
  });

  assert.equal(events.length, 1);
  assert.equal(events[0].rows[0].id.asString(), '1');
  assert.equal(events[0].rows[0].content.asString(), 'hello');

  await unsubscribe();
});

test('liveQueryRowsWithSql delegates materialized rows to the Rust/WASM layer', async () => {
  const client = createClient({
    url: 'http://127.0.0.1:8080',
    authProvider: async () => ({ type: 'none' }),
  });

  const fakeWasmClient = createFakeWasmClient();
  client.initialized = true;
  client.wasmClient = fakeWasmClient;

  const snapshots = [];
  const unsubscribe = await client.liveQueryRowsWithSql(
    'SELECT * FROM chat_demo.messages ORDER BY id ASC',
    (rows) => {
      snapshots.push(rows.map((row) => ({
        id: row.id.asString(),
        content: row.content.asString(),
      })));
    },
    {
      limit: 2,
    },
  );

  fakeWasmClient.emit('sub-1', {
    type: 'rows',
    subscription_id: 'sub-1',
    rows: [{ id: '1', content: 'one' }],
  });
  fakeWasmClient.emit('sub-1', {
    type: 'rows',
    subscription_id: 'sub-1',
    rows: [
      { id: '1', content: 'one' },
      { id: '2', content: 'two' },
    ],
  });
  fakeWasmClient.emit('sub-1', {
    type: 'rows',
    subscription_id: 'sub-1',
    rows: [
      { id: '1', content: 'one' },
      { id: '2', content: 'two-updated' },
    ],
  });
  fakeWasmClient.emit('sub-1', {
    type: 'rows',
    subscription_id: 'sub-1',
    rows: [
      { id: '2', content: 'two-updated' },
      { id: '3', content: 'three' },
    ],
  });
  fakeWasmClient.emit('sub-1', {
    type: 'rows',
    subscription_id: 'sub-1',
    rows: [{ id: '3', content: 'three' }],
  });

  assert.deepEqual(snapshots[0], [{ id: '1', content: 'one' }]);
  assert.deepEqual(snapshots[1], [
    { id: '1', content: 'one' },
    { id: '2', content: 'two' },
  ]);
  assert.deepEqual(snapshots[2], [
    { id: '1', content: 'one' },
    { id: '2', content: 'two-updated' },
  ]);
  assert.deepEqual(snapshots[3], [
    { id: '2', content: 'two-updated' },
    { id: '3', content: 'three' },
  ]);
  assert.deepEqual(snapshots[4], [{ id: '3', content: 'three' }]);

  await unsubscribe();
});