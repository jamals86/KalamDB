#!/usr/bin/env node

/**
 * WebSocket and SQL Integration Test
 * 
 * Tests real WebSocket connection and SQL operations against KalamDB server
 * Requires: Server running on localhost:8080 with API key: test-api-key-12345
 */

import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { readFile } from 'fs/promises';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const sdkPath = join(__dirname, '..');
const API_KEY = 'test-api-key-12345';
const WS_URL = 'ws://localhost:8080';
const HTTP_URL = 'http://localhost:8080';

// Test configuration
const TEST_NAMESPACE = 'app';
const TEST_TABLE = 'sdk_test_todos';
const FULL_TABLE_NAME = `${TEST_NAMESPACE}.${TEST_TABLE}`;

let testsPassed = 0;
let testsFailed = 0;

function log(message) {
  console.log(`  ${message}`);
}

function pass(message) {
  testsPassed++;
  log(`✓ ${message}`);
}

function fail(message) {
  testsFailed++;
  log(`✗ ${message}`);
}

async function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function runTests() {
  console.log('🧪 Running WebSocket and SQL Integration Tests\n');
  console.log(`Server: ${HTTP_URL}`);
  console.log(`API Key: ${API_KEY}`);
  console.log(`Test Table: ${FULL_TABLE_NAME}\n`);

  // Initialize WASM
  let KalamClient;
  try {
    const { default: init, KalamClient: Client } = await import(join(sdkPath, 'kalam_link.js'));
    const wasmBuffer = await readFile(join(sdkPath, 'kalam_link_bg.wasm'));
    await init(wasmBuffer);
    KalamClient = Client;
    pass('WASM initialized');
  } catch (error) {
    fail(`WASM initialization: ${error.message}`);
    process.exit(1);
  }

  // Test 1: Server Health Check
  console.log('\n📋 Test 1: Server Health Check');
  try {
    const response = await fetch(`${HTTP_URL}/health`);
    if (response.ok) {
      pass('Server is running');
    } else {
      fail(`Server returned ${response.status}`);
    }
  } catch (error) {
    fail(`Cannot connect to server: ${error.message}`);
    console.log('\n⚠️  Please start the server: cd backend && cargo run');
    process.exit(1);
  }

  // Test 2: Create Client
  console.log('\n📋 Test 2: Client Creation');
  let client;
  try {
    client = new KalamClient(WS_URL, API_KEY);
    pass('Client created');
  } catch (error) {
    fail(`Client creation: ${error.message}`);
    process.exit(1);
  }

  // Test 3: WebSocket Connection
  console.log('\n📋 Test 3: WebSocket Connection');
  try {
    await client.connect();
    pass('WebSocket connected');
    
    await sleep(100); // Give connection time to stabilize
    
    if (client.isConnected()) {
      pass('Connection status verified');
    } else {
      fail('isConnected() returned false');
    }
  } catch (error) {
    fail(`WebSocket connection: ${error.message}`);
    process.exit(1);
  }

  // Test 4: Create Test Table
  console.log('\n📋 Test 4: Create Test Table');
  try {
    const createSQL = `
      CREATE TABLE IF NOT EXISTS ${FULL_TABLE_NAME} (
        id BIGINT,
        title TEXT,
        completed BOOLEAN
      )
    `;
    
    const result = await client.query(createSQL);
    
    if (result && result.status === 'success') {
      pass('Test table created');
    } else if (result && result.status === 'error') {
      fail(`Create table failed: ${result.error?.message || 'Unknown error'}`);
    } else {
      fail(`Unexpected response format: ${JSON.stringify(result)}`);
    }
  } catch (error) {
    fail(`Create table: ${error.message}`);
  }

  // Test 5: Clear Test Data
  console.log('\n📋 Test 5: Clear Test Data');
  try {
    const deleteSQL = `DELETE FROM ${FULL_TABLE_NAME}`;
    const result = await client.query(deleteSQL);
    
    if (result && result.status === 'success') {
      pass('Test data cleared');
    } else {
      fail(`Clear data failed: ${result?.error?.message || 'Unknown error'}`);
    }
  } catch (error) {
    fail(`Clear data: ${error.message}`);
  }

  // Test 6: Insert via SQL Query
  console.log('\n📋 Test 6: Insert via SQL Query');
  try {
    const insertSQL = `
      INSERT INTO ${FULL_TABLE_NAME} (title, completed)
      VALUES ('Test SQL Insert', false)
    `;
    
    const result = await client.query(insertSQL);
    
    if (result && result.status === 'success') {
      pass('SQL INSERT executed');
    } else {
      fail(`SQL INSERT failed: ${result?.error?.message || 'Unknown error'}`);
    }
  } catch (error) {
    fail(`SQL INSERT: ${error.message}`);
  }

  // Test 7: Select Query
  console.log('\n📋 Test 7: Select Query');
  let firstId;
  try {
    const selectSQL = `SELECT * FROM ${FULL_TABLE_NAME} ORDER BY id`;
    const result = await client.query(selectSQL);
    
    if (result && result.status === 'success') {
      pass('SELECT executed');
      
      if (result.results && result.results[0] && result.results[0].rows) {
        const rows = result.results[0].rows;
        
        if (rows.length > 0) {
          pass(`Found ${rows.length} row(s)`);
          
          const row = rows[0];
          if (row.title === 'Test SQL Insert' && row.completed === false) {
            pass('Row data matches INSERT');
            firstId = row.id;
            log(`  → ID: ${firstId}`);
          } else {
            fail(`Row data mismatch: ${JSON.stringify(row)}`);
          }
        } else {
          fail('No rows returned');
        }
      } else {
        fail(`Unexpected result format: ${JSON.stringify(result)}`);
      }
    } else {
      fail(`SELECT failed: ${result?.error?.message || 'Unknown error'}`);
    }
  } catch (error) {
    fail(`SELECT: ${error.message}`);
  }

  // Test 8: Insert via WASM Method
  console.log('\n📋 Test 8: Insert via WASM Method');
  try {
    const data = JSON.stringify({
      title: 'Test WASM Insert',
      completed: true
    });
    
    const result = await client.insert(FULL_TABLE_NAME, data);
    
    if (result && result.status === 'success') {
      pass('WASM insert() executed');
    } else {
      fail(`WASM insert() failed: ${result?.error?.message || 'Unknown error'}`);
    }
  } catch (error) {
    fail(`WASM insert(): ${error.message}`);
  }

  // Test 9: Verify Multiple Rows
  console.log('\n📋 Test 9: Verify Multiple Rows');
  try {
    const selectSQL = `SELECT * FROM ${FULL_TABLE_NAME} ORDER BY id`;
    const result = await client.query(selectSQL);
    
    if (result && result.status === 'success' && result.results[0]?.rows) {
      const rows = result.results[0].rows;
      
      if (rows.length === 2) {
        pass('Both rows exist');
        
        const row1 = rows[0];
        const row2 = rows[1];
        
        if (row1.title === 'Test SQL Insert' && row2.title === 'Test WASM Insert') {
          pass('Both inserts verified');
        } else {
          fail(`Row titles mismatch: ${row1.title}, ${row2.title}`);
        }
        
        if (row1.completed === false && row2.completed === true) {
          pass('Boolean values correct');
        } else {
          fail(`Completed flags mismatch: ${row1.completed}, ${row2.completed}`);
        }
      } else {
        fail(`Expected 2 rows, got ${rows.length}`);
      }
    } else {
      fail('Failed to fetch rows');
    }
  } catch (error) {
    fail(`Verify rows: ${error.message}`);
  }

  // Test 10: Delete via WASM Method
  console.log('\n📋 Test 10: Delete via WASM Method');
  if (firstId) {
    try {
      const result = await client.delete(FULL_TABLE_NAME, firstId);
      
      if (result && result.status === 'success') {
        pass('WASM delete() executed');
        
        // Verify deletion
        const selectSQL = `SELECT * FROM ${FULL_TABLE_NAME} WHERE id = ${firstId}`;
        const verifyResult = await client.query(selectSQL);
        
        if (verifyResult?.results[0]?.rows?.length === 0) {
          pass('Row deleted successfully');
        } else {
          fail('Row still exists after delete');
        }
      } else {
        fail(`WASM delete() failed: ${result?.error?.message || 'Unknown error'}`);
      }
    } catch (error) {
      fail(`WASM delete(): ${error.message}`);
    }
  } else {
    fail('No ID available for delete test');
  }

  // Test 11: WebSocket Subscription
  console.log('\n📋 Test 11: WebSocket Subscription');
  let subscriptionReceived = false;
  let subscriptionId;
  
  try {
    // Subscribe to table
    subscriptionId = await client.subscribe(
      FULL_TABLE_NAME,
      null, // Start from beginning
      (message) => {
        subscriptionReceived = true;
        log(`  → Subscription message: ${JSON.stringify(message).substring(0, 100)}...`);
      }
    );
    
    if (subscriptionId) {
      pass(`Subscription created: ${subscriptionId}`);
      
      // Wait a bit for subscription to activate
      await sleep(200);
      
      // Insert a row to trigger subscription
      const insertSQL = `
        INSERT INTO ${FULL_TABLE_NAME} (title, completed)
        VALUES ('Subscription Test', false)
      `;
      await client.query(insertSQL);
      
      // Wait for message
      await sleep(500);
      
      if (subscriptionReceived) {
        pass('Subscription received message');
      } else {
        fail('No subscription message received (timeout)');
      }
    } else {
      fail('Subscription returned no ID');
    }
  } catch (error) {
    fail(`Subscription: ${error.message}`);
  }

  // Test 12: Unsubscribe
  console.log('\n📋 Test 12: Unsubscribe');
  if (subscriptionId) {
    try {
      await client.unsubscribe(subscriptionId);
      pass('Unsubscribed successfully');
      
      // Reset flag
      subscriptionReceived = false;
      
      // Insert another row - should NOT trigger callback
      const insertSQL = `
        INSERT INTO ${FULL_TABLE_NAME} (title, completed)
        VALUES ('After Unsubscribe', false)
      `;
      await client.query(insertSQL);
      await sleep(300);
      
      if (!subscriptionReceived) {
        pass('No message after unsubscribe');
      } else {
        fail('Received message after unsubscribe');
      }
    } catch (error) {
      fail(`Unsubscribe: ${error.message}`);
    }
  } else {
    fail('No subscription ID to unsubscribe');
  }

  // Test 13: Disconnect
  console.log('\n📋 Test 13: Disconnect');
  try {
    client.disconnect();
    await sleep(100);
    
    if (!client.isConnected()) {
      pass('WebSocket disconnected');
    } else {
      fail('Still connected after disconnect()');
    }
  } catch (error) {
    fail(`Disconnect: ${error.message}`);
  }

  // Test 14: Reconnect
  console.log('\n📋 Test 14: Reconnect');
  try {
    await client.connect();
    await sleep(100);
    
    if (client.isConnected()) {
      pass('Reconnected successfully');
      
      // Verify can still query
      const result = await client.query(`SELECT COUNT(*) as count FROM ${FULL_TABLE_NAME}`);
      if (result && result.status === 'success') {
        pass('Queries work after reconnect');
      } else {
        fail('Queries failed after reconnect');
      }
    } else {
      fail('Reconnect failed');
    }
  } catch (error) {
    fail(`Reconnect: ${error.message}`);
  }

  // Cleanup
  console.log('\n📋 Cleanup');
  try {
    client.disconnect();
    pass('Client disconnected');
  } catch (error) {
    fail(`Cleanup: ${error.message}`);
  }

  // Results
  console.log('\n' + '='.repeat(60));
  console.log(`Results: ${testsPassed} passed, ${testsFailed} failed`);
  console.log('='.repeat(60));

  if (testsFailed === 0) {
    console.log('\n✅ All tests passed!');
    process.exit(0);
  } else {
    console.log('\n❌ Some tests failed');
    process.exit(1);
  }
}

runTests().catch(error => {
  console.error('💥 Test suite error:', error);
  process.exit(1);
});
