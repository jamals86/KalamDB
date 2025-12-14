# Live Queries UI Feature

## Overview
The Live Queries page provides real-time monitoring and management of active WebSocket connections and live query subscriptions in KalamDB.

## Features

### 1. **Live Query List**
Displays all active live query subscriptions with comprehensive information:

**Columns:**
- **Subscription ID** - Client-provided identifier
- **User** - User ID who created the subscription
- **Namespace** - Database namespace
- **Table** - Target table name
- **Status** - Current status (Active, Paused, Completed, Error)
- **Duration** - How long the query has been running
- **Changes** - Number of changes/updates sent
- **Query** - SQL query text (truncated with tooltip)
- **Node** - Server node handling the subscription
- **Actions** - Kill query button

### 2. **Statistics Dashboard**
Real-time metrics displayed in cards:
- **Active Queries** - Count of currently active subscriptions
- **Total Queries** - Total number of queries in the system
- **Auto-refresh** - Automatic refresh every 5 seconds

### 3. **Filtering**
Filter queries by:
- User ID
- Namespace ID
- Table Name
- Status (Active, Paused, Completed, Error)

### 4. **Query Management**
**Kill Query Action:**
- Terminates a live query subscription
- Confirmation dialog before killing
- Shows success/error messages
- Auto-refreshes list after action

### 5. **Auto-refresh**
- List automatically refreshes every 5 seconds
- Manual refresh button available
- Loading indicator during refresh

## Component Structure

### Files Created

1. **`ui/src/hooks/useLiveQueries.ts`**
   - Hook for fetching and managing live queries
   - Methods: `fetchLiveQueries()`, `killLiveQuery()`, `getActiveCount()`
   - Uses SQL queries to `system.live_queries` table

2. **`ui/src/components/live-queries/LiveQueryList.tsx`**
   - Main component for displaying the live queries table
   - Handles filtering, killing, and auto-refresh
   - Stats cards with icons

3. **`ui/src/pages/LiveQueries.tsx`**
   - Page wrapper component
   - Contains LiveQueryList component

### Integration Points

4. **`ui/src/App.tsx`** - Added route `/live-queries`
5. **`ui/src/pages/index.ts`** - Exported LiveQueries page
6. **`ui/src/components/layout/Sidebar.tsx`** - Added navigation item with Wifi icon

## Data Source

### Backend System Table
Queries data from `system.live_queries` table:

```sql
SELECT live_id, connection_id, subscription_id, namespace_id, 
       table_name, user_id, query, options, status, 
       created_at, last_update, changes, node
FROM system.live_queries
ORDER BY created_at DESC
```

### LiveQuery Interface
```typescript
interface LiveQuery {
  live_id: string;           // Unique ID: {user_id}-{conn_id}-{table}-{sub_id}
  connection_id: string;     // WebSocket connection ID
  subscription_id: string;   // Client subscription ID
  namespace_id: string;      // Namespace
  table_name: string;        // Target table
  user_id: string;          // User who created query
  query: string;            // SQL query text
  options: string | null;   // JSON options
  status: string;           // Active, Paused, Completed, Error
  created_at: number;       // Unix timestamp (ms)
  last_update: number;      // Last update timestamp (ms)
  changes: number;          // Change count
  node: string;             // Server node ID
}
```

## Kill Query Implementation

### Current Implementation
```typescript
const killLiveQuery = async (liveId: string) => {
  const sql = `KILL LIVE QUERY '${liveId}'`;
  await executeSql(sql);
};
```

### Note on Backend Support
The `KILL LIVE QUERY` command is used to terminate queries. If this command is not yet implemented in the backend, you may need to:

1. **Option A**: Implement SQL `KILL LIVE QUERY` command
2. **Option B**: Add REST API endpoint: `DELETE /v1/api/live-queries/:live_id`
3. **Option C**: Use WebSocket unsubscribe mechanism (requires connection context)

## UI/UX Features

### Status Badges
- **Active** - Default (blue) badge
- **Paused** - Secondary (gray) badge  
- **Completed** - Outline badge
- **Error** - Destructive (red) badge

### Duration Formatting
Displays human-readable duration:
- `45s` - Under 1 minute
- `5m 30s` - Under 1 hour
- `2h 15m` - Under 1 day
- `3d 5h` - Days and hours

### Icons (lucide-react)
- **Activity** (green) - Active queries count
- **Database** (blue) - Total queries count
- **Clock** (orange) - Auto-refresh indicator
- **Wifi** - Navigation menu icon
- **XCircle** - Kill action button

## Usage

### Navigation
1. Log in to Admin UI
2. Click "Live Queries" in sidebar (with Wifi icon)
3. View list of active subscriptions

### Filtering Queries
1. Enter filter criteria in input fields
2. Click "Apply" button
3. Clear filters by emptying fields and clicking "Apply"

### Killing a Query
1. Locate the query in the list
2. Click red "Kill" button in Actions column
3. Confirm in the dialog
4. Success message appears if successful
5. List auto-refreshes to show updated state

### Manual Refresh
- Click circular refresh icon button next to filters
- Icon spins during loading
- List updates with latest data

## Auto-refresh Behavior

```typescript
useEffect(() => {
  loadLiveQueries();
  
  // Auto-refresh every 5 seconds
  const interval = setInterval(() => {
    loadLiveQueries();
  }, 5000);
  
  return () => clearInterval(interval);
}, []);
```

- Fetches data immediately on mount
- Refreshes every 5000ms (5 seconds)
- Cleans up interval on unmount
- User actions (filter, kill) trigger immediate refresh

## Error Handling

### Display Errors
- Fetch errors shown in red alert box at top
- Kill errors shown in separate red alert box
- Success messages shown in green alert box

### Auto-dismiss
- Success messages auto-dismiss after 3 seconds
- Error messages auto-dismiss after 5 seconds
- User can see multiple operations' results

### Confirmation
- All kill operations require confirmation
- Prevents accidental termination
- Shows user/table context in confirmation

## Styling

Uses shadcn-ui components:
- `Table` - Data table layout
- `Card` - Container cards for stats and filters
- `Badge` - Status indicators
- `Button` - Actions and refresh
- `Input` - Filter fields
- `Select` - Status dropdown filter

Custom styling:
- Truncated query text with tooltips
- Monospace font for IDs and queries
- Color-coded status badges
- Responsive grid layout for stats cards

## Future Enhancements

Potential improvements:
1. **Real-time Updates** - WebSocket updates instead of polling
2. **Query Details Modal** - Click row to see full query details
3. **Bulk Actions** - Kill multiple queries at once
4. **Performance Metrics** - Show query performance stats
5. **Historical Data** - View completed/terminated queries
6. **Export** - Export query list to CSV/JSON
7. **Search** - Full-text search across queries
8. **Sorting** - Sort by any column
9. **Pagination** - Handle large numbers of queries
10. **Query Pause/Resume** - Additional lifecycle controls
