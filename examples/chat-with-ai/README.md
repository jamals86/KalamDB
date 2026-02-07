# KalamDB Chat with AI

A real-time chat application demonstrating KalamDB's topic-based messaging pipeline, live queries, and the `kalam-link` TypeScript SDK.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│            Next.js Application                       │
│  ┌─────────────┐         ┌──────────────────────┐   │
│  │  Frontend   │         │ API Routes           │   │
│  │  (React)    │────────▶│ /api/conversations   │   │
│  │             │         │                      │   │
│  └──────┬──────┘         └──────────────────────┘   │
│         │ kalam-link                                 │
│         │ (WebSocket subscriptions)                  │
└─────────┼────────────────────────────────────────────┘
          │
          ▼
   ┌─────────────┐                  ┌────────────────────┐
   │  KalamDB    │─────CDC────────▶ │ chat.ai-processing │
   │  Server     │                  │     (Topic)        │
   │             │◀────CONSUME──────│                    │
   └─────────────┘                  └────────────────────┘
          ▲                                  │
          │                                  │
          │                                  ▼
          │                      ┌───────────────────────┐
          │                      │  AI Processor Service │
          │                      │  (standalone Node.js) │
          │                      │  • Consumes from topic│
          └──────────────────────│  • Generates replies  │
                                 │  • INSERTs to DB      │
                                 └───────────────────────┘
```

**Flow:**
1. User sends a message → Frontend calls API → INSERT into `chat.messages`  
2. Database trigger publishes message to `chat.ai-processing` topic
3. **Standalone AI processor service** consumes from topic continuously
4. Service generates AI response and INSERTs reply into `chat.messages`
5. Frontend receives real-time updates via kalam-link WebSocket subscriptions
6. Messages appear instantly with streaming animation

**✅ Pure kalam-link architecture - No polling, standalone service handles AI processing**

## Features

- **3 Design Variants** - Switch between designs at `/design1`, `/design2`, `/design3`
- **Real-time messaging** - kalam-link WebSocket subscriptions (no polling!)
- **Typing indicators** - Shows when the AI is "thinking"
- **Streaming text animation** - Token-by-token rendering of AI responses
- **Topic-based processing** - Messages flow through KalamDB topics
- **Standalone AI service** - Dedicated Node.js service handles AI responses
- **All kalam-link** - 100% uses kalam-link SDK for all KalamDB communication

## Design Variants

| Route | Name | Style |
|-------|------|-------|
| `/design1` | Classic Chat | ChatGPT-style sidebar + bubbles |
| `/design2` | Modern Minimal | Centered layout, soft gradients |
| `/design3` | Dark Futuristic | Terminal/cyberpunk theme, neon accents |

## Prerequisites

- KalamDB server running on `http://localhost:8080`
- Node.js 18+

## Quick Start

```bash
# 1. Start KalamDB server (from repo root)
cd backend && cargo run

# 2. Setup database tables, topics, and users
cd examples/chat-with-ai
chmod +x setup.sh
./setup.sh

# 3. Install dependencies
npm install

# 4. Start the AI processor service (in one terminal)
npm run service

# 5. Start the Next.js dev server (in another terminal)
npm run dev

# 6. Open browser to any design
open http://localhost:3002/design1  # Classic Chat
open http://localhost:3002/design2  # Modern Minimal
open http://localhost:3002/design3  # Dark Futuristic
```

**Important:** Run `npm run service` first to start the AI processor before opening the UI!

## Project Structure

```
chat-with-ai/
├── setup.sh                    # Database setup script
├── chat-app.sql                # SQL schema (tables + topics)
├── package.json
├── src/
│   ├── app/
│   │   ├── layout.tsx          # Root layout
│   │   ├── page.tsx            # Home (design picker)
│   │   ├── globals.css         # Tailwind + shadcn variables
│   │   ├── api/
│   │   │   └── conversations/  # REST API routes
│   │   ├── design1/            # Classic Chat design
│   │   │   ├── page.tsx
│   │   │   ├── sidebar.tsx
│   │   │   ├── chat-area.tsx
│   │   │   └── empty-state.tsx
│   │   ├── design2/            # Modern Minimal design
│   │   │   ├── page.tsx
│   │   │   ├── conversation-drawer.tsx
│   │   │   ├── chat-panel.tsx
│   │   │   └── welcome-screen.tsx
│   │   └── design3/            # Dark Futuristic design
│   │       ├── page.tsx
│   │       ├── terminal-sidebar.tsx
│   │       ├── terminal-chat.tsx
│   │       └── terminal-welcome.tsx
│   ├── components/
│   │   ├── chat/               # Shared chat components
│   │   │   ├── typing-dots.tsx
│   │   │   ├── streaming-text.tsx
│   │   │   ├── message-input.tsx
│   │   │   └── connection-badge.tsx
│   │   └── ui/                 # shadcn-ui components
│   ├── hooks/
│   │   └── use-kalamdb.ts      # kalam-link subscription hooks
│   ├── providers/
│   │   └── kalamdb-provider.tsx # kalam-link client provider
│   ├── lib/
│   │   ├── config.ts           # Client-side config
│   │   └── utils.ts            # Tailwind merge helper
│   ├── services/
│   │   └── message-processor.ts # Standalone AI processor service
│   └── types/
│       └── chat.ts             # TypeScript type definitions
└── tests/                      # Test suites
    ├── sdk.test.ts             # kalam-link SDK tests
    ├── api.test.ts             # API endpoint tests
    └── hooks.test.tsx          # React hooks tests
```

## SDK Enhancements

This example prompted the addition of these methods to the `kalam-link` TypeScript SDK:

- **`update(tableName, rowId, data)`** - Convenience UPDATE method
- **`queryOne<T>(sql, params)`** - Returns first row as typed object or null
- **`queryAll<T>(sql, params)`** - Returns all rows as typed objects
- **`parseRows<T>(response)`** - Parse QueryResponse into typed objects
- **`TypedSubscriptionCallback<T>`** - Generic typed callback for subscriptions

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `NEXT_PUBLIC_KALAMDB_URL` | KalamDB server URL (frontend) | `http://localhost:8080` |
| `NEXT_PUBLIC_KALAMDB_USERNAME` | Chat user username (frontend) | `admin` |
| `NEXT_PUBLIC_KALAMDB_PASSWORD` | Chat user password (frontend) | `kalamdb123` |
| `KALAMDB_URL` | KalamDB server URL (service) | `http://localhost:8080` |
| `KALAMDB_USERNAME` | Service user username | `admin` |
| `KALAMDB_PASSWORD` | Service user password | `kalamdb123` |

## How It Works

### Message Flow
1. **User sends message** → Frontend uses kalam-link → INSERT into `chat.messages`
2. **Database trigger** → Publishes message to `chat.ai-processing` topic
3. **AI processor service** → Consumes from topic continuously (no polling)
4. **Service generates reply** → INSERTs AI response into `chat.messages`
5. **Frontend subscription** → Receives real-time update via WebSocket
6. **UI updates** → New message appears with streaming animation

### Real-time Subscriptions
- Frontend uses `kalam-link` WebSocket subscriptions
- No polling - messages arrive instantly via live queries
- Typing indicators use separate subscription
- Single WebSocket connection handles all subscriptions
### Real-time Subscriptions
- Frontend uses `kalam-link` WebSocket subscriptions
- No polling - messages arrive instantly via live queries
- Typing indicators use separate subscription
- Single WebSocket connection handles all subscriptions

### Tables
- `chat.conversations` - Chat sessions
- `chat.messages` - All messages (user + assistant)
- `chat.typing_indicators` - Ephemeral typing status (optional)

### Topic Pipeline
```sql
-- Topic for AI processing queue
CREATE TOPIC "chat.ai-processing";

-- Database trigger publishes user messages to topic
-- (See chat-app.sql for full implementation)
```

## Architecture Benefits

✅ **No polling** - Pure event-driven with kalam-link subscriptions  
✅ **Decoupled** - Frontend and AI service are independent  
✅ **Scalable** - Multiple AI services can consume from same topic  
✅ **Testable** - Service can be tested independently  
✅ **Simple** - All communication via kalam-link SDK

## License

Part of the KalamDB project. See repository root for license.
