/**
 * Example: Basic KalamDB TypeScript SDK Usage (HTTP Only)
 * 
 * This example demonstrates HTTP-only functionality:
 * - Initializing the client
 * - Creating a namespace and table
 * - Inserting and querying data
 * 
 * Note: WebSocket subscriptions are not tested in this example
 * due to browser WebSocket API limitations with authentication.
 */

import { KalamDBClient } from './dist/index.js';

async function main() {
  // Configuration
  const SERVER_URL = process.env.KALAMDB_URL || 'http://localhost:8080';
  const USERNAME = process.env.KALAMDB_USER || 'root';
  const PASSWORD = process.env.KALAMDB_PASSWORD || 'root';

  console.log('🚀 KalamDB TypeScript SDK Example (HTTP)\n');
  console.log(`Server: ${SERVER_URL}`);
  console.log(`User: ${USERNAME}\n`);

  // Create client
  const client = new KalamDBClient(SERVER_URL, USERNAME, PASSWORD);

  try {
    // Initialize WASM (no WebSocket connection)
    console.log('📦 Initializing WASM...');
    await client.initialize();
    console.log('✅ WASM initialized\n');

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

    // Update a todo
    console.log('📝 Updating a todo...');
    const firstTodo = result.results[0].rows[0];
    await client.query(`
      UPDATE example.todos 
      SET completed = true 
      WHERE id = ${firstTodo.id}
    `);
    console.log('✅ Todo updated\n');

    // Query again to see the update
    console.log('🔍 Querying updated todos...');
    const updatedResult = await client.query(`
      SELECT * FROM example.todos 
      ORDER BY priority DESC, created_at ASC
    `);

    console.log(`Found ${updatedResult.results[0].row_count} todos:\n`);
    if (updatedResult.results[0].rows) {
      updatedResult.results[0].rows.forEach((todo, index) => {
        const status = todo.completed ? '✓' : '○';
        const priority = todo.priority.toUpperCase().padEnd(6);
        console.log(`  ${index + 1}. [${status}] ${priority} ${todo.title}`);
      });
    }
    console.log('');

    // Delete a todo
    console.log('🗑️  Deleting a todo...');
    await client.delete('example.todos', firstTodo.id);
    console.log('✅ Todo deleted\n');

    // Final query
    console.log('🔍 Final query...');
    const finalResult = await client.query('SELECT COUNT(*) as count FROM example.todos');
    console.log(`Remaining todos: ${finalResult.results[0].rows[0].count}\n`);

    console.log('🎉 Example completed successfully!');
    console.log('\n📝 Note: WebSocket subscriptions require additional backend setup');
    console.log('   for authentication with query parameters or protocol headers.');
    
  } catch (error) {
    console.error('\n❌ Error:', error);
    if (error.message && error.message.includes('401')) {
      console.error('\n💡 Hint: Check your username and password');
    }
    process.exit(1);
  }
}

// Run the example
main();
