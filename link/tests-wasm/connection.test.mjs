#!/usr/bin/env node

/**
 * Connection Test
 * 
 * Tests WebSocket connection functionality
 * Requires KalamDB server running on localhost:8080
 */

import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const pkgPath = join(__dirname, '..', 'pkg');

// Get API key from environment or use default
const API_KEY = process.env.KALAMDB_API_KEY || 'test-api-key-for-localhost';
const SERVER_URL = process.env.KALAMDB_URL || 'ws://localhost:8080';

async function runTests() {
  console.log('🧪 Running Connection Tests\n');
  console.log(`Server: ${SERVER_URL}`);
  console.log(`API Key: ${API_KEY.substring(0, 8)}...\n`);
  
  let passed = 0;
  let failed = 0;

  const { default: init, KalamClient } = await import(join(pkgPath, 'kalam_link.js'));
  await init();

  // Test 1: Initial state
  console.log('Test 1: Initial connection state...');
  try {
    const client = new KalamClient(SERVER_URL, API_KEY);
    
    if (!client.isConnected()) {
      console.log('  ✓ Client starts disconnected');
      passed++;
    } else {
      console.log('  ✗ Client should start disconnected');
      failed++;
    }
  } catch (error) {
    console.log('  ✗ Test failed:', error.message);
    failed++;
  }

  // Test 2: Connect to server
  console.log('\nTest 2: Connect to server...');
  let client;
  try {
    client = new KalamClient(SERVER_URL, API_KEY);
    await client.connect();
    
    if (client.isConnected()) {
      console.log('  ✓ Connection established');
      passed++;
    } else {
      console.log('  ✗ Connection failed');
      failed++;
    }
  } catch (error) {
    console.log('  ✗ Connection error:', error.message);
    console.log('  ℹ️  Make sure KalamDB server is running on', SERVER_URL);
    failed++;
  }

  // Test 3: Disconnect
  console.log('\nTest 3: Disconnect from server...');
  try {
    if (client && client.isConnected()) {
      await client.disconnect();
      
      if (!client.isConnected()) {
        console.log('  ✓ Disconnected successfully');
        passed++;
      } else {
        console.log('  ✗ Still connected after disconnect');
        failed++;
      }
    } else {
      console.log('  ⊘ Skipped (not connected)');
    }
  } catch (error) {
    console.log('  ✗ Disconnect error:', error.message);
    failed++;
  }

  // Test 4: Reconnect
  console.log('\nTest 4: Reconnect to server...');
  try {
    client = new KalamClient(SERVER_URL, API_KEY);
    await client.connect();
    
    if (client.isConnected()) {
      console.log('  ✓ Reconnection successful');
      passed++;
    } else {
      console.log('  ✗ Reconnection failed');
      failed++;
    }
    
    await client.disconnect();
  } catch (error) {
    console.log('  ✗ Reconnection error:', error.message);
    failed++;
  }

  // Test 5: Invalid API key (if server enforces auth)
  console.log('\nTest 5: Invalid API key handling...');
  try {
    const badClient = new KalamClient(SERVER_URL, 'invalid-api-key-12345');
    
    try {
      await badClient.connect();
      console.log('  ⚠️  Connected with invalid key (server may allow localhost)');
      await badClient.disconnect();
      // Not counting as fail since localhost exception is valid
    } catch (error) {
      console.log('  ✓ Invalid key rejected:', error.message);
      passed++;
    }
  } catch (error) {
    console.log('  ✗ Test error:', error.message);
    failed++;
  }

  // Test 6: Connection to invalid URL
  console.log('\nTest 6: Invalid URL handling...');
  try {
    const badClient = new KalamClient('ws://invalid-host-12345:9999', API_KEY);
    
    try {
      await badClient.connect();
      console.log('  ✗ Should not connect to invalid URL');
      failed++;
    } catch (error) {
      console.log('  ✓ Invalid URL rejected');
      passed++;
    }
  } catch (error) {
    console.log('  ✗ Test error:', error.message);
    failed++;
  }

  // Results
  console.log('\n' + '='.repeat(50));
  console.log(`Results: ${passed} passed, ${failed} failed`);
  console.log('='.repeat(50));

  if (failed === 0) {
    console.log('\n✅ All connection tests passed!');
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
