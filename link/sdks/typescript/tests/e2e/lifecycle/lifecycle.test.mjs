/**
 * Client lifecycle e2e tests — connect, disconnect, reconnect, disableCompression.
 *
 * Run: node --test tests/e2e/lifecycle/lifecycle.test.mjs
 */

import { test, describe, after } from 'node:test';
import assert from 'node:assert/strict';
import {
  SERVER_URL,
  ADMIN_USER,
  ADMIN_PASS,
  connectJwtClient,
  sleep,
} from '../helpers.mjs';
import { createClient, Auth } from '../../../dist/src/index.js';

describe('Client Lifecycle', { timeout: 30_000 }, () => {
  // -----------------------------------------------------------------------
  // Connect / disconnect
  // -----------------------------------------------------------------------
  test('connect then disconnect toggles isConnected', async () => {
    const client = await connectJwtClient();
    assert.equal(client.isConnected(), true);

    await client.disconnect();
    assert.equal(client.isConnected(), false);
  });

  // -----------------------------------------------------------------------
  // Reconnect on disconnect
  // -----------------------------------------------------------------------
  test('setAutoReconnect / setReconnectDelay / setMaxReconnectAttempts', async () => {
    const client = await connectJwtClient();

    // These should not throw
    client.setAutoReconnect(true);
    client.setReconnectDelay(500, 5000);
    client.setMaxReconnectAttempts(3);

    assert.equal(client.getReconnectAttempts(), 0);
    assert.equal(client.isReconnecting(), false);

    await client.disconnect();
  });

  // -----------------------------------------------------------------------
  // disableCompression passes ?compress=false
  // -----------------------------------------------------------------------
  test('disableCompression: true still connects and queries', async () => {
    const client = createClient({
      url: SERVER_URL,
      auth: Auth.basic(ADMIN_USER, ADMIN_PASS),
      disableCompression: true,
    });

    await client.connect();
    assert.ok(client.isConnected());

    const resp = await client.query("SELECT 'no-compress' AS val");
    assert.ok(resp.results?.length > 0);

    await client.disconnect();
  });

  // -----------------------------------------------------------------------
  // autoConnect
  // -----------------------------------------------------------------------
  test('autoConnect: false does not connect until explicit connect()', async () => {
    const client = createClient({
      url: SERVER_URL,
      auth: Auth.basic(ADMIN_USER, ADMIN_PASS),
      autoConnect: false,
    });

    // Not connected yet — isConnected should be false
    assert.equal(client.isConnected(), false);

    // Query forces init (auto-jwt-login via ensureJwtForBasicAuth)
    const resp = await client.query('SELECT 1 AS n');
    assert.ok(resp.results?.length > 0);

    await client.disconnect();
  });

  // -----------------------------------------------------------------------
  // Connection event callbacks
  // -----------------------------------------------------------------------
  test('onConnect callback fires', async () => {
    let connectFired = false;

    const client = createClient({
      url: SERVER_URL,
      auth: Auth.basic(ADMIN_USER, ADMIN_PASS),
      onConnect: () => {
        connectFired = true;
      },
    });

    await client.connect();
    await sleep(500);

    assert.ok(connectFired, 'onConnect should fire');
    await client.disconnect();
  });

  // -----------------------------------------------------------------------
  // Multiple connect calls are idempotent
  // -----------------------------------------------------------------------
  test('calling connect() twice is safe', async () => {
    const client = createClient({
      url: SERVER_URL,
      auth: Auth.basic(ADMIN_USER, ADMIN_PASS),
    });

    await client.connect();
    await client.connect(); // should not throw
    assert.ok(client.isConnected());

    await client.disconnect();
  });
});
