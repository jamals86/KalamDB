/**
 * Subscription e2e tests — subscribe, change events, unsubscribe.
 *
 * Run: node --test tests/e2e/subscription/subscription.test.mjs
 */

import { test, describe, before, after } from 'node:test';
import assert from 'node:assert/strict';
import {
  connectJwtClient,
  uniqueName,
  ensureNamespace,
  dropTable,
  sleep,
} from '../helpers.mjs';

describe('Subscription', { timeout: 60_000 }, () => {
  let client;
  const ns = uniqueName('ts_sub');
  const tbl = `${ns}.messages`;

  before(async () => {
    client = await connectJwtClient();
    await ensureNamespace(client, ns);
    await client.query(
      `CREATE TABLE IF NOT EXISTS ${tbl} (
        id INT PRIMARY KEY,
        body TEXT
      )`,
    );
  });

  after(async () => {
    await client.unsubscribeAll();
    await dropTable(client, tbl);
    await client.disconnect();
  });

  // -----------------------------------------------------------------------
  // Basic subscribe / unsubscribe
  // -----------------------------------------------------------------------
  test('subscribe returns unsubscribe function', async () => {
    const events = [];
    const unsub = await client.subscribe(tbl, (event) => {
      events.push(event);
    });
    assert.equal(typeof unsub, 'function');
    assert.ok(client.getSubscriptionCount() >= 1);

    await unsub();
  });

  // -----------------------------------------------------------------------
  // Receives subscription_ack
  // -----------------------------------------------------------------------
  test('subscribe receives subscription_ack event', async () => {
    const events = [];
    const unsub = await client.subscribe(tbl, (event) => {
      events.push(event);
    });

    // Wait for ack event
    await sleep(1500);

    const ackEvent = events.find((e) => e.type === 'subscription_ack');
    assert.ok(ackEvent, 'should receive subscription_ack');
    assert.ok(ackEvent.subscription_id, 'ack should have subscription_id');

    await unsub();
  });

  // -----------------------------------------------------------------------
  // Insert triggers change event
  // -----------------------------------------------------------------------
  test('insert triggers change event on subscriber', async () => {
    const events = [];
    const unsub = await client.subscribe(tbl, (event) => {
      events.push(event);
    });

    // Wait for initial ack
    await sleep(1500);

    // Insert from a second client
    const writer = await connectJwtClient();
    await writer.query(
      `INSERT INTO ${tbl} (id, body) VALUES (500, 'hello from writer')`,
    );

    // Wait for change event
    await sleep(3000);

    const changeEvents = events.filter((e) => e.type === 'change');
    assert.ok(changeEvents.length >= 1, 'should receive at least one change event');

    const rows = changeEvents[0].rows;
    assert.ok(rows, 'change event should have rows');
    assert.ok(rows.some((r) => r.id === 500), 'should see inserted row id');

    await unsub();
    await writer.disconnect();
  });

  // -----------------------------------------------------------------------
  // subscribeWithSql
  // -----------------------------------------------------------------------
  test('subscribeWithSql with WHERE clause works', async () => {
    const events = [];
    const unsub = await client.subscribeWithSql(
      `SELECT * FROM ${tbl} WHERE id = 600`,
      (event) => events.push(event),
    );

    await sleep(1500);

    const writer = await connectJwtClient();
    await writer.query(
      `INSERT INTO ${tbl} (id, body) VALUES (600, 'targeted')`,
    );

    await sleep(3000);

    const ack = events.find((e) => e.type === 'subscription_ack');
    assert.ok(ack, 'should get ack for sql subscription');

    await unsub();
    await writer.disconnect();
  });

  // -----------------------------------------------------------------------
  // Subscription tracking
  // -----------------------------------------------------------------------
  test('getSubscriptions / isSubscribedTo track subscriptions', async () => {
    const unsub = await client.subscribe(tbl, () => {});
    // Wait for subscription ack to register
    await sleep(1500);

    assert.ok(client.getSubscriptionCount() >= 1, 'should have at least 1 subscription');

    const subs = client.getSubscriptions();
    assert.ok(subs.length >= 1, 'getSubscriptions should return subscriptions');
    assert.ok(subs[0].id, 'subscription should have id');

    await unsub();
  });

  // -----------------------------------------------------------------------
  // unsubscribeAll
  // -----------------------------------------------------------------------
  test('unsubscribeAll clears all subscriptions', async () => {
    // Subscribe twice with different queries to ensure separate subscriptions
    await client.subscribe(tbl, () => {});
    await sleep(500);
    await client.subscribeWithSql(`SELECT * FROM ${tbl} WHERE id > 0`, () => {});
    // Wait for both subscriptions to register
    await sleep(1500);

    const count = client.getSubscriptionCount();
    assert.ok(count >= 1, `should have at least 1 subscription, got ${count}`);

    await client.unsubscribeAll();
    assert.equal(client.getSubscriptionCount(), 0);
  });
});
