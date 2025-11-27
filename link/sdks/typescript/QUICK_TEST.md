# Quick Test Guide - Live Subscriptions

## 🚀 Quick Start (3 steps)

1. **Open browser**: http://localhost:3001/link/sdks/typescript/example.html
2. **Click**: "1. Initialize WASM" button
3. **Click**: "🔔 Subscribe to Todos" button
4. **Click**: "➕ Insert New Todo (Live)" multiple times
5. **Watch**: Subscription panel updates in real-time!

## 🎯 What You'll See

### Health Badge (Top Right)
- Automatically checks server every 5 seconds
- 🟢 **"Server Online"** = KalamDB is running
- 🔴 **"Server Offline"** = KalamDB is down

### Subscription Panel
When you click "Insert New Todo (Live)", you'll see:

```
📡 Live Updates (Last: 1:52:30 PM)

○ Review pull request [MEDIUM]
  ID: 1234567890123456 | Created: 11/24/2025, 1:52:30 PM

✓ Fix critical bug [HIGH]
  ID: 1234567890123457 | Created: 11/24/2025, 1:52:31 PM
```

## 🔄 Test Sequence

```
Initialize → Subscribe → Insert (Live) → Insert (Live) → Insert (Live) → Unsubscribe
```

## 🎨 Color Coding

- 🔴 **Red border** = High priority
- 🟠 **Orange border** = Medium priority  
- 🟢 **Green border** = Low priority
- ✓ = Completed
- ○ = Not completed

## 📊 What Gets Inserted

Random combinations of:

**Tasks:**
- Review pull request
- Update documentation
- Fix critical bug
- Deploy to production
- Write unit tests
- Refactor authentication
- Optimize database queries
- Design new API endpoint

**Priorities:** low, medium, high  
**Completed:** 30% chance of being completed

## 🐛 Troubleshooting

**Health badge stuck on "Server Offline"?**
- Check if KalamDB is running: `http://localhost:8080/health`
- Restart KalamDB server

**Subscribe button does nothing?**
- Click "Initialize WASM" first
- Check browser console for errors
- Verify credentials (default: root/root)

**No live updates appearing?**
- Ensure you clicked "🔔 Subscribe to Todos" first
- Look for green "✅ Subscribed!" message in output
- Verify "➕ Insert New Todo (Live)" button is enabled

## 💡 Pro Tips

1. **Open two browser tabs** side-by-side:
   - Both subscribe to same table
   - Insert from one tab
   - Watch both update simultaneously!

2. **Test rapid inserts**:
   - Click "Insert New Todo (Live)" 10 times quickly
   - All 10 should appear in subscription panel

3. **Monitor health while testing**:
   - Stop KalamDB server mid-subscription
   - Watch badge turn red within 5 seconds
   - Restart server, watch it turn green

## 🎬 Demo Script

"Let me show you real-time subscriptions..."

1. "First, initialize the SDK" → Click Initialize
2. "Now subscribe to the todos table" → Click Subscribe  
3. "Watch what happens when I insert data" → Click Insert Live
4. "See? It appears immediately!" → Point to subscription panel
5. "Let me add a few more" → Click Insert Live 3-4 times rapidly
6. "All updates arrive in real-time through WebSocket"
7. "And look - the health badge shows server is online"

## 📈 Performance Notes

- Health checks: Every 5 seconds
- Subscription updates: Immediate (WebSocket)
- Insert operations: ~50-200ms
- UI updates: <10ms after data received

## 🔧 Technical Implementation

```javascript
// Health monitoring (auto-start)
window.addEventListener('load', () => {
  startHealthCheck(); // Checks /health every 5s
});

// Live subscription
subscriptionId = await client.subscribe(
  'test_browser.todos',
  (data) => { /* real-time callback */ }
);

// Insert triggers live update
await client.insert('test_browser.todos', {
  title: 'New task',
  priority: 'high'
});
// ↑ Subscription callback fires immediately
```
