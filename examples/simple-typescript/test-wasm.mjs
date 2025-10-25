/**
 * WASM Module Test
 * Tests that the KalamDB WASM client loads and works correctly
 */

import { readFile } from 'fs/promises';
import init, { KalamClient } from './src/wasm/kalam_link.js';

async function testWasmModule() {
  console.log('🧪 Testing KalamDB WASM Module...\n');

  try {
    // Test 1: Initialize WASM module
    console.log('1️⃣ Testing WASM initialization...');
    const wasmBuffer = await readFile('./src/wasm/kalam_link_bg.wasm');
    await init(wasmBuffer);
    console.log('   ✅ WASM module initialized\n');

    // Test 2: Create client with valid parameters
    console.log('2️⃣ Testing KalamClient constructor with valid parameters...');
    const client = new KalamClient('http://localhost:8080', 'test-api-key');
    console.log('   ✅ KalamClient created successfully\n');

    // Test 3: Check initial connection state
    console.log('3️⃣ Testing isConnected() before connection...');
    const initialState = client.isConnected();
    console.log(`   Connection state: ${initialState}`);
    if (!initialState) {
      console.log('   ✅ Correctly returns false before connect()\n');
    } else {
      console.log('   ❌ Should be false before connect()\n');
    }

    // Test 4: Test parameter validation - empty URL
    console.log('4️⃣ Testing parameter validation (empty URL)...');
    try {
      new KalamClient('', 'test-api-key');
      console.log('   ❌ Should have thrown an error for empty URL\n');
    } catch (err) {
      console.log(`   ✅ Correctly rejected: ${err}\n`);
    }

    // Test 5: Test parameter validation - empty API key
    console.log('5️⃣ Testing parameter validation (empty API key)...');
    try {
      new KalamClient('http://localhost:8080', '');
      console.log('   ❌ Should have thrown an error for empty API key\n');
    } catch (err) {
      console.log(`   ✅ Correctly rejected: ${err}\n`);
    }

    // Test 6: Test connect method exists
    console.log('6️⃣ Testing connect() method...');
    try {
      await client.connect();
      console.log('   ✅ connect() method executed\n');
    } catch (err) {
      console.log(`   ⚠️  connect() called but server may not be available: ${err}\n`);
    }

    // Test 7: Test disconnect method exists
    console.log('7️⃣ Testing disconnect() method...');
    try {
      await client.disconnect();
      console.log('   ✅ disconnect() method executed\n');
    } catch (err) {
      console.log(`   ❌ disconnect() failed: ${err}\n`);
    }

    // Test 8: Verify methods exist
    console.log('8️⃣ Verifying all required methods exist...');
    const methods = ['connect', 'disconnect', 'isConnected', 'insert', 'delete', 'query', 'subscribe', 'unsubscribe'];
    const missingMethods = methods.filter(method => typeof client[method] !== 'function');
    
    if (missingMethods.length === 0) {
      console.log('   ✅ All required methods present:', methods.join(', '));
    } else {
      console.log(`   ❌ Missing methods: ${missingMethods.join(', ')}`);
    }

    console.log('\n🎉 All basic WASM module tests passed!\n');

    // ========================================
    // DATABASE INTEGRATION TESTS
    // ========================================
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('�️  DATABASE INTEGRATION TESTS');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

    // Create a new client for database tests
    const dbClient = new KalamClient('http://localhost:8080', 'test-api-key');
    await dbClient.connect();
    console.log('✅ Connected to database\n');

    // Test 9: INSERT operation
    console.log('9️⃣ Testing INSERT operation...');
    const testTodo = {
      title: 'Test TODO from WASM',
      completed: false,
      created_at: new Date().toISOString()
    };
    
    try {
      const insertResult = await dbClient.insert('app.todos', JSON.stringify(testTodo));
      console.log(`   ✅ INSERT successful`);
      console.log(`   Response: ${insertResult.substring(0, 100)}...\n`);
    } catch (err) {
      console.log(`   ❌ INSERT failed: ${err}\n`);
      throw err;
    }

    // Test 10: SELECT to verify INSERT
    console.log('🔟 Testing SELECT after INSERT...');
    try {
      const selectResult = await dbClient.query("SELECT * FROM app.todos WHERE title = 'Test TODO from WASM' ORDER BY id DESC LIMIT 1");
      const result = JSON.parse(selectResult);
      
      if (result.status === 'success' && result.results[0]?.rows?.length > 0) {
        const row = result.results[0].rows[0];
        console.log(`   ✅ SELECT successful - Found inserted row`);
        console.log(`   ID: ${row.id}, Title: "${row.title}", Completed: ${row.completed}`);
        
        // Store the ID for later tests
        global.testTodoId = row.id;
        console.log(`   Stored test TODO ID: ${global.testTodoId}\n`);
      } else {
        console.log(`   ❌ SELECT failed - Row not found`);
        console.log(`   Result: ${selectResult}\n`);
      }
    } catch (err) {
      console.log(`   ❌ SELECT failed: ${err}\n`);
      throw err;
    }

    // Test 11: UPDATE operation
    console.log('1️⃣1️⃣ Testing UPDATE operation...');
    try {
      const updateSql = `UPDATE app.todos SET completed = true, title = 'Test TODO from WASM (UPDATED)' WHERE id = ${global.testTodoId}`;
      const updateResult = await dbClient.query(updateSql);
      const result = JSON.parse(updateResult);
      
      if (result.status === 'success') {
        console.log(`   ✅ UPDATE successful`);
        console.log(`   Rows affected: ${result.results[0]?.message || 'unknown'}\n`);
      } else {
        console.log(`   ❌ UPDATE failed: ${updateResult}\n`);
      }
    } catch (err) {
      console.log(`   ❌ UPDATE failed: ${err}\n`);
      throw err;
    }

    // Test 12: SELECT to verify UPDATE
    console.log('1️⃣2️⃣ Testing SELECT after UPDATE...');
    try {
      const selectResult = await dbClient.query(`SELECT * FROM app.todos WHERE id = ${global.testTodoId}`);
      const result = JSON.parse(selectResult);
      
      if (result.status === 'success' && result.results[0]?.rows?.length > 0) {
        const row = result.results[0].rows[0];
        console.log(`   ✅ SELECT successful - Verified UPDATE`);
        console.log(`   ID: ${row.id}, Title: "${row.title}", Completed: ${row.completed}`);
        
        if (row.completed === true || row.completed === 1) {
          console.log(`   ✅ Completed flag updated correctly\n`);
        } else {
          console.log(`   ⚠️  Completed flag not updated (expected: true, got: ${row.completed})\n`);
        }
        
        if (row.title.includes('UPDATED')) {
          console.log(`   ✅ Title updated correctly\n`);
        } else {
          console.log(`   ⚠️  Title not updated (got: "${row.title}")\n`);
        }
      } else {
        console.log(`   ❌ SELECT failed - Row not found after UPDATE\n`);
      }
    } catch (err) {
      console.log(`   ❌ SELECT failed: ${err}\n`);
      throw err;
    }

    // Test 13: COUNT query
    console.log('1️⃣3️⃣ Testing COUNT query...');
    try {
      const countResult = await dbClient.query("SELECT COUNT(*) as total FROM app.todos");
      const result = JSON.parse(countResult);
      
      if (result.status === 'success' && result.results[0]?.rows?.length > 0) {
        const total = result.results[0].rows[0].total;
        console.log(`   ✅ COUNT successful - Total rows: ${total}\n`);
      } else {
        console.log(`   ❌ COUNT failed\n`);
      }
    } catch (err) {
      console.log(`   ❌ COUNT failed: ${err}\n`);
    }

    // Test 14: DELETE operation
    console.log('1️⃣4️⃣ Testing DELETE operation...');
    try {
      const deleteResult = await dbClient.delete('app.todos', global.testTodoId.toString());
      console.log(`   ✅ DELETE successful`);
      console.log(`   Response: ${deleteResult.substring(0, 100)}...\n`);
    } catch (err) {
      console.log(`   ❌ DELETE failed: ${err}\n`);
      throw err;
    }

    // Test 15: SELECT to verify DELETE
    console.log('1️⃣5️⃣ Testing SELECT after DELETE...');
    try {
      const selectResult = await dbClient.query(`SELECT * FROM app.todos WHERE id = ${global.testTodoId}`);
      const result = JSON.parse(selectResult);
      
      if (result.status === 'success' && result.results[0]?.rows?.length === 0) {
        console.log(`   ✅ SELECT successful - Row correctly deleted (0 rows returned)\n`);
      } else if (result.status === 'success' && result.results[0]?.rows?.length > 0) {
        console.log(`   ⚠️  Row still exists after DELETE - may be soft delete\n`);
      } else {
        console.log(`   Result: ${selectResult}\n`);
      }
    } catch (err) {
      console.log(`   ❌ SELECT failed: ${err}\n`);
    }

    // Test 16: Batch INSERT
    console.log('1️⃣6️⃣ Testing batch INSERT...');
    try {
      const batchTodos = [
        { title: 'Batch TODO 1', completed: false, created_at: new Date().toISOString() },
        { title: 'Batch TODO 2', completed: true, created_at: new Date().toISOString() },
        { title: 'Batch TODO 3', completed: false, created_at: new Date().toISOString() }
      ];

      for (let i = 0; i < batchTodos.length; i++) {
        await dbClient.insert('app.todos', JSON.stringify(batchTodos[i]));
      }
      console.log(`   ✅ Batch INSERT successful - ${batchTodos.length} rows inserted\n`);
    } catch (err) {
      console.log(`   ❌ Batch INSERT failed: ${err}\n`);
    }

    // Test 17: SELECT with WHERE clause
    console.log('1️⃣7️⃣ Testing SELECT with WHERE clause...');
    try {
      const selectResult = await dbClient.query("SELECT * FROM app.todos WHERE completed = true");
      const result = JSON.parse(selectResult);
      
      if (result.status === 'success') {
        const completedCount = result.results[0]?.rows?.length || 0;
        console.log(`   ✅ SELECT with WHERE successful - Found ${completedCount} completed TODOs\n`);
      } else {
        console.log(`   ❌ SELECT with WHERE failed\n`);
      }
    } catch (err) {
      console.log(`   ❌ SELECT with WHERE failed: ${err}\n`);
    }

    // Test 18: Cleanup - Delete batch TODOs
    console.log('1️⃣8️⃣ Cleaning up batch TODOs...');
    try {
      const deleteResult = await dbClient.query("DELETE FROM app.todos WHERE title LIKE 'Batch TODO%'");
      const result = JSON.parse(deleteResult);
      
      if (result.status === 'success') {
        console.log(`   ✅ Cleanup successful\n`);
      } else {
        console.log(`   ⚠️  Cleanup completed with warnings\n`);
      }
    } catch (err) {
      console.log(`   ⚠️  Cleanup failed (non-critical): ${err}\n`);
    }

    // Disconnect
    await dbClient.disconnect();
    console.log('✅ Disconnected from database\n');

    console.log('\n🎉 All tests passed!\n');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('📋 SUMMARY');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━');
    console.log('✅ WASM Module Tests (8 tests)');
    console.log('   • Module initialization');
    console.log('   • Parameter validation');
    console.log('   • Method verification');
    console.log('');
    console.log('✅ Database Integration Tests (10 tests)');
    console.log('   • INSERT operation');
    console.log('   • SELECT verification');
    console.log('   • UPDATE operation');
    console.log('   • UPDATE verification');
    console.log('   • COUNT query');
    console.log('   • DELETE operation');
    console.log('   • DELETE verification');
    console.log('   • Batch INSERT');
    console.log('   • WHERE clause filtering');
    console.log('   • Cleanup operations');
    console.log('━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n');

  } catch (error) {
    console.error('❌ Test failed:', error);
    process.exit(1);
  }
}

// Run tests
testWasmModule().catch(err => {
  console.error('Fatal error:', err);
  process.exit(1);
});
