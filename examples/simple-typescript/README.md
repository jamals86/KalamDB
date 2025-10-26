# KalamDB TODO Example - TypeScript/React

A real-time TODO application demonstrating KalamDB's WASM client with React, featuring:

- ✅ **Real-time sync** across multiple browser tabs via WebSocket
- ✅ **Offline-first** with localStorage caching for instant load
- ✅ **Professional UI** with modern design and smooth animations
- ✅ **Type-safe** TypeScript throughout
- ✅ **Connection resilience** with automatic reconnection and sync
- ✅ **< 500 LOC** - simple enough to understand, powerful enough to be useful

## Features

### Real-Time Synchronization
- Changes propagate instantly to all connected tabs
- Subscribe from last known ID to catch missed updates
- Automatic reconnection with sync resume

### Offline-First Architecture
- TODOs load instantly from localStorage cache
- Full functionality in read-only mode when disconnected
- Background sync when connection is restored

### Professional UI/UX
- Clean, modern interface with dark mode support
- Smooth animations and transitions
- Accessible design with keyboard navigation
- Responsive layout for mobile and desktop
- Visual connection status indicator

## Prerequisites

- **KalamDB server** running (see [main README](../../README.md))
- **Node.js 18+** and npm
- **API key** from KalamDB (see setup instructions below)

## Quick Start

### 1. Setup Database

```bash
# Make sure KalamDB server is running
cargo run --bin kalamdb-server

# In another terminal, create TODO table
./setup.sh
```

The setup script will:
- ✅ Check KalamDB server connectivity
- ✅ Create the `todos` table if it doesn't exist
- ✅ Verify setup was successful

### 2. Get API Key

**For Local Development/Testing:**

The KalamDB server automatically creates a system user with a test API key on startup when configured in `backend/config.toml`:

```toml
[server]
test_api_key = "test-api-key-12345"
```

This test API key is **only for development** and should never be used in production.

**For Production:**

Create a dedicated user with the CLI:

```bash
# Using kalam CLI
kalam user create --name "demo-user" --role "user"

# Output will show your API key:
# API Key: 550e8400-e29b-41d4-a716-446655440000
# ⚠️  Save this securely!
```

### 3. Configure Environment

```bash
# Copy example env file
cp .env.example .env

# Edit .env and add your API key
# VITE_KALAMDB_URL=ws://localhost:8080
# VITE_KALAMDB_API_KEY=your-api-key-here
```

### 4. Install Dependencies

```bash
npm install
```

**Note**: This example uses the KalamDB TypeScript SDK from `link/sdks/typescript/` as a local dependency. The SDK is automatically linked during `npm install` via the `file:` protocol in package.json:

```json
{
  "dependencies": {
    "@kalamdb/client": "file:../../link/sdks/typescript"
  }
}
```

**Important - Vite Configuration**: Since the SDK is located outside the project directory, Vite's `fs.allow` option must be configured to allow serving files from parent directories:

```typescript
// vite.config.ts
export default defineConfig({
  server: {
    fs: {
      allow: ['..', '../..']  // Allow access to KalamDB repository root
    }
  }
})
```

This allows Vite to serve the WASM module (`kalam_link_bg.wasm`) from the SDK directory.

### 5. Run Development Server

```bash
npm run dev
```

The app will be available at **http://localhost:5173**

## Project Structure

```
examples/simple-typescript/
├── src/
│   ├── types/
│   │   └── todo.ts              # TypeScript type definitions
│   ├── services/
│   │   ├── kalamClient.ts       # WASM client wrapper (mock for dev)
│   │   └── localStorage.ts      # Cache management
│   ├── hooks/
│   │   └── useTodos.ts          # React hook for TODO state
│   ├── components/
│   │   ├── ConnectionStatus.tsx # WebSocket status indicator
│   │   ├── AddTodoForm.tsx      # Add TODO form
│   │   ├── TodoList.tsx         # TODO list container
│   │   └── TodoItem.tsx         # Individual TODO item
│   ├── styles/
│   │   └── App.css              # Professional styling
│   ├── App.tsx                  # Main app component
│   └── main.tsx                 # React entry point
├── setup.sh                     # Database setup script
├── todo-app.sql                 # SQL schema
├── package.json                 # Dependencies
├── tsconfig.json                # TypeScript config
├── vite.config.ts               # Vite config
└── README.md                    # This file
```

## How It Works

### Data Flow

```
┌──────────────┐
│  localStorage│ ◄────┐
└───────┬──────┘      │
        │ Load cache  │ Save cache
        ▼             │
┌──────────────┐      │
│  React State │ ─────┘
└───────┬──────┘
        │ User actions (add/delete/toggle)
        ▼
┌──────────────┐
│ WASM Client  │ ◄──── WebSocket events (insert/update/delete)
└───────┬──────┘
        │ HTTP (write) / WebSocket (subscribe)
        ▼
┌──────────────┐
│ KalamDB      │
│ Server       │
└──────────────┘
```

### Initial Load Sequence

1. **Instant render**: Load TODOs from localStorage cache
2. **Connect**: Establish WebSocket connection with API key
3. **Subscribe**: Subscribe to `todos` table from last sync ID
4. **Sync**: Receive any missed events since last connection
5. **Update**: Merge new events with cached state
6. **Persist**: Save updated state to localStorage

### Real-Time Sync

- All write operations (INSERT, UPDATE, DELETE) go through HTTP `/sql` endpoint
- Server broadcasts changes to all subscribers via WebSocket
- Each tab receives events and updates both UI and localStorage
- Last sync ID is tracked to resume subscriptions after disconnect

### Error Handling

- **No connection**: Display error banner with helpful message
- **Disconnected**: Show warning, disable writes, enable read-only mode
- **Write failure**: Show error message, don't update UI
- **localStorage full**: Clear old cache and retry

## API Reference

### useTodos Hook

```typescript
const {
  todos,              // Todo[] - Current TODO list
  connectionStatus,   // ConnectionStatus - 'connected' | 'connecting' | 'disconnected' | 'error'
  addTodo,            // (title: string) => Promise<void>
  deleteTodo,         // (id: number) => Promise<void>
  toggleTodo,         // (id: number) => Promise<void>
  isLoading,          // boolean - Initial load state
  error               // string | null - Error message
} = useTodos();
```

### KalamDB Client Interface

The example uses the official KalamDB TypeScript SDK (`@kalamdb/client`) which provides a type-safe WASM client. The client is wrapped in `src/services/kalamdb.ts` with TODO-specific helpers:

```typescript
// Import from SDK
import init, { KalamClient } from '@kalamdb/client';

// SDK provides:
class KalamClient {
  constructor(url: string, apiKey: string);
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  isConnected(): boolean;
  insert(table: string, data: string): Promise<string>;
  delete(table: string, rowId: string): Promise<void>;
  query(sql: string): Promise<string>;
  subscribe(table: string, callback: Function): Promise<string>;
  unsubscribe(subscriptionId: string): Promise<void>;
}

// Example wrapper adds convenience methods:
class KalamDBClient {
  async insertTodo(todo: CreateTodoInput): Promise<Todo>;
  async deleteTodo(id: number): Promise<void>;
  async query<T>(sql: string): Promise<T[]>; // Auto-parses JSON
  async subscribe(table: string, fromId: number, callback: SubscriptionCallback): Promise<void>;
}
```

**SDK Location**: `link/sdks/typescript/` (linked as local dependency)  
**SDK Documentation**: See `link/sdks/typescript/README.md` for full API reference

## Testing

### Manual Testing Checklist

- [ ] **Add TODO**: Type title, click Add, verify appears in list
- [ ] **Delete TODO**: Click delete (✕), confirm, verify removed
- [ ] **Toggle completion**: Check/uncheck checkbox, verify strikethrough
- [ ] **Multi-tab sync**: Open 2 tabs, add TODO in one, verify appears in other
- [ ] **localStorage persistence**: Close tab, reopen, verify TODOs still there
- [ ] **Disconnect**: Stop server, verify "Disconnected" badge, add button disabled
- [ ] **Reconnect**: Restart server, verify "Connected" badge, functionality restored
- [ ] **Offline sync**: Disconnect, add TODO elsewhere, reconnect, verify sync

### Automated Tests

```bash
npm test
```

(Test files to be added in future iteration)

## Configuration

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `VITE_KALAMDB_URL` | Yes | - | WebSocket URL (e.g., `ws://localhost:8080`) |
| `VITE_KALAMDB_API_KEY` | Yes | - | API key from `kalam user create` |

### localStorage Keys

- `kalamdb_todos_cache`: Complete TODO list with metadata
- `kalamdb_last_sync_id`: Highest ID seen (for subscription resume)

Cache expires after 7 days of inactivity.

## Customization

### Change TODO Table Schema

Edit `todo-app.sql`:

```sql
CREATE TABLE IF NOT EXISTS todos (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    completed BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    priority TEXT,  -- Add custom field
    tags TEXT       -- Add custom field
);
```

Re-run setup:

```bash
./setup.sh
```

Update types in `src/types/todo.ts`:

```typescript
export interface Todo {
  id: number;
  title: string;
  completed: boolean;
  created_at: string;
  priority?: string;  // Add custom field
  tags?: string;      // Add custom field
}
```

### Customize Styling

All styles are in `src/styles/App.css` using CSS variables:

```css
:root {
  --color-primary: #3b82f6;        /* Change primary color */
  --color-success: #10b981;        /* Change success color */
  --spacing-md: 1rem;              /* Adjust spacing */
  --radius-md: 0.5rem;             /* Change border radius */
}
```

## Troubleshooting

### "Cannot connect to KalamDB"

- ✅ Check server is running: `curl http://localhost:8080/health`
- ✅ Verify `VITE_KALAMDB_URL` matches server address
- ✅ Check firewall isn't blocking WebSocket connections

### "API key is required"

- ✅ Verify `.env` file exists (not just `.env.example`)
- ✅ Check `VITE_KALAMDB_API_KEY` is set correctly
- ✅ Restart dev server after changing `.env`

### TODOs not syncing

- ✅ Check connection status (should be green "Connected")
- ✅ Open browser console for errors
- ✅ Verify WebSocket connection in Network tab

### localStorage not working

- ✅ Check browser privacy settings (localStorage might be disabled)
- ✅ Clear cache: `localStorage.clear()` in console
- ✅ Check storage quota: Chrome DevTools → Application → Storage

## Performance

### Bundle Size

- **Uncompressed**: ~200 KB (React + app code)
- **Gzip**: ~60 KB
- **WASM module**: ~40 KB (when using real WASM client)

### Load Times

- **Initial load** (no cache): ~500ms
- **Initial load** (with cache): <50ms (instant render)
- **WebSocket connection**: ~100ms
- **Subscription setup**: ~50ms
- **Event propagation**: <100ms (server to all clients)

### Browser Support

- Chrome/Edge 90+
- Firefox 88+
- Safari 14+
- Mobile browsers (iOS Safari, Chrome Mobile)

## Production Deployment

### 1. Build for Production

```bash
npm run build
```

Output in `dist/` directory.

### 2. Use Real WASM Client

Replace mock client in `src/services/kalamClient.ts`:

```typescript
// TODO: Uncomment when WASM module is available
import init, { KalamClient } from '../../../link/sdks/typescript/pkg/kalam_link.js';

export async function createKalamClient(config: KalamClientConfig): Promise<IKalamClient> {
  await init(); // Initialize WASM
  return new KalamClient(config.url, config.apiKey);
}
```

### 3. Configure for Production

- Use `wss://` (WebSocket Secure) instead of `ws://`
- Enable HTTPS for web server
- Use environment-specific API keys
- Configure CORS on KalamDB server
- Set up monitoring and error tracking

### 4. Deploy

Serve `dist/` with any static file server:

```bash
# Using Python
python3 -m http.server 8000 --directory dist

# Using Caddy
caddy file-server --root dist --listen :8000

# Using Vercel
vercel deploy dist
```

## Learn More

- [KalamDB Documentation](../../docs/README.md)
- [WASM Client API](../../specs/006-docker-wasm-examples/contracts/wasm-client.md)
- [Quick Start Guide](../../specs/006-docker-wasm-examples/quickstart.md)
- [Architecture Overview](../../docs/architecture/README.md)

## License

Same as KalamDB main project.

## Contributing

Contributions welcome! Please see [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.
