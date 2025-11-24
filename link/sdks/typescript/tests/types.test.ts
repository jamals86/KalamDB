/**
 * Type safety tests for KalamDB TypeScript SDK
 * Ensures TypeScript definitions are correct
 * Run with: npx tsc --noEmit tests/types.test.ts
 */

import { KalamDBClient, QueryResponse, ServerMessage, createClient } from '../src/index';

// Test: Constructor types
const client1 = new KalamDBClient('http://localhost:8080', 'user', 'pass');

// Test: Factory function
const client2 = createClient({
  url: 'http://localhost:8080',
  username: 'user',
  password: 'pass'
});

// Test: Async methods return promises
async function testMethods() {
  const client = new KalamDBClient('http://localhost:8080', 'user', 'pass');
  
  // Connection methods
  const connectResult: void = await client.connect();
  const disconnectResult: void = await client.disconnect();
  const isConnected: boolean = client.isConnected();
  
  // Query methods
  const queryResult: QueryResponse = await client.query('SELECT 1');
  const insertResult: QueryResponse = await client.insert('table', { id: 1 });
  const deleteResult: void = await client.delete('table', '123');
  
  // Subscription methods
  const subId: string = await client.subscribe('table', (event: ServerMessage) => {
    // Type check event
    if (event.type === 'change') {
      const changeType: 'insert' | 'update' | 'delete' = event.change_type;
      const rows: Record<string, any>[] | undefined = event.rows;
    } else if (event.type === 'initial_data_batch') {
      const rows: Record<string, any>[] = event.rows;
      const batchControl = event.batch_control;
    } else if (event.type === 'error') {
      const code: string = event.code;
      const message: string = event.message;
    }
  });
  
  const unsubResult: void = await client.unsubscribe(subId);
}

// Test: QueryResponse structure
const response: QueryResponse = {
  status: 'success',
  results: [
    {
      rows: [{ id: 1, name: 'test' }],
      row_count: 1,
      columns: ['id', 'name']
    }
  ],
  took: 15.5
};

// Test: Error response
const errorResponse: QueryResponse = {
  status: 'error',
  results: [],
  error: {
    code: 'ERR_TABLE_NOT_FOUND',
    message: 'Table does not exist'
  }
};

console.log('✅ Type definitions are valid');
