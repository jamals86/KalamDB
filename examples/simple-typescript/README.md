# KalamDB TODO App Example

A real-time TODO application built with React, TypeScript, and KalamDB using WebAssembly. Demonstrates real-time synchronization, localStorage caching, and offline-first capabilities.

## Features

- ✅ **Real-time sync**: Changes propagate instantly across all open browser tabs
- 💾 **LocalStorage caching**: TODOs load instantly on app startup
- 🔄 **Offline-first**: Queue changes when disconnected, sync when reconnected
- 🔐 **API key authentication**: Secure connection to KalamDB server
- 🌐 **WASM-powered**: Uses KalamDB's WebAssembly client for browser compatibility

## Testing

The project includes comprehensive test suites to verify functionality:

### Run All Tests

```bash
npm test
```

This runs both the WASM module tests and database integration tests.

### WASM Module Tests

```bash
npm run test:wasm
```

Tests the KalamDB WASM client module:
- ✅ Module initialization and loading
- ✅ Client constructor with parameter validation
- ✅ Connection state management
- ✅ Error handling for invalid inputs
- ✅ Method presence verification (8 methods)

**Note:** The WASM client currently has stub implementations for HTTP methods. See `cli/kalam-link/src/wasm.rs` for implementation status.

### Database Integration Tests

```bash
npm run test:db
```

Tests actual database operations with the KalamDB server:
- ✅ INSERT operations with auto-increment IDs
- ✅ SELECT queries with various clauses
- ✅ UPDATE operations (modify existing rows)
- ✅ DELETE operations (soft delete behavior)
- ✅ COUNT aggregation queries
- ✅ Batch INSERT (multiple rows)
- ✅ WHERE clause filtering
- ✅ LIKE pattern matching
- ✅ Cleanup and verification

**Known Limitations:**
- UPDATE and DELETE return "0 rows affected" for USER tables (limitation being investigated)
- DELETE with LIKE pattern not supported (use simple `col=value` syntax)
- Soft delete behavior means deleted rows may still appear in queries

## Prerequisites

1. **KalamDB server** must be running:
   ```bash
   cd ../../backend
   cargo run --bin kalamdb-server
   ```

2. **Create a user** to get an API key:
   ```bash
   cargo run --bin kalamdb-server -- create-user \
     --username todo-app \
     --email todo@example.com \
     --role user
   ```
   
   Save the API key from the output!

3. **Node.js 18+** for running the React app

## Setup

1. **Configure environment variables**:
   ```bash
   cp .env.example .env
   # Edit .env and add your API key
   ```

2. **Run setup script** to create database tables:
   ```bash
   ./setup.sh
   ```

3. **Install dependencies**:
   ```bash
   npm install
   ```

4. **Copy WASM files** (one-time setup):
   ```bash
   # Build WASM module if not already built
   cd ../../cli/kalam-link
   wasm-pack build --target web --out-dir pkg --features wasm --no-default-features
   
   # Copy to example directory
   cd ../../examples/simple-typescript
   mkdir -p public/pkg
   cp -r ../../cli/kalam-link/pkg/* public/pkg/
   ```

## Usage

### Development

Start the development server:

```bash
npm run dev
```

The app will open at http://localhost:3000

### Building for Production

```bash
npm run build
npm run preview
```

## Features Demo

### 1. Adding TODOs

- Type your TODO in the input field
- Click "➕ Add TODO" or press Enter
- The TODO appears immediately (real-time)

### 2. Real-time Synchronization

1. Open the app in **two browser tabs**
2. Add a TODO in one tab
3. Watch it appear instantly in the other tab! 🎉

### 3. LocalStorage Persistence

1. Add some TODOs
2. Close the browser completely
3. Reopen the app
4. TODOs load instantly from cache ⚡

### 4. Connection Status

- **🟢 Connected**: Server is reachable, real-time sync active
- **🔴 Disconnected**: Server unreachable, add button disabled

### 5. Offline-First (Reconnection Sync)

1. Disconnect from server (stop kalamdb-server)
2. Notice connection status turns red
3. Reconnect (restart server)
4. App automatically syncs missed changes

## Architecture

### Directory Structure

```
examples/simple-typescript/
├── src/
│   ├── types/
│   │   └── todo.ts              # TypeScript type definitions
│   ├── services/
│   │   ├── kalamClient.ts       # WASM client initialization
│   │   └── localStorage.ts      # Browser cache management
│   ├── hooks/
│   │   └── useTodos.ts          # Custom React hook for TODO state
│   ├── components/
│   │   ├── ConnectionStatus.tsx # Connection indicator
│   │   ├── AddTodoForm.tsx      # Form for adding TODOs
│   │   ├── TodoList.tsx         # List of TODOs
│   │   └── TodoItem.tsx         # Individual TODO item
│   ├── styles/
│   │   └── App.css              # Application styles
│   ├── App.tsx                  # Main app component
│   └── main.tsx                 # React entry point
├── public/
│   └── pkg/                     # WASM module files
├── index.html                   # HTML entry point
├── package.json                 # Dependencies
├── tsconfig.json                # TypeScript config
├── vite.config.ts               # Vite build config
├── setup.sh                     # Database setup script
├── todo-app.sql                 # SQL schema
└── .env.example                 # Environment template
```

### Data Flow

```
User Action → WASM Client → KalamDB Server → WebSocket Event
                   ↓                               ↓
             LocalStorage ← React State Update ←──┘
```

1. **User adds TODO**: Form calls `addTodo()` → WASM client inserts to KalamDB
2. **Server broadcasts**: KalamDB sends WebSocket event to all subscribed clients
3. **Local update**: Subscription handler updates React state and localStorage
4. **UI updates**: React re-renders with new TODO list

### Key Files

#### `src/hooks/useTodos.ts`
Custom React hook managing:
- TODO state (useState)
- Connection status (isConnected)
- WASM client initialization
- WebSocket subscription
- LocalStorage sync
- Add/delete operations

#### `src/services/kalamClient.ts`
WASM client wrapper:
- Singleton pattern for client instance
- Environment variable loading
- API key validation

#### `src/services/localStorage.ts`
Browser cache management:
- `loadTodosFromCache()` - Load on app start
- `saveTodosToCache()` - Save after each change
- `getLastSyncId()` / `setLastSyncId()` - Track sync position

## Environment Variables

Create `.env` file:

```bash
# KalamDB server URL
VITE_KALAMDB_URL=http://localhost:8080

# API key from create-user command
VITE_KALAMDB_API_KEY=your-api-key-here
```

## Troubleshooting

### "Cannot reach KalamDB server"

- Ensure server is running: `cargo run --bin kalamdb-server`
- Check URL in `.env` matches server address
- Verify no firewall blocking port 8080

### "VITE_KALAMDB_API_KEY is not set"

- Run `./setup.sh` to see detailed error
- Create user with `kalamdb-server create-user`
- Copy API key to `.env` file

### "Failed to initialize WASM module"

- Ensure WASM files exist in `public/pkg/`
- Rebuild WASM: `cd ../../cli/kalam-link && wasm-pack build --target web --features wasm --no-default-features`
- Copy files: `cp -r ../../cli/kalam-link/pkg/* public/pkg/`

### TODOs not syncing across tabs

- Check connection status shows "🟢 Connected"
- Open browser console (F12) for WebSocket errors
- Verify API key is valid

### Build errors

- Clear node_modules: `rm -rf node_modules package-lock.json && npm install`
- Clear Vite cache: `rm -rf node_modules/.vite`
- Check Node.js version: `node --version` (should be 18+)

## Development Tips

### Hot Reload

Vite provides instant hot reload. Changes to `.tsx` files update immediately without page refresh.

### Browser DevTools

- **Network tab**: Monitor WebSocket connection
- **Application tab**: Inspect localStorage cache
- **Console**: See subscription events and errors

### Testing Multi-Tab Sync

1. Open http://localhost:3000 in multiple tabs
2. Arrange side-by-side
3. Add/delete TODOs in one tab
4. Watch real-time updates in other tabs

### Debugging WASM

The WASM client logs to browser console. Look for:
- Connection errors
- Subscription events
- Insert/delete operations

## Next Steps

- **Add TODO completion**: Implement toggle completed status
- **Add filtering**: Show all/active/completed TODOs
- **Add editing**: Click to edit TODO title
- **Add bulk operations**: Clear completed, mark all complete
- **Add user authentication**: Multi-user support
- **Add animations**: Smooth transitions for add/delete

## License

See main KalamDB repository for license information.
