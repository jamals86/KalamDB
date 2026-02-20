# KalamDB Chat with AI

A real-time AI chat application demonstrating KalamDB's topic-based messaging pipeline, live subscriptions, Keycloak OIDC authentication, and the `kalam-link` TypeScript SDK — from a single clone to a fully working example.

## What you'll build

A chat UI where every message you type is stored in KalamDB, routed through a topic to a standalone AI processor service, and the AI reply streams back to your browser in real time — no polling, no webhooks, pure event-driven subscriptions.

```
Browser (Next.js)          KalamDB Server          Keycloak
  │  keycloak-js PKCE  ──▶  validate JWT  ◀──── JWKS endpoint
  │                                │
  │  kalam-link WS  ──────────────▶│
  │                                │
  │  INSERT message ──────────────▶│ chat.messages
  │                                │      │ topic CDC
  │                                │      ▼
  │                        chat.ai-processing (topic)
  │                                │      │ consume
  │                                │      ▼
  │                                │  AI Processor (Node.js)
  │                                │      │ Gemini API call
  │                                │      │ INSERT reply
  │  live subscription  ◀──────────│◀─────┘
  │  (message appears)
```

## Design Variants

Three complete UI themes — switch between them at any time:

| Route | Name | Style |
|-------|------|-------|
| `/design1` | Classic Chat | ChatGPT-style sidebar + message bubbles |
| `/design2` | Modern Minimal | Centered layout, soft gradients |
| `/design3` | Dark Futuristic | Terminal/cyberpunk theme, neon accents |

---

## Prerequisites

Before you start, you need:

| Requirement | Version | Notes |
|-------------|---------|-------|
| Rust + Cargo | stable >= 1.90 | [rustup.rs](https://rustup.rs) |
| Node.js | >= 18 | [nodejs.org](https://nodejs.org) |
| Docker + Docker Compose | any recent | needed for Keycloak |
| Gemini API key | - | free tier works - [aistudio.google.com](https://aistudio.google.com/apikey) |

---

## Getting Started - Step by Step

### Step 1 - Clone the repository

```bash
git clone https://github.com/jamals86/KalamDB.git
cd KalamDB
```

---

### Step 2 - Build and start the KalamDB server

```bash
cd backend
cp server.example.toml server.toml   # copy the default config
cargo run
```

The first build downloads all Rust dependencies and typically takes 3-5 minutes. Once compiled you will see:

```
INFO  kalamdb_server: KalamDB server started on http://0.0.0.0:8080
```

Leave this terminal running. Open a new terminal for the remaining steps.

> **Tip:** Pass `KALAMDB_LOG_LEVEL=debug` for verbose output: `KALAMDB_LOG_LEVEL=debug cargo run`

---

### Step 3 - Configure the server for Keycloak OIDC

Open `backend/server.toml` in an editor and ensure the following lines are present (add them if missing):

```toml
# Trust Keycloak as a JWT issuer
jwt_trusted_issuers = "http://localhost:8081/realms/kalamdb"
auto_create_users_from_provider = true
```

- `jwt_trusted_issuers` - tells KalamDB to accept JWTs issued by Keycloak (RS256, validated via Keycloak's JWKS endpoint at `/.well-known/openid-configuration`).
- `auto_create_users_from_provider` - automatically provisions a KalamDB user account the first time a new Keycloak user logs in. Accounts are created with the `user` role and an ID like `oidc:kcl:<keycloak-uuid>`.

After editing `server.toml`, restart the server (`Ctrl+C` then `cargo run` again).

---

### Step 4 - Start Keycloak

From the repo root, start Keycloak using the bundled compose file:

```bash
cd docker/utils
docker-compose up -d keycloak
```

Wait ~20 seconds for Keycloak to finish booting, then verify:

```bash
curl -s http://localhost:8081/realms/kalamdb | grep realm
# Expected: "realm":"kalamdb"
```

What gets set up automatically:
- **Realm** `kalamdb` imported from `docker/utils/keycloak/realm-import/kalamdb-realm.json`
- **Client** `kalamdb-chat` - PKCE browser client (used by the Next.js UI)
- **Client** `kalamdb-api` - direct-grant client (for scripts and curl testing)
- **Test user** `kalamdb-user` with password `kalamdb123`
- **SSL disabled** for local development (all traffic over `http://`)
- **User registration** enabled - new users can sign up from the login page

Keycloak admin console: `http://localhost:8081` - admin / admin

> **Note:** Keycloak stores its data in `docker/utils/keycloak/data/`. If you need to re-import the realm
> after config changes, wipe this directory first:
> ```bash
> docker-compose down
> rm -rf keycloak/data
> docker-compose up -d keycloak
> ```

---

### Step 5 - Set up the database

Run the setup script from the `examples/chat-with-ai` directory:

```bash
cd examples/chat-with-ai
chmod +x setup.sh
./setup.sh
```

The script:
1. Connects to `http://localhost:8080` using the root password (`kalamdb123` by default)
2. Creates the `chat` namespace, tables (`conversations`, `messages`, `typing_events`), and topic (`ai-processing`) from `chat-app.sql`
3. Creates a `kalamdb-service` user for the AI processor service
4. Writes a `.env.local` file with all required environment variables

Sample output:

```
Logging in as root...
Logged in (token acquired)
Running SQL from chat-app.sql...
Schema created
Creating users...
Created user: kalamdb-service
Generated .env.local
```

If your server is on a different address or uses a different root password:

```bash
./setup.sh --server http://192.168.1.100:8080 --password mysecret
```

---

### Step 6 - Add your Gemini API key

1. Go to [aistudio.google.com/apikey](https://aistudio.google.com/apikey) and create a free API key.
2. Open the `.env.local` that setup.sh just created and set the key:

```bash
GEMINI_API_KEY=your_key_here
```

The complete `.env.local` looks like this:

```dotenv
# Authentication
NEXT_PUBLIC_AUTH_MODE=keycloak
NEXT_PUBLIC_KEYCLOAK_URL=http://localhost:8081
NEXT_PUBLIC_KEYCLOAK_REALM=kalamdb
NEXT_PUBLIC_KEYCLOAK_CLIENT_ID=kalamdb-chat

# KalamDB (frontend)
NEXT_PUBLIC_KALAMDB_URL=http://localhost:8080

# KalamDB (AI processor service)
KALAMDB_URL=http://localhost:8080
KALAMDB_USERNAME=kalamdb-service
KALAMDB_PASSWORD=<generated-by-setup>

# Gemini
GEMINI_API_KEY=your_key_here
GEMINI_MODEL=gemini-2.5-flash
```

---

### Step 7 - Install Node.js dependencies

```bash
# From examples/chat-with-ai/
npm install
```

---

### Step 8 - Start the AI processor service

Open a dedicated terminal and run:

```bash
npm run service
```

You should see:

```
[AI Processor] Connected to KalamDB
[AI Processor] Consuming from topic: chat.ai-processing
[AI Processor] Waiting for messages...
```

**What this service does:**
- Connects to KalamDB using `kalamdb-service` credentials via the kalam-link WASM SDK
- Subscribes to the `chat.ai-processing` topic as consumer group `ai-processor`
- When a new `user` role message arrives, calls the Gemini API for a reply
- Inserts the reply back into `chat.messages` as an `assistant` message
- That `INSERT` triggers live subscription updates on all connected browsers

Leave this terminal running.

---

### Step 9 - Start the Next.js development server

Open another terminal:

```bash
npm run dev
```

The app starts on `http://localhost:3000` (or the next available port). The URL is printed in the terminal:

```
 Next.js 14.x
- Local: http://localhost:3000
```

---

### Step 10 - Open the app and log in

```bash
open http://localhost:3000/design1
```

The app redirects to Keycloak's login page at `http://localhost:8081`. Log in with:

- **Username:** `kalamdb-user`
- **Password:** `kalamdb123`

Or click **Register** to create a new account - it will be automatically provisioned in KalamDB on first login.

After login you are redirected back to the chat UI. Your username appears in the top-right corner. Type a message and watch the AI reply appear in real time.

Try all three designs:

```bash
open http://localhost:3000/design1   # Classic Chat
open http://localhost:3000/design2   # Modern Minimal
open http://localhost:3000/design3   # Dark Futuristic
```

---

## Under the Hood

### Authentication flow (Keycloak PKCE to KalamDB JWT)

```
1. Browser loads /design1
2. keycloak-js detects no active session -> redirects to Keycloak login
3. User logs in -> Keycloak issues an RS256-signed JWT (access token)
4. keycloak-js stores token in memory, sets up background refresh (every 30s)
5. KeycloakKalamDBBridge reads the token and passes it to KalamDBProvider
6. KalamDBProvider calls kalam-link Auth.jwt(token) -> client.connect()
7. kalam-link opens a WebSocket to KalamDB (no username/password needed)
8. KalamDB validates the JWT:
   a. Checks issuer against jwt_trusted_issuers list
   b. Fetches Keycloak's JWKS to verify the RS256 signature
   c. If auto_create_users_from_provider=true, provisions account on first login
9. Session established - queries, inserts, and subscriptions work authenticated
```

Token refresh is automatic: keycloak-js checks every 10 seconds and refreshes the token 30 seconds before expiry, then `KalamDBProvider` recreates the kalam-link client with the new token.

### Message pipeline

```
User types a message
    |
    v
kalam-link INSERT INTO chat.messages (role='user', ...)
    |
    v (CDC - Change Data Capture)
KalamDB publishes row to chat.ai-processing topic
    |
    v
AI Processor service receives the row via consumer.run()
    |
    v
Gemini API generates a reply
    |
    v
AI Processor INSERT INTO chat.messages (role='assistant', content=reply)
    |
    v (live subscription)
Browser receives WebSocket event with new row
    |
    v
UI re-renders the message with streaming text animation
```

### Live subscriptions

The frontend opens a single WebSocket connection to KalamDB via kalam-link and subscribes to a SQL query scoped to the current conversation:

```typescript
await client.subscribeWithSql(
  `SELECT * FROM chat.messages
   WHERE conversation_id = ${conversationId}
   ORDER BY created_at ASC`,
  (event) => {
    if (event.type === 'change' && event.change_type === 'insert') {
      addMessage(event.rows[0]);
    }
  },
  { batch_size: 200 }
);
```

When any `INSERT` happens on `chat.messages` for that `conversation_id`, the server pushes the new row over the existing WebSocket immediately. No polling. No server-sent events. One connection per browser tab handles all table subscriptions.

### Database schema

```sql
CREATE NAMESPACE IF NOT EXISTS chat;

-- Conversations - one per chat session
CREATE TABLE chat.conversations (
  id              BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  title           TEXT NOT NULL,
  created_at      TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000');

-- Messages - user + assistant turns
CREATE TABLE chat.messages (
  id              BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  conversation_id BIGINT NOT NULL,
  role            TEXT NOT NULL,   -- 'user' | 'assistant'
  content         TEXT NOT NULL,
  created_at      TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'USER', FLUSH_POLICY = 'rows:1000');

-- Typing indicators - ephemeral stream, auto-expired after 30s
CREATE TABLE chat.typing_events (
  id              BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
  conversation_id BIGINT NOT NULL,
  user_id         TEXT NOT NULL,
  event_type      TEXT NOT NULL,   -- 'start' | 'stop'
  created_at      TIMESTAMP DEFAULT NOW()
) WITH (TYPE = 'STREAM', TTL_SECONDS = 30);

-- Topic for async AI processing
CREATE TOPIC "chat.ai-processing";

-- CDC route: every INSERT into chat.messages publishes to the topic
ALTER TOPIC "chat.ai-processing"
ADD SOURCE chat.messages
ON INSERT
WITH (payload = 'full');
```

`TYPE = 'USER'` means each authenticated user has a private, isolated partition of data - users only see their own conversations and messages. `TYPE = 'STREAM'` creates an append-only, short-lived log ideal for ephemeral events.

---

## Environment Variables Reference

| Variable | Description | Default |
|----------|-------------|---------|
| `NEXT_PUBLIC_AUTH_MODE` | `keycloak` (OIDC) or `basic` (username/password) | `keycloak` |
| `NEXT_PUBLIC_KEYCLOAK_URL` | Keycloak server URL | `http://localhost:8081` |
| `NEXT_PUBLIC_KEYCLOAK_REALM` | Keycloak realm name | `kalamdb` |
| `NEXT_PUBLIC_KEYCLOAK_CLIENT_ID` | Keycloak client ID for the browser app | `kalamdb-chat` |
| `NEXT_PUBLIC_KALAMDB_URL` | KalamDB server base URL (used by frontend) | `http://localhost:8080` |
| `NEXT_PUBLIC_KALAMDB_USERNAME` | Username for basic auth mode only | `admin` |
| `NEXT_PUBLIC_KALAMDB_PASSWORD` | Password for basic auth mode only | `kalamdb123` |
| `KALAMDB_URL` | KalamDB server URL (used by AI processor service) | `http://localhost:8080` |
| `KALAMDB_USERNAME` | AI processor service username | - |
| `KALAMDB_PASSWORD` | AI processor service password | - |
| `GEMINI_API_KEY` | Google Gemini API key | - |
| `GEMINI_MODEL` | Gemini model ID | `gemini-2.5-flash` |
| `AI_MAX_OUTPUT_TOKENS` | Max tokens per AI reply | `512` |
| `AI_TEMPERATURE` | Gemini temperature (0-1) | `0.7` |
| `AI_CONTEXT_WINDOW_MESSAGES` | Recent messages passed as context | `20` |

> **Security note:** Variables prefixed with `NEXT_PUBLIC_` are bundled into the browser JavaScript bundle.
> Never put secrets there. Service credentials and API keys belong in unprefixed variables - they stay server-side only.

---

## Switching to Basic Auth (no Keycloak required)

Set `NEXT_PUBLIC_AUTH_MODE=basic` in `.env.local`:

```dotenv
NEXT_PUBLIC_AUTH_MODE=basic
NEXT_PUBLIC_KALAMDB_USERNAME=admin
NEXT_PUBLIC_KALAMDB_PASSWORD=kalamdb123
```

In this mode the browser connects directly with username/password credentials - no OIDC redirect, no Keycloak needed. The Keycloak and server.toml OIDC config can be left in place; it doesn't interfere.

---

## Project Structure

```
chat-with-ai/
├── setup.sh                       # Database setup + .env.local generator
├── chat-app.sql                   # Full SQL schema (tables, topics, routes)
├── .env.local                     # Your local config (generated by setup.sh)
├── .env.example                   # Reference config with all variables
├── package.json
└── src/
    ├── app/
    │   ├── layout.tsx             # Root layout (providers wrapper)
    │   ├── page.tsx               # Home page (design picker)
    │   ├── globals.css
    │   ├── api/
    │   │   └── conversations/     # REST API for conversation CRUD
    │   ├── design1/               # Classic Chat UI
    │   ├── design2/               # Modern Minimal UI
    │   └── design3/               # Dark Futuristic UI
    ├── components/
    │   ├── user-menu.tsx          # Logged-in user + logout button
    │   └── chat/                  # Shared chat components
    │       ├── typing-dots.tsx
    │       ├── streaming-text.tsx
    │       ├── message-input.tsx
    │       └── connection-badge.tsx
    ├── hooks/
    │   └── use-kalamdb.ts         # kalam-link subscription hooks
    ├── providers/
    │   ├── kalamdb-provider.tsx   # kalam-link client (token or basic auth)
    │   └── keycloak-provider.tsx  # Keycloak OIDC session + token lifecycle
    ├── lib/
    │   ├── config.ts              # App config from env vars
    │   └── utils.ts               # Tailwind merge helper
    ├── services/
    │   ├── ai-agent.ts            # Vercel AI SDK + Gemini reply generation
    │   ├── service-config.ts      # Env loading + validation for the service
    │   └── message-processor.ts  # Standalone AI processor entrypoint
    └── types/
        └── chat.ts                # TypeScript type definitions
```

---

## Troubleshooting

### Keycloak login page shows an SSL error

**Symptom:** The browser warns about an insecure connection, or the Keycloak redirect fails with an SSL error.

**Fix:** The realm JSON already sets `sslRequired: none` for local development. If you see this after a volume reset, wipe Keycloak data and restart:

```bash
cd docker/utils
docker-compose down
rm -rf keycloak/data
docker-compose up -d keycloak
```

---

### Browser stuck in Keycloak redirect loop

**Symptom:** You log in successfully but the app immediately redirects to login again.

**Cause:** The `kalamdb-chat` Keycloak client's redirect URI list doesn't include the port Next.js is actually running on.

**Fix:** Find the actual port (check `npm run dev` output, e.g. `:3002`) and add it in Keycloak admin:
1. Open `http://localhost:8081` -> Realm `kalamdb` -> Clients -> `kalamdb-chat` -> Settings
2. Add `http://localhost:3002/*` to **Valid redirect URIs**
3. Add `http://localhost:3002` to **Web origins**
4. Save

---

### Local `admin` account gets `UNTRUSTED_ISSUER`

**Symptom:** Requests with local KalamDB credentials return `{"error":{"code":"UNTRUSTED_ISSUER",...}}`

The server always keeps the internal `"kalamdb"` issuer in the trusted list alongside external issuers. If still seeing this, rebuild:

```bash
cd backend && cargo build
```

---

### AI replies not appearing

1. Confirm the AI processor is running: check the `npm run service` terminal
2. Confirm `GEMINI_API_KEY` is set in `.env.local`
3. Check the processor terminal for API errors (quota, network, key issues)
4. Re-run `./setup.sh` to verify the table schema and topic route exist (idempotent)

---

### Port 3000 is already in use

Next.js automatically increments the port. Check the `npm run dev` output for the actual URL (e.g. `http://localhost:3002`). Update your Keycloak redirect URIs to match - see the redirect loop fix above.

---

## License

Part of the KalamDB project. See the repository root for license details.
