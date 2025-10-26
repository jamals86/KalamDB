# React TODO Example - Testing Checklist

## Prerequisites
- ✅ Backend server running: `cd backend && cargo run`
- ✅ Vite dev server running: `cd examples/simple-typescript && npm run dev`
- ✅ Browser open at: http://localhost:5173

## Manual Test Checklist

### 1. Initial Load
- [ ] App loads without errors
- [ ] Connection badge shows "Connected" (green)
- [ ] No error banner displayed
- [ ] Empty state shows: "No todos yet. Add one above!"

### 2. Add TODO
- [ ] Type "Test TODO 1" in input field
- [ ] Click "Add" button
- [ ] TODO appears in list immediately
- [ ] TODO shows uncompleted state (empty checkbox)
- [ ] Input field clears after adding

### 3. Add Multiple TODOs
- [ ] Add "Test TODO 2"
- [ ] Add "Test TODO 3"  
- [ ] All TODOs appear in list
- [ ] TODOs display in correct order

### 4. Toggle TODO Complete
- [ ] Click checkbox for "Test TODO 1"
- [ ] TODO text gets strikethrough
- [ ] Checkbox shows checked
- [ ] Other TODOs remain unchanged

### 5. Delete TODO
- [ ] Click "Delete" button for "Test TODO 2"
- [ ] TODO removed from list immediately
- [ ] Remaining TODOs still visible
- [ ] No errors in console

### 6. Real-time Sync (Multi-tab)
- [ ] Open second browser tab to http://localhost:5173
- [ ] Both tabs show same TODOs
- [ ] Add TODO in tab 1 → appears in tab 2
- [ ] Delete TODO in tab 2 → disappears in tab 1
- [ ] Toggle TODO in tab 1 → updates in tab 2

### 7. Browser Console
- [ ] No errors in console
- [ ] Check for successful WebSocket messages:
  - "KalamClient: Connecting to WebSocket..."
  - "KalamClient: Subscribed to table: app.todos"
- [ ] Subscription messages appear when TODOs change

### 8. Network Tab
- [ ] WebSocket connection established to ws://localhost:8080
- [ ] HTTP POST requests to /v1/api/sql return 200
- [ ] All requests include X-API-KEY header

### 9. Persistence
- [ ] Refresh page (F5)
- [ ] All TODOs reload correctly
- [ ] Completed state preserved
- [ ] Connection re-establishes

### 10. Error Handling
- [ ] Stop backend server
- [ ] Connection badge shows "Disconnected" (red)
- [ ] Error banner appears
- [ ] Restart backend server
- [ ] App reconnects automatically (refresh if needed)

## Known Issues
- [ ] List any issues found during testing

## Test Results
- Date: ___________
- Tester: ___________
- Pass/Fail: ___________
- Notes:

## Automated Testing (Future)
See `link/sdks/typescript/tests/browser-test.html` for SDK-level automated tests.
