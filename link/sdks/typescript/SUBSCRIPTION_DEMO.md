# Live Subscription Demo

## Overview

The `example.html` file now includes comprehensive live subscription testing capabilities with health monitoring.

## New Features

### 1. 🏥 Server Health Monitoring

**Automatic Health Check:**
- Checks server health every 5 seconds
- Updates status badge in real-time (Server Online/Offline)
- Displays server health status (healthy/unhealthy/offline)
- Starts automatically on page load

**Status Badge:**
- 🟢 **Green (Server Online)**: Server is healthy and responding
- 🔴 **Red (Server Offline)**: Server is not responding or unhealthy

### 2. 📡 Live Subscriptions Section

**Three New Buttons:**

1. **🔔 Subscribe to Todos**
   - Creates subscription to `test_browser.todos` table
   - Automatically creates table if it doesn't exist
   - Displays live updates in subscription output panel
   - Shows real-time data changes with formatted display

2. **🔕 Unsubscribe**
   - Cancels active subscription
   - Clears subscription output panel
   - Re-enables subscribe button

3. **➕ Insert New Todo (Live)**
   - Inserts random todo items into the subscribed table
   - Demonstrates live updates immediately after insert
   - Randomly generates:
     - Task names (8 different tasks)
     - Priority levels (low/medium/high)
     - Completion status (30% chance of completed)

### 3. 🎨 Subscription Output Panel

**Real-time Display:**
- Shows live updates as they arrive
- Color-coded by priority:
  - 🔴 **High**: Red border
  - 🟠 **Medium**: Orange border
  - 🟢 **Low**: Green border
- Displays:
  - Completion status (✓ or ○)
  - Task title
  - Priority badge
  - Record ID
  - Creation timestamp

## How to Use

### Step-by-Step Demo

1. **Initialize the SDK**
   ```
   Click: "1. Initialize WASM"
   ```

2. **Set up the environment**
   ```
   Click: "3. Create Namespace"
   Click: "4. Create Table"
   ```

3. **Subscribe to live updates**
   ```
   Click: "🔔 Subscribe to Todos"
   ```
   - Watch the subscription output panel
   - Initial data will be loaded

4. **Test live updates**
   ```
   Click: "➕ Insert New Todo (Live)"
   ```
   - Multiple times to see different random todos
   - Watch the subscription panel update in real-time!

5. **Clean up**
   ```
   Click: "🔕 Unsubscribe"
   ```

### Full Automated Test

Click **"▶️ Run All Tests"** to execute all basic operations automatically.

## Technical Details

### Health Check Implementation

```javascript
// Runs every 5 seconds
async function startHealthCheck() {
  const checkHealth = async () => {
    try {
      const response = await fetch(`${config.url}/health`, {
        method: 'GET',
        headers: { 'Accept': 'application/json' }
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
  
  await checkHealth();
  healthCheckInterval = setInterval(checkHealth, 5000);
}
```

### Subscription Implementation

```javascript
subscriptionId = await client.subscribe(
  'test_browser.todos',
  (data) => {
    // Callback function receives live updates
    if (data.rows && data.rows.length > 0) {
      // Format and display data in subscription panel
      displayLiveData(data.rows);
    }
  },
  { /* optional filters */ }
);
```

### Button State Management

- **Subscribe button**: Disabled when subscription is active
- **Unsubscribe button**: Disabled when no subscription
- **Insert Live button**: Only enabled during active subscription

## Browser Requirements

- Modern browser with ES6 module support
- WebSocket support
- Fetch API support
- Served via HTTP (not file://)

## Testing the Demo

1. Start KalamDB server:
   ```powershell
   # From backend directory
   cargo run
   ```

2. Start HTTP server:
   ```powershell
   # From link/sdks/typescript directory
   npx http-server -p 3001
   ```

3. Open browser:
   ```
   http://localhost:3001/link/sdks/typescript/example.html
   ```

4. Watch the health badge turn green when server connects

5. Follow the demo steps above

## Expected Behavior

### Health Check
- Badge updates every 5 seconds
- Shows "Server Online" when KalamDB is running
- Shows "Server Offline" when KalamDB is stopped

### Subscription
- Initial subscription loads existing todos
- New inserts appear immediately in subscription panel
- Multiple rapid inserts all appear in real-time
- Each update shows timestamp and full details

### Visual Feedback
- Green logs for success
- Red logs for errors
- Orange logs for warnings
- Subscription panel updates with color-coded priorities

## Troubleshooting

### Health check shows offline
- Verify KalamDB is running on port 8080
- Check browser console for CORS errors
- Verify health endpoint: `http://localhost:8080/health`

### Subscription not receiving updates
- WebSocket subscriptions may have browser limitations
- Check browser console for WebSocket connection errors
- Verify authentication credentials are correct
- Note: Browser WebSocket API can't set Authorization headers easily

### Insert not showing in subscription
- Ensure subscription is active (unsubscribe button enabled)
- Check main output console for insert success/error
- Verify table exists before subscribing

## Demo Video Script

1. **"Watch the health badge - it's checking the server every 5 seconds"**
2. **"Click Initialize WASM to connect"**
3. **"Create namespace and table"**
4. **"Now subscribe to the todos table"**
5. **"Watch this - click Insert New Todo (Live)"**
6. **"See? The subscription panel updates immediately!"**
7. **"Click it again - different random todo appears instantly"**
8. **"Multiple priorities, different completion states"**
9. **"All updates in real-time through WebSocket subscription"**
10. **"Unsubscribe when done"**

## Future Enhancements

- Add filter controls for subscriptions
- Multiple concurrent subscriptions
- Subscription metrics (messages received, bytes transferred)
- Visual diff showing what changed
- Sound notification on new data
- Export subscription data to JSON/CSV
