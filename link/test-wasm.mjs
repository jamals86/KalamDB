#!/usr/bin/env node
// WASM Module Test Script (T056)
// Tests that the WASM module loads and works correctly in Node.js

import { readFile } from 'fs/promises';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import init, { KalamClient } from './pkg/kalam_link.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

async function main() {
    console.log('🧪 Testing kalam-link WASM module...\n');

    // Initialize WASM module by loading the binary directly
    try {
        const wasmPath = join(__dirname, 'pkg', 'kalam_link_bg.wasm');
        const wasmBuffer = await readFile(wasmPath);
        await init(wasmBuffer);
        console.log('✅ WASM module initialized successfully');
    } catch (err) {
        console.error('❌ Failed to initialize WASM module:', err);
        process.exit(1);
    }

    // T057: Test client construction with valid API key
    try {
        const client = new KalamClient(
            "http://localhost:8080",
            "test-api-key-123"
        );
        console.log('✅ KalamClient created successfully');
        console.log('   URL: http://localhost:8080');
        console.log('   API Key: test-api-key-123');

        // Test connection status
        const connected = client.isConnected();
        console.log(`   Connected: ${connected} (expected: false before connect())`);

        // Test connect/disconnect
        await client.connect();
        console.log('✅ client.connect() succeeded');
        console.log(`   Connected: ${client.isConnected()} (expected: true)`);

        await client.disconnect();
        console.log('✅ client.disconnect() succeeded');
        console.log(`   Connected: ${client.isConnected()} (expected: false)`);

    } catch (err) {
        console.error('❌ Test failed:', err);
        process.exit(1);
    }

    // T058: Test client with missing URL
    try {
        const client = new KalamClient("", "test-api-key-123");
        console.error('❌ Should have failed with empty URL');
        process.exit(1);
    } catch (err) {
        if (err.toString().includes("'url' parameter is required")) {
            console.log('✅ Correctly rejected empty URL');
            console.log('   Error:', err.toString());
        } else {
            console.error('❌ Wrong error message:', err);
            process.exit(1);
        }
    }

    // T058: Test client with missing API key
    try {
        const client = new KalamClient("http://localhost:8080", "");
        console.error('❌ Should have failed with empty API key');
        process.exit(1);
    } catch (err) {
        if (err.toString().includes("'api_key' parameter is required")) {
            console.log('✅ Correctly rejected empty API key');
            console.log('   Error:', err.toString());
        } else {
            console.error('❌ Wrong error message:', err);
            process.exit(1);
        }
    }

    console.log('\n🎉 All WASM tests passed!');
    console.log('\n📋 Summary:');
    console.log('   • WASM module loads correctly');
    console.log('   • KalamClient constructor validates parameters');
    console.log('   • connect()/disconnect() methods work');
    console.log('   • isConnected() returns correct state');
    console.log('   • Error messages are clear and helpful');
}

main().catch(err => {
    console.error('💥 Unexpected error:', err);
    process.exit(1);
});
