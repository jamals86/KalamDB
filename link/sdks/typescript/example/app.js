/**
 * KalamDB Browser Example Application
 * 
 * Uses the KalamDB WASM client from parent directory
 */

import { KalamDBClient } from '/dist/index.js';

/** @type {KalamDBClient | null} */
let client = null;

/** @type {(() => Promise<void>) | null} */
let unsubscribeTodos = null;

/** @type {(() => Promise<void>) | null} */
let unsubscribeEvents = null;

/** @type {number | null} */
let healthCheckInterval = null;

/** @type {number | null} */
let statusBarInterval = null;

/**
 * Update the subscription status bar at the bottom of the page
 */
function updateSubscriptionBar() {
  const countEl = document.getElementById('subCount');
  const listEl = document.getElementById('subList');
  const wsIndicator = document.getElementById('wsIndicator');
  const wsStatus = document.getElementById('wsStatus');
  
  // Update WebSocket status
  const isConnected = client?.isConnected() ?? false;
  if (isConnected) {
    wsIndicator.classList.add('connected');
    wsStatus.textContent = 'WebSocket: Connected';
  } else {
    wsIndicator.classList.remove('connected');
    wsStatus.textContent = 'WebSocket: Disconnected';
  }
  
  // Get subscription info from client
  const count = client?.getSubscriptionCount() ?? 0;
  const subscriptions = client?.getSubscriptions() ?? [];
  
  // Update count
  countEl.textContent = `${count} Active Subscription${count !== 1 ? 's' : ''}`;
  countEl.className = count > 0 ? 'count' : 'count zero';
  
  // Update list
  if (subscriptions.length === 0) {
    listEl.innerHTML = '<span style="color: #808080; font-style: italic;">No active subscriptions</span>';
  } else {
    listEl.innerHTML = subscriptions.map(sub => {
      // Truncate long SQL queries
      const displayName = sub.tableName.length > 50 
        ? sub.tableName.substring(0, 47) + '...' 
        : sub.tableName;
      return `<div class="sub-item" title="${sub.tableName}">
        <span class="sub-id">${sub.id.substring(0, 12)}...</span> ${displayName}
      </div>`;
    }).join('');
  }
}

// Start status bar updates
function startStatusBarUpdates() {
  if (statusBarInterval) {
    clearInterval(statusBarInterval);
  }
  // Update every 500ms
  statusBarInterval = setInterval(updateSubscriptionBar, 500);
  // Initial update
  updateSubscriptionBar();
}

// Start health check monitoring
async function startHealthCheck() {
  const config = getConfig();
  const checkHealth = async () => {
    try {
      const response = await fetch(`${config.url}/v1/api/healthcheck`, {
        method: 'GET',
        headers: {
          'Accept': 'application/json'
        }
      });
      
      if (response.ok) {
        const data = await response.json();
        updateStatus(true, data.status || 'healthy');
      } else {
        updateStatus(false, 'unhealthy');
      }
    } catch (error) {
      updateStatus(false, 'offline');
    }
  };

  // Initial check
  await checkHealth();
  
  // Check every 5 seconds
  if (healthCheckInterval) {
    clearInterval(healthCheckInterval);
  }
  healthCheckInterval = setInterval(checkHealth, 3000);
}

// Start health check and auto-initialize on page load
window.addEventListener('load', async () => {
  await startHealthCheck();
  startStatusBarUpdates();
  await testInit();
});

/**
 * Get timestamp with milliseconds
 */
function getTimestamp() {
  const now = new Date();
  const time = now.toLocaleTimeString('en-US', { hour12: false });
  const ms = now.getMilliseconds().toString().padStart(3, '0');
  return `${time}.${ms}`;
}

/**
 * Simple logger - shows raw messages with timestamps
 */
function log(message, type = 'info') {
  const output = document.getElementById('output');
  const timestamp = getTimestamp();
  const className = `log-${type}`;
  
  // Escape HTML in message
  const escaped = String(message)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
  
  output.innerHTML += `<span class="${className}">[${timestamp}] ${escaped}</span>\n`;
  output.scrollTop = output.scrollHeight;
}

/**
 * Log JSON data with expand/collapse
 */
function logJson(label, data, type = 'info') {
  const output = document.getElementById('output');
  const timestamp = getTimestamp();
  const className = `log-${type}`;
  const id = `json-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  
  const jsonStr = JSON.stringify(data, null, 2);
  const preview = jsonStr.split('\n').slice(0, 3).join('\n');
  const lines = jsonStr.split('\n').length;
  const needsCollapse = lines > 3;
  
  output.innerHTML += `<span class="${className}">[${timestamp}] ${label}: <span class="json-collapsible" onclick="toggleJson('${id}')"><span class="json-content${needsCollapse ? '' : ' expanded'}" id="${id}" data-full="${encodeURIComponent(jsonStr)}">${needsCollapse ? preview + '\n...' : jsonStr}</span>${needsCollapse ? '<span class="json-toggle">[expand]</span>' : ''}</span></span>\n`;
  output.scrollTop = output.scrollHeight;
}

function updateStatus(online, statusText = null) {
  const status = document.getElementById('status');
  if (online) {
    status.textContent = statusText ? `Server Online (${statusText})` : 'Server Online';
    status.className = 'status connected';
  } else {
    status.textContent = statusText ? `Server Offline (${statusText})` : 'Server Offline';
    status.className = 'status disconnected';
  }
}

function getConfig() {
  return {
    url: document.getElementById('serverUrl').value,
    username: document.getElementById('username').value,
    password: document.getElementById('password').value,
  };
}

window.testInit = async function() {
  try {
    log('🔧 Initializing WASM and creating client...', 'info');
    const config = getConfig();
    
    client = new KalamDBClient(config.url, config.username, config.password);
    await client.initialize();
    
    log('✅ WASM initialized successfully!', 'success');
    log(`   Server: ${config.url}`, 'info');
    log(`   User: ${config.username}`, 'info');
  } catch (error) {
    log(`❌ Initialization failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.testQuery = async function() {
  if (!client) {
    log('⚠️  Please initialize first!', 'warning');
    return;
  }

  try {
    const sql = "SELECT 1 as number, 'hello' as text";
    log(`→ SQL: ${sql}`, 'info');
    const result = await client.query(sql);
    logJson('← Result', result, 'success');
  } catch (error) {
    log(`❌ Query failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.testCreateNamespace = async function() {
  if (!client) {
    log('⚠️  Please initialize first!', 'warning');
    return;
  }

  try {
    const sql = 'CREATE NAMESPACE IF NOT EXISTS test_browser';
    log(`→ SQL: ${sql}`, 'info');
    const result = await client.query(sql);
    logJson('← Result', result, 'success');
  } catch (error) {
    log(`❌ Failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.testCreateTable = async function() {
  if (!client) {
    log('⚠️  Please initialize first!', 'warning');
    return;
  }

  try {
    const sql = `CREATE TABLE IF NOT EXISTS test_browser.todos (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  title TEXT NOT NULL,
  completed BOOLEAN DEFAULT false,
  priority TEXT DEFAULT 'medium',
  created_at TIMESTAMP DEFAULT NOW()
) WITH (TYPE='USER', FLUSH_POLICY='rows:100')`;
    log(`→ SQL: ${sql}`, 'info');
    const result = await client.query(sql);
    logJson('← Result', result, 'success');
  } catch (error) {
    log(`❌ Failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.testInsert = async function() {
  if (!client) {
    log('⚠️  Please initialize first!', 'warning');
    return;
  }

  try {
    const todos = [
      { title: 'Test from browser', priority: 'high', completed: false },
      { title: 'TypeScript SDK works!', priority: 'medium', completed: true },
      { title: 'WASM is awesome', priority: 'high', completed: false }
    ];
    
    for (const todo of todos) {
      log(`→ INSERT test_browser.todos: ${JSON.stringify(todo)}`, 'info');
      const result = await client.insert('test_browser.todos', todo);
      logJson('← Result', result, 'success');
    }
  } catch (error) {
    log(`❌ Failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.testSelect = async function() {
  if (!client) {
    log('⚠️  Please initialize first!', 'warning');
    return;
  }

  try {
    const sql = `SELECT * FROM test_browser.todos ORDER BY priority DESC, created_at ASC`;
    log(`→ SQL: ${sql}`, 'info');
    const result = await client.query(sql);
    logJson('← Result', result, 'success');
  } catch (error) {
    log(`❌ Failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.runAll = async function() {
  clearOutput();
  log('🚀 Running all tests...', 'info');
  log('═'.repeat(60), 'info');
  
  await testInit();
  await new Promise(r => setTimeout(r, 500));
  
  await testQuery();
  await new Promise(r => setTimeout(r, 500));
  
  await testCreateNamespace();
  await new Promise(r => setTimeout(r, 500));
  
  await testCreateTable();
  await new Promise(r => setTimeout(r, 500));
  
  await testInsert();
  await new Promise(r => setTimeout(r, 500));
  
  await testSelect();
  
  log('═'.repeat(60), 'info');
  log('🎉 All tests completed!', 'success');
};

window.testSubscribe = async function() {
  if (!client) {
    log('⚠️  Please initialize first!', 'warning');
    return;
  }

  // Guard: prevent duplicate subscriptions
  if (unsubscribeTodos) {
    log('⚠️  Already subscribed to todos! Unsubscribe first.', 'warning');
    return;
  }

  try {
    // Connect to WebSocket if not already connected
    log('🔌 Connecting to server for subscriptions...', 'info');
    await client.connect();
    log('✅ Connected to server!', 'success');
    
    log('📡 Subscribing to test_browser.todos with options...', 'info');
    
    // Make sure table exists first
    await client.query('CREATE TABLE IF NOT EXISTS test_browser.todos (id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(), title TEXT NOT NULL, completed BOOLEAN DEFAULT false, priority TEXT DEFAULT \'medium\', created_at TIMESTAMP DEFAULT NOW()) WITH (TYPE=\'USER\', FLUSH_POLICY=\'rows:100\')');
    
    // Use subscribeWithSql for filtered query with options
    // Returns an unsubscribe function (Firebase/Supabase style)
    unsubscribeTodos = await client.subscribeWithSql(
      'SELECT * FROM test_browser.todos ORDER BY created_at DESC',
      (data) => {
        logJson('← [TODOS] WebSocket message', data, 'success');
        updateSubscriptionBar();
      },
      { batch_size: 50 }  // Load initial data in batches of 50
    );
    
    log(`✅ Subscribed to test_browser.todos`, 'success');
    log(`   Active subscriptions: ${client.getSubscriptionCount()}`, 'info');
    updateSubscriptionBar();
    
    // Update button states
    document.getElementById('subscribeBtn').disabled = true;
    document.getElementById('unsubscribeBtn').disabled = false;
    document.getElementById('insertLiveBtn').disabled = false;
    updateUnsubscribeAllButton();
    
  } catch (error) {
    log(`❌ Subscription failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.testUnsubscribe = async function() {
  if (!client) {
    log('⚠️  Please initialize first!', 'warning');
    return;
  }
  
  if (!unsubscribeTodos) {
    log('⚠️  No active subscription!', 'warning');
    return;
  }

  try {
    log(`📡 Unsubscribing from todos...`, 'info');
    
    // Call the unsubscribe function
    const unsub = unsubscribeTodos;
    unsubscribeTodos = null;  // Clear first to prevent re-entry
    await unsub();
    
    log('✅ Unsubscribed successfully!', 'success');
    updateSubscriptionBar();
    
    // Update button states
    document.getElementById('subscribeBtn').disabled = false;
    document.getElementById('unsubscribeBtn').disabled = true;
    document.getElementById('insertLiveBtn').disabled = true;
    updateUnsubscribeAllButton();
    
    // Clear subscription output
    document.getElementById('subscriptionOutput').innerHTML = '<div style="color: #808080; font-style: italic; font-size: 11px;">Unsubscribed. Click "Subscribe to Todos" to start again.</div>';
    
  } catch (error) {
    log(`❌ Unsubscribe failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.testInsertLive = async function() {
  if (!client) {
    log('⚠️  Please initialize first!', 'warning');
    return;
  }

  if (!unsubscribeTodos) {
    log('⚠️  Please subscribe first!', 'warning');
    return;
  }

  try {
    const priorities = ['low', 'medium', 'high'];
    const tasks = [
      'Review pull request',
      'Update documentation',
      'Fix critical bug',
      'Deploy to production',
      'Write unit tests',
      'Refactor authentication',
      'Optimize database queries',
      'Design new API endpoint'
    ];
    
    const randomTask = tasks[Math.floor(Math.random() * tasks.length)];
    const randomPriority = priorities[Math.floor(Math.random() * priorities.length)];
    const randomCompleted = Math.random() > 0.7;
    
    const data = { title: randomTask, priority: randomPriority, completed: randomCompleted };
    log(`→ INSERT test_browser.todos: ${JSON.stringify(data)}`, 'info');
    const result = await client.insert('test_browser.todos', data);
    logJson('← Result', result, 'success');
    
  } catch (error) {
    log(`❌ Insert failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.testCreateAndSubscribeTodos = async function() {
  if (!client) {
    log('⚠️  Please initialize first!', 'warning');
    return;
  }

  // Guard: prevent duplicate subscriptions
  if (unsubscribeTodos) {
    log('⚠️  Already subscribed to todos! Unsubscribe first.', 'warning');
    return;
  }

  try {
    // Connect to WebSocket if not already connected
    log('→ Connecting to WebSocket...', 'info');
    await client.connect();
    log('✅ WebSocket connected', 'success');
    
    // Create table
    const sql = `CREATE TABLE IF NOT EXISTS test_browser.todos (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(), 
  title TEXT NOT NULL, 
  completed BOOLEAN DEFAULT false, 
  priority TEXT DEFAULT 'medium', 
  created_at TIMESTAMP DEFAULT NOW()
) WITH (TYPE='USER', FLUSH_POLICY='rows:100')`;
    log(`→ SQL: ${sql}`, 'info');
    const result = await client.query(sql);
    logJson('← Result', result, 'success');
    
    // Subscribe with SQL query and options
    log('→ SUBSCRIBE test_browser.todos (with batch_size: 100)', 'info');
    unsubscribeTodos = await client.subscribeWithSql(
      'SELECT * FROM test_browser.todos ORDER BY created_at DESC',
      (data) => {
        logJson('← [TODOS] WebSocket message', data, 'success');
        updateSubscriptionBar();
      },
      { batch_size: 100 }
    );
    
    log(`✅ Subscribed to todos`, 'success');
    log(`   Active subscriptions: ${client.getSubscriptionCount()}`, 'info');
    updateSubscriptionBar();
    
    // Update button states
    document.getElementById('subscribeBtn').disabled = true;
    document.getElementById('unsubscribeBtn').disabled = false;
    document.getElementById('insertLiveBtn').disabled = false;
    updateUnsubscribeAllButton();
    
  } catch (error) {
    log(`❌ Failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.testCreateAndSubscribeEvents = async function() {
  if (!client) {
    log('⚠️  Please initialize first!', 'warning');
    return;
  }

  // Guard: prevent duplicate subscriptions
  if (unsubscribeEvents) {
    log('⚠️  Already subscribed to events! Unsubscribe first.', 'warning');
    return;
  }

  try {
    // Connect to WebSocket if not already connected
    if (!client.isConnected()) {
      log('→ Connecting to WebSocket...', 'info');
      await client.connect();
      log('✅ WebSocket connected', 'success');
    }
    
    // Create STREAM table (requires TTL_SECONDS for stream tables)
    const sql = `CREATE TABLE IF NOT EXISTS test_browser.events (
  id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(), 
  event_type TEXT NOT NULL, 
  user_id TEXT, 
  payload TEXT, 
  timestamp TIMESTAMP DEFAULT NOW()
) WITH (TYPE='STREAM', TTL_SECONDS=3600)`;
    log(`→ SQL: ${sql}`, 'info');
    const result = await client.query(sql);
    logJson('← Result', result, 'success');
    
    // Subscribe to stream with SQL query and options
    log('→ SUBSCRIBE test_browser.events (with batch_size: 25)', 'info');
    unsubscribeEvents = await client.subscribeWithSql(
      'SELECT * FROM test_browser.events ORDER BY timestamp DESC',
      (data) => {
        logJson('← [EVENTS] WebSocket message', data, 'success');
        updateSubscriptionBar();
      },
      { batch_size: 25 }
    );
    
    log(`✅ Subscribed to events`, 'success');
    log(`   Active subscriptions: ${client.getSubscriptionCount()}`, 'info');
    updateSubscriptionBar();
    
    // Update button states
    document.getElementById('createSubEventsBtn').disabled = true;
    document.getElementById('insertEventBtn').disabled = false;
    updateUnsubscribeAllButton();
    
  } catch (error) {
    log(`❌ Failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.testInsertEvent = async function() {
  if (!client) {
    log('⚠️  Please initialize first!', 'warning');
    return;
  }

  if (!unsubscribeEvents) {
    log('⚠️  Please subscribe to events first!', 'warning');
    return;
  }

  try {
    const eventTypes = ['user_login', 'user_logout', 'page_view', 'button_click', 'api_call', 'error_occurred', 'data_saved'];
    const userIds = ['user_001', 'user_002', 'user_003', 'alice', 'bob', 'charlie'];
    const payloads = [
      '{"page": "/dashboard"}',
      '{"button": "submit"}',
      '{"endpoint": "/api/users"}',
      '{"error": "timeout"}',
      '{"table": "todos"}',
      '{"session": "abc123"}',
      '{"duration": 1234}'
    ];
    
    const randomEventType = eventTypes[Math.floor(Math.random() * eventTypes.length)];
    const randomUserId = userIds[Math.floor(Math.random() * userIds.length)];
    const randomPayload = payloads[Math.floor(Math.random() * payloads.length)];
    
    const data = { event_type: randomEventType, user_id: randomUserId, payload: randomPayload };
    log(`→ INSERT test_browser.events: ${JSON.stringify(data)}`, 'info');
    const result = await client.insert('test_browser.events', data);
    logJson('← Result', result, 'success');
    
  } catch (error) {
    log(`❌ Insert failed: ${error.message}`, 'error');
    console.error(error);
  }
};

window.testUnsubscribeAll = async function() {
  if (!client) {
    log('⚠️  No client initialized!', 'warning');
    return;
  }

  try {
    let unsubscribed = false;
    
    if (unsubscribeTodos) {
      log(`📡 Unsubscribing from todos...`, 'info');
      const unsub = unsubscribeTodos;
      unsubscribeTodos = null; // Clear first
      
      try {
        await unsub();
        log('✅ Unsubscribed from todos!', 'success');
        unsubscribed = true;
      } catch (err) {
        log(`⚠️  Failed to unsubscribe from todos: ${err.message}`, 'warning');
      }
      
      // Update button states
      document.getElementById('subscribeBtn').disabled = false;
      document.getElementById('unsubscribeBtn').disabled = true;
      document.getElementById('insertLiveBtn').disabled = true;
      document.getElementById('subscriptionOutput').innerHTML = '<div style="color: #808080; font-style: italic; font-size: 11px;">No active subscription</div>';
    }
    
    if (unsubscribeEvents) {
      log(`📡 Unsubscribing from events...`, 'info');
      const unsub = unsubscribeEvents;
      unsubscribeEvents = null; // Clear first
      
      try {
        await unsub();
        log('✅ Unsubscribed from events!', 'success');
        unsubscribed = true;
      } catch (err) {
        log(`⚠️  Failed to unsubscribe from events: ${err.message}`, 'warning');
      }
      
      // Update button states
      document.getElementById('createSubEventsBtn').disabled = false;
      document.getElementById('insertEventBtn').disabled = true;
    }
    
    if (!unsubscribed) {
      log('⚠️  No active subscriptions!', 'warning');
    }
    
    updateUnsubscribeAllButton();
    updateSubscriptionBar();
    
  } catch (error) {
    log(`❌ Unsubscribe failed: ${error.message}`, 'error');
    console.error(error);
  }
};

function updateUnsubscribeAllButton() {
  const hasSubscriptions = unsubscribeTodos !== null || unsubscribeEvents !== null;
  document.getElementById('unsubscribeAllBtn').disabled = !hasSubscriptions;
}

window.toggleSection = function(sectionId) {
  const content = document.getElementById(`${sectionId}-content`);
  const icon = content.previousElementSibling.querySelector('.toggle-icon');
  
  if (content.classList.contains('collapsed')) {
    content.classList.remove('collapsed');
    icon.classList.remove('collapsed');
    icon.textContent = '▼';
  } else {
    content.classList.add('collapsed');
    icon.classList.add('collapsed');
    icon.textContent = '▶';
  }
};

window.executeSqlQuery = async function() {
  const sql = document.getElementById('sqlQuery').value.trim();
  
  if (!sql) {
    log('⚠️  Please enter a SQL query!', 'warning');
    return;
  }

  if (!client) {
    log('⚠️  Please initialize the client first!', 'warning');
    return;
  }

  try {
    log(`→ SQL: ${sql}`, 'info');
    const result = await client.query(sql);
    logJson('← Result', result, 'success');
  } catch (error) {
    log(`❌ SQL execution failed: ${error.message}`, 'error');
    console.error(error);
  }
};

// Add Ctrl+Enter handler for SQL textarea
document.addEventListener('DOMContentLoaded', function() {
  const sqlQuery = document.getElementById('sqlQuery');
  if (sqlQuery) {
    sqlQuery.addEventListener('keydown', function(e) {
      if (e.ctrlKey && e.key === 'Enter') {
        e.preventDefault();
        executeSqlQuery();
      }
    });
  }
});

window.toggleJson = function(id) {
  const element = document.getElementById(id);
  const toggle = element.nextElementSibling;
  const fullContent = decodeURIComponent(element.getAttribute('data-full'));
  
  if (element.classList.contains('expanded')) {
    element.classList.remove('expanded');
    const preview = fullContent.split('\n').slice(0, 3).join('\n');
    const lines = fullContent.split('\n').length;
    element.textContent = preview + (lines > 3 ? '\n...' : '');
    toggle.textContent = '[expand]';
  } else {
    element.classList.add('expanded');
    element.textContent = fullContent;
    toggle.textContent = '[collapse]';
  }
};

window.clearSqlQuery = function() {
  document.getElementById('sqlQuery').value = '';
};

window.clearOutput = function() {
  document.getElementById('output').innerHTML = '';
};
