/**
 * Example: Basic KalamDB TypeScript SDK Usage
 * 
 * This example demonstrates:
 * - Connecting to KalamDB
 * - Creating a namespace and table
 * - Inserting and querying data
 * - Real-time subscriptions
 */

import { KalamDBClient } from './dist/index.js';

async function main() {
  // Configuration
  const SERVER_URL = process.env.KALAMDB_URL || 'http://localhost:8080';
  const USERNAME = process.env.KALAMDB_USER || 'root';
  const PASSWORD = process.env.KALAMDB_PASSWORD || 'root';

  console.log('🚀 KalamDB TypeScript SDK Example\n');
  console.log(`Connecting to: ${SERVER_URL}`);
  console.log(`User: ${USERNAME}\n`);

  // Create client
  const client = new KalamDBClient(SERVER_URL, USERNAME, PASSWORD);

  try {
    // Connect to server
    console.log('📡 Connecting...');
    await client.connect();
    console.log('✅ Connected!\n');

    // Create namespace
    console.log('📦 Creating namespace...');
    await client.query('CREATE NAMESPACE IF NOT EXISTS example');
    console.log('✅ Namespace created\n');

    // Create table
    console.log('📋 Creating table...');
    await client.query(`
      CREATE TABLE IF NOT EXISTS example.todos (
        id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
        title TEXT NOT NULL,
        completed BOOLEAN DEFAULT false,
        priority TEXT DEFAULT 'medium',
        created_at TIMESTAMP DEFAULT NOW()
      ) WITH (TYPE='SHARED', FLUSH_POLICY='rows:100')
    `);
    console.log('✅ Table created\n');

    // Insert some data
    console.log('➕ Inserting todos...');
    await client.insert('example.todos', {
      title: 'Buy groceries',
      priority: 'high',
      completed: false
    });
    await client.insert('example.todos', {
      title: 'Write documentation',
      priority: 'medium',
      completed: true
    });
    await client.insert('example.todos', {
      title: 'Review pull requests',
      priority: 'high',
      completed: false
    });
    console.log('✅ Inserted 3 todos\n');

    // Query data
    console.log('🔍 Querying todos...');
    const result = await client.query(`
      SELECT * FROM example.todos 
      ORDER BY priority DESC, created_at ASC
    `);

    console.log(`Found ${result.results[0].row_count} todos:\n`);
    if (result.results[0].rows) {
      result.results[0].rows.forEach((todo, index) => {
        const status = todo.completed ? '✓' : '○';
        const priority = todo.priority.toUpperCase().padEnd(6);
        console.log(`  ${index + 1}. [${status}] ${priority} ${todo.title}`);
      });
    }
    console.log('');

    // Subscribe to changes
    console.log('👂 Subscribing to real-time changes...');
    const subId = await client.subscribe('example.todos', (event) => {
      if (event.type === 'subscription_ack') {
        console.log(`✅ Subscription active (${event.total_rows} total rows)\n`);
      } else if (event.type === 'change' && event.change_type === 'insert' && event.rows) {
        console.log('🆕 New todo added:');
        event.rows.forEach(row => {
          console.log(`   - ${row.title} [${row.priority}]`);
        });
      } else if (event.type === 'change' && event.change_type === 'update' && event.rows) {
        console.log('📝 Todo updated:');
        event.rows.forEach(row => {
          console.log(`   - ${row.title} [${row.completed ? 'DONE' : 'PENDING'}]`);
        });
      } else if (event.type === 'error') {
        console.error(`❌ Subscription error: ${event.message}`);
      }
    });

    // Wait a bit for subscription to establish
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Insert a new todo to trigger change event
    console.log('➕ Adding a new todo (will trigger subscription event)...');
    await client.insert('example.todos', {
      title: 'Test real-time subscription',
      priority: 'low',
      completed: false
    });

    // Wait for event to be received
    await new Promise(resolve => setTimeout(resolve, 2000));

    // Update a todo
    console.log('\n📝 Updating a todo (will trigger subscription event)...');
    const firstTodo = result.results[0].rows[0];
    await client.query(`
      UPDATE example.todos 
      SET completed = true 
      WHERE id = ${firstTodo.id}
    `);

    // Wait for event
    await new Promise(resolve => setTimeout(resolve, 2000));

    // Unsubscribe
    console.log('\n👋 Unsubscribing...');
    await client.unsubscribe(subId);
    console.log('✅ Unsubscribed\n');

    // Disconnect
    console.log('🔌 Disconnecting...');
    await client.disconnect();
    console.log('✅ Disconnected\n');

    console.log('🎉 Example completed successfully!');
    
  } catch (error) {
    console.error('\n❌ Error:', error);
    process.exit(1);
  }
}

// Run the example
main();
