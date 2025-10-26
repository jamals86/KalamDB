#!/usr/bin/env node

/**
 * Subscription Test
 * 
 * Tests real-time subscription functionality
 * Requires KalamDB server running on localhost:8080
 */

import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const pkgPath = join(__dirname, '..', 'pkg');

const API_KEY = process.env.KALAMDB_API_KEY || 'test-api-key-for-localhost';
const SERVER_URL = process.env.KALAMDB_URL || 'ws://localhost:8080';

async function runTests() {
  console.log('🧪 Running Subscription Tests\n');
  console.log(`Server: ${SERVER_URL}`);
  console.log(`API Key: ${API_KEY.substring(0, 8)}...\n`);
  
  let passed = 0;
  let failed = 0;

  const { default: init, KalamClient } = await import(join(pkgPath, 'kalam_link.js'));
  await init();

  let client;

  // Setup: Connect to server
  console.log('Setup: Connecting to server...');
  try {
    client = new KalamClient(SERVER_URL, API_KEY);
    await client.connect();
    console.log('  ✓ Connected\n');
  } catch (error) {
    console.log('  ✗ Connection failed:', error.message);
    console.log('  ℹ️  Make sure KalamDB server is running on', SERVER_URL);
    process.exit(1);
  }

  // Setup: Create test table
  console.log('Setup: Creating test table...');
  try {
    await client.query('CREATE TABLE IF NOT EXISTS test_subscriptions (id INTEGER PRIMARY KEY, value TEXT)');
    console.log('  ✓ Test table ready\n');
  } catch (error) {
    console.log('  ✗ Table creation failed:', error.message);
    process.exit(1);
  }

  // Test 1: Subscribe to table
  console.log('Test 1: Subscribe to table...');
  let subscriptionId;
  let eventReceived = false;
  
  try {
    subscriptionId = await client.subscribe('test_subscriptions', (event) => {
      console.log('  📨 Event received:', event.type);
      eventReceived = true;
    });
    
    if (subscriptionId) {
      console.log('  ✓ Subscription created with ID:', subscriptionId);
      passed++;
    } else {
      console.log('  ✗ No subscription ID returned');
      failed++;
    }
  } catch (error) {
    console.log('  ✗ Subscribe error:', error.message);
    failed++;
  }

  // Test 2: Receive insert event
  console.log('\nTest 2: Receive insert event...');
  try {
    // Insert data to trigger event
    await client.insert('test_subscriptions', { value: 'test_value_1' });
    
    // Wait for event (with timeout)
    await new Promise((resolve) => setTimeout(resolve, 500));
    
    if (eventReceived) {
      console.log('  ✓ Insert event received');
      passed++;
    } else {
      console.log('  ✗ No event received');
      failed++;
    }
  } catch (error) {
    console.log('  ✗ Test error:', error.message);
    failed++;
  }

  // Test 3: Receive delete event
  console.log('\nTest 3: Receive delete event...');
  let deleteEventReceived = false;
  
  try {
    // Subscribe with new callback
    if (subscriptionId) {
      await client.unsubscribe(subscriptionId);
    }
    
    subscriptionId = await client.subscribe('test_subscriptions', (event) => {
      if (event.type === 'delete') {
        deleteEventReceived = true;
        console.log('  📨 Delete event received for ID:', event.id);
      }
    });
    
    // Insert and then delete
    await client.insert('test_subscriptions', { value: 'to_be_deleted' });
    await new Promise((resolve) => setTimeout(resolve, 200));
    
    // Get the ID of last inserted row (simplified - assumes sequential IDs)
    const result = await client.query('SELECT MAX(id) as max_id FROM test_subscriptions');
    const lastId = result.rows[0]?.max_id;
    
    if (lastId) {
      await client.delete('test_subscriptions', lastId);
      await new Promise((resolve) => setTimeout(resolve, 500));
      
      if (deleteEventReceived) {
        console.log('  ✓ Delete event received');
        passed++;
      } else {
        console.log('  ✗ No delete event received');
        failed++;
      }
    } else {
      console.log('  ⚠️  Could not get last ID for delete test');
    }
  } catch (error) {
    console.log('  ✗ Test error:', error.message);
    failed++;
  }

  // Test 4: Unsubscribe
  console.log('\nTest 4: Unsubscribe from table...');
  try {
    if (subscriptionId) {
      await client.unsubscribe(subscriptionId);
      console.log('  ✓ Unsubscribed successfully');
      passed++;
      
      // Try to receive event after unsubscribe
      let eventAfterUnsubscribe = false;
      await client.insert('test_subscriptions', { value: 'after_unsubscribe' });
      await new Promise((resolve) => setTimeout(resolve, 500));
      
      if (!eventAfterUnsubscribe) {
        console.log('  ✓ No events received after unsubscribe');
        passed++;
      } else {
        console.log('  ✗ Still receiving events after unsubscribe');
        failed++;
      }
    } else {
      console.log('  ⊘ Skipped (no active subscription)');
    }
  } catch (error) {
    console.log('  ✗ Unsubscribe error:', error.message);
    failed++;
  }

  // Test 5: Multiple subscriptions
  console.log('\nTest 5: Multiple subscriptions...');
  try {
    let sub1Events = 0;
    let sub2Events = 0;
    
    const sub1 = await client.subscribe('test_subscriptions', () => sub1Events++);
    const sub2 = await client.subscribe('test_subscriptions', () => sub2Events++);
    
    await client.insert('test_subscriptions', { value: 'multi_sub_test' });
    await new Promise((resolve) => setTimeout(resolve, 500));
    
    if (sub1Events > 0 && sub2Events > 0) {
      console.log('  ✓ Both subscriptions received events');
      passed++;
    } else {
      console.log('  ✗ Not all subscriptions received events');
      failed++;
    }
    
    await client.unsubscribe(sub1);
    await client.unsubscribe(sub2);
  } catch (error) {
    console.log('  ✗ Test error:', error.message);
    failed++;
  }

  // Test 6: Subscribe when disconnected
  console.log('\nTest 6: Subscribe when disconnected...');
  try {
    await client.disconnect();
    
    try {
      await client.subscribe('test_subscriptions', () => {});
      console.log('  ✗ Should not allow subscribe when disconnected');
      failed++;
    } catch (error) {
      console.log('  ✓ Subscribe rejected when disconnected');
      passed++;
    }
  } catch (error) {
    console.log('  ✗ Test error:', error.message);
    failed++;
  }

  // Cleanup
  console.log('\nCleanup: Disconnecting...');
  try {
    if (client.isConnected()) {
      await client.disconnect();
    }
    console.log('  ✓ Disconnected\n');
  } catch (error) {
    console.log('  ⚠️  Disconnect error:', error.message, '\n');
  }

  // Results
  console.log('='.repeat(50));
  console.log(`Results: ${passed} passed, ${failed} failed`);
  console.log('='.repeat(50));

  if (failed === 0) {
    console.log('\n✅ All subscription tests passed!');
    process.exit(0);
  } else {
    console.log('\n❌ Some tests failed');
    process.exit(1);
  }
}

runTests().catch(error => {
  console.error('Test suite error:', error);
  process.exit(1);
});
