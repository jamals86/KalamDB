#!/usr/bin/env node

/**
 * Query Test
 * 
 * Tests SQL query execution functionality
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
  console.log('🧪 Running Query Tests\n');
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

  // Test 1: Simple SELECT query
  console.log('Test 1: Simple SELECT query...');
  try {
    const result = await client.query('SELECT 1 as test');
    
    if (result && result.rows) {
      console.log('  ✓ Query executed successfully');
      console.log('  ✓ Result has rows array');
      passed += 2;
      
      if (result.rows.length > 0) {
        console.log('  ✓ Query returned data:', result.rows[0]);
        passed++;
      } else {
        console.log('  ⚠️  Query returned empty result');
      }
    } else {
      console.log('  ✗ Invalid result format');
      failed++;
    }
  } catch (error) {
    console.log('  ✗ Query error:', error.message);
    failed += 3;
  }

  // Test 2: Create test table
  console.log('\nTest 2: CREATE TABLE...');
  try {
    await client.query('CREATE TABLE IF NOT EXISTS test_sdk (id INTEGER PRIMARY KEY, name TEXT)');
    console.log('  ✓ Table created successfully');
    passed++;
  } catch (error) {
    console.log('  ✗ CREATE TABLE error:', error.message);
    failed++;
  }

  // Test 3: INSERT via query
  console.log('\nTest 3: INSERT via query...');
  try {
    const result = await client.query("INSERT INTO test_sdk (name) VALUES ('test_row')");
    console.log('  ✓ INSERT executed successfully');
    passed++;
  } catch (error) {
    console.log('  ✗ INSERT error:', error.message);
    failed++;
  }

  // Test 4: SELECT from created table
  console.log('\nTest 4: SELECT from created table...');
  try {
    const result = await client.query('SELECT * FROM test_sdk');
    
    if (result && result.rows) {
      console.log('  ✓ SELECT executed successfully');
      passed++;
      
      if (result.rows.length > 0) {
        console.log('  ✓ Data found:', result.rows.length, 'rows');
        passed++;
      } else {
        console.log('  ⚠️  No data returned');
      }
    } else {
      console.log('  ✗ Invalid result format');
      failed++;
    }
  } catch (error) {
    console.log('  ✗ SELECT error:', error.message);
    failed += 2;
  }

  // Test 5: Invalid SQL
  console.log('\nTest 5: Invalid SQL handling...');
  try {
    await client.query('INVALID SQL STATEMENT');
    console.log('  ✗ Should have thrown error for invalid SQL');
    failed++;
  } catch (error) {
    console.log('  ✓ Invalid SQL rejected:', error.message.substring(0, 50));
    passed++;
  }

  // Test 6: Empty query
  console.log('\nTest 6: Empty query handling...');
  try {
    await client.query('');
    console.log('  ✗ Should have thrown error for empty query');
    failed++;
  } catch (error) {
    console.log('  ✓ Empty query rejected');
    passed++;
  }

  // Test 7: Query when disconnected
  console.log('\nTest 7: Query when disconnected...');
  try {
    await client.disconnect();
    await client.query('SELECT 1');
    console.log('  ✗ Should have thrown error when disconnected');
    failed++;
  } catch (error) {
    console.log('  ✓ Query rejected when disconnected');
    passed++;
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
    console.log('\n✅ All query tests passed!');
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
