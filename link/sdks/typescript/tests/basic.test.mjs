#!/usr/bin/env node

/**
 * Basic WASM Module Test
 * 
 * Tests that the WASM module loads and initializes correctly
 */

import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { readFile } from 'fs/promises';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Import from parent directory (the SDK root)
const sdkPath = join(__dirname, '..');

async function runTests() {
  console.log('🧪 Running Basic WASM Module Tests\n');
  
  let passed = 0;
  let failed = 0;

  // Test 1: Module loads
  console.log('Test 1: WASM module loads...');
  try {
    const { default: init, KalamClient } = await import(join(sdkPath, 'kalam_link.js'));
    
    if (typeof init === 'function') {
      console.log('  ✓ init function exists');
      passed++;
    } else {
      console.log('  ✗ init is not a function');
      failed++;
    }

    if (typeof KalamClient === 'function') {
      console.log('  ✓ KalamClient class exists');
      passed++;
    } else {
      console.log('  ✗ KalamClient is not a constructor');
      failed++;
    }
  } catch (error) {
    console.log('  ✗ Failed to load module:', error.message);
    failed += 2;
  }

  // Test 2: WASM initialization
  console.log('\nTest 2: WASM initializes...');
  try {
    const { default: init } = await import(join(sdkPath, 'kalam_link.js'));
    
    // For Node.js, we need to pass the WASM file path explicitly
    const wasmPath = join(sdkPath, 'kalam_link_bg.wasm');
    const wasmBuffer = await readFile(wasmPath);
    
    await init(wasmBuffer);
    console.log('  ✓ WASM initialized successfully');
    passed++;
  } catch (error) {
    console.log('  ✗ WASM initialization failed:', error.message);
    failed++;
  }

  // Test 3: Client construction
  console.log('\nTest 3: KalamClient construction...');
  try {
    const { KalamClient } = await import(join(sdkPath, 'kalam_link.js'));
    
    const client = new KalamClient('ws://localhost:8080', 'test-api-key');
    
    if (client) {
      console.log('  ✓ KalamClient instance created');
      passed++;
    } else {
      console.log('  ✗ Failed to create instance');
      failed++;
    }

    // Test isConnected method exists
    if (typeof client.isConnected === 'function') {
      console.log('  ✓ isConnected method exists');
      passed++;
    } else {
      console.log('  ✗ isConnected method missing');
      failed++;
    }

    // Test connect method exists
    if (typeof client.connect === 'function') {
      console.log('  ✓ connect method exists');
      passed++;
    } else {
      console.log('  ✗ connect method missing');
      failed++;
    }

    // Test disconnect method exists
    if (typeof client.disconnect === 'function') {
      console.log('  ✓ disconnect method exists');
      passed++;
    } else {
      console.log('  ✗ disconnect method missing');
      failed++;
    }

    // Test query method exists
    if (typeof client.query === 'function') {
      console.log('  ✓ query method exists');
      passed++;
    } else {
      console.log('  ✗ query method missing');
      failed++;
    }

    // Test insert method exists
    if (typeof client.insert === 'function') {
      console.log('  ✓ insert method exists');
      passed++;
    } else {
      console.log('  ✗ insert method missing');
      failed++;
    }

    // Test delete method exists
    if (typeof client.delete === 'function') {
      console.log('  ✓ delete method exists');
      passed++;
    } else {
      console.log('  ✗ delete method missing');
      failed++;
    }

    // Test subscribe method exists
    if (typeof client.subscribe === 'function') {
      console.log('  ✓ subscribe method exists');
      passed++;
    } else {
      console.log('  ✗ subscribe method missing');
      failed++;
    }

    // Test unsubscribe method exists
    if (typeof client.unsubscribe === 'function') {
      console.log('  ✓ unsubscribe method exists');
      passed++;
    } else {
      console.log('  ✗ unsubscribe method missing');
      failed++;
    }

  } catch (error) {
    console.log('  ✗ Client construction failed:', error.message);
    failed += 9;
  }

  // Test 4: Required parameters validation
  console.log('\nTest 4: Constructor parameter validation...');
  try {
    const { KalamClient } = await import(join(sdkPath, 'kalam_link.js'));
    
    // Should throw without URL
    try {
      new KalamClient();
      console.log('  ✗ Missing URL should throw error');
      failed++;
    } catch (error) {
      console.log('  ✓ Missing URL throws error');
      passed++;
    }

    // Should throw with only URL
    try {
      new KalamClient('ws://localhost:8080');
      console.log('  ✗ Missing API key should throw error');
      failed++;
    } catch (error) {
      console.log('  ✓ Missing API key throws error');
      passed++;
    }

  } catch (error) {
    console.log('  ✗ Parameter validation test failed:', error.message);
    failed += 2;
  }

  // Results
  console.log('\n' + '='.repeat(50));
  console.log(`Results: ${passed} passed, ${failed} failed`);
  console.log('='.repeat(50));

  if (failed === 0) {
    console.log('\n✅ All tests passed!');
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
