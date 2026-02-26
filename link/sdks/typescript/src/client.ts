/**
 * KalamDBClient — TypeScript wrapper around the WASM bindings
 *
 * Most network I/O (queries, subscriptions, topic consume/ack, authentication)
 * goes through the compiled Rust/WASM `KalamClient`. File uploads use direct
 * fetch() + FormData for better browser compatibility.
 */

import init, { KalamClient as WasmClient } from '../.wasm-out/kalam_link.js';

import type { AuthCredentials } from './auth.js';
import { buildAuthHeader } from './auth.js';

import type {
  ClientOptions,
  ConsumeContext,
  ConsumerHandle,
  ConsumerHandler,
  LoginResponse,
  OnConnectCallback,
  OnDisconnectCallback,
  OnErrorCallback,
  OnReceiveCallback,
  OnSendCallback,
  QueryResponse,
  ServerMessage,
  SubscriptionCallback,
  SubscriptionInfo,
  SubscriptionOptions,
  Unsubscribe,
  UploadProgress,
  Username,
} from './types.js';

import type {
  AckResponse,
  ConsumeMessage,
  ConsumeRequest,
  ConsumeResponse,
} from '../.wasm-out/kalam_link.js';

import { parseRows } from './helpers/query_helpers.js';

type NodeWindowShim = {
  location?: {
    protocol: string;
    hostname: string;
    port: string;
    href: string;
  };
  fetch?: typeof fetch;
};

/* ================================================================== */
/*  KalamDBClient                                                     */
/* ================================================================== */

/**
 * KalamDB Client — type-safe interface to KalamDB
 *
 * Features:
 * - SQL query execution (via WASM)
 * - Real-time WebSocket subscriptions
 * - Topic consume / ack (via WASM)
 * - Multiple auth methods (Basic, JWT, Anonymous)
 * - Cross-platform (Node.js & Browser)
 * - Subscription tracking & management
 *
 * @example
 * ```typescript
 * import { createClient, Auth } from 'kalam-link';
 *
 * const client = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.basic('alice', 'password123'),
 * });
 *
 * await client.connect();
 *
 * const users = await client.query('SELECT * FROM users WHERE active = true');
 * console.log(users.results[0].rows);
 *
 * const unsubscribe = await client.subscribe('messages', (event) => {
 *   if (event.type === 'change') console.log('New:', event.rows);
 * });
 *
 * await unsubscribe();
 * await client.disconnect();
 * ```
 */
export class KalamDBClient {
  private wasmClient: WasmClient | null = null;
  private initialized = false;
  private connecting: Promise<void> | null = null;
  private url: string;
  private auth: AuthCredentials;
  private wasmUrl?: string | BufferSource;
  private autoConnect: boolean;
  private pingIntervalMs: number;

  /** Track active subscriptions for management */
  private subscriptions: Map<string, SubscriptionInfo> = new Map();

  /** Connection lifecycle event handlers */
  private _onConnect?: OnConnectCallback;
  private _onDisconnect?: OnDisconnectCallback;
  private _onError?: OnErrorCallback;
  private _onReceive?: OnReceiveCallback;
  private _onSend?: OnSendCallback;

  /**
   * Create a new KalamDB client
   *
   * @param options - Client options with URL and auth credentials
   * @throws Error if url or auth is missing
   *
   * @example
   * ```typescript
   * import { KalamDBClient, Auth } from 'kalam-link';
   *
   * const client = new KalamDBClient({
   *   url: 'http://localhost:8080',
   *   auth: Auth.basic('admin', 'secret'),
   * });
   * ```
   */
  constructor(options: ClientOptions) {
    if (!options.url) throw new Error('KalamDBClient: url is required');
    if (!options.auth) throw new Error('KalamDBClient: auth is required');

    this.url = options.url;
    this.auth = options.auth;
    this.wasmUrl = options.wasmUrl;
    this.autoConnect = options.autoConnect ?? true;
    this.pingIntervalMs = options.pingIntervalMs ?? 30_000;
    this._onConnect = options.onConnect;
    this._onDisconnect = options.onDisconnect;
    this._onError = options.onError;
    this._onReceive = options.onReceive;
    this._onSend = options.onSend;
  }

  /* ---------------------------------------------------------------- */
  /*  Auth helpers                                                    */
  /* ---------------------------------------------------------------- */

  /** Get the current authentication type */
  getAuthType(): 'basic' | 'jwt' | 'none' {
    return this.auth.type;
  }

  /* ---------------------------------------------------------------- */
  /*  Lifecycle                                                       */
  /* ---------------------------------------------------------------- */

  /**
   * Initialize WASM module and create client instance.
   * Called automatically by `connect()` if not already done.
   */
  async initialize(): Promise<void> {
    if (this.initialized) return;

    try {
      await this.ensureNodeRuntimeCompat();
      console.log('[kalam-link] Starting WASM initialization...');

      if (this.wasmUrl) {
        const kind = typeof this.wasmUrl === 'string' ? 'URL' : 'buffer';
        console.log(`[kalam-link] Loading WASM from explicit ${kind}`);
        await init({ module_or_path: this.wasmUrl });
      } else {
        console.log('[kalam-link] Loading WASM from default path');
        await init();
      }

      switch (this.auth.type) {
        case 'basic':
          this.wasmClient = new WasmClient(this.url, this.auth.username, this.auth.password);
          break;
        case 'jwt':
          this.wasmClient = WasmClient.withJwt(this.url, this.auth.token);
          break;
        case 'none':
          this.wasmClient = WasmClient.anonymous(this.url);
          break;
      }

      this.initialized = true;
      console.log('[kalam-link] Initialization complete');

      // Wire connection lifecycle event handlers to the WASM client
      this.applyEventHandlers();
    } catch (error) {
      console.error('[kalam-link] WASM initialization failed:', error);
      throw new Error(`Failed to initialize WASM client: ${error}`);
    }
  }

  /**
   * Connect to KalamDB server via WebSocket.
   * Also initialises WASM if not already done.
   *
   * When using Basic auth, `login()` is called automatically to exchange
   * credentials for a JWT before opening the WebSocket.
   *
   * This is called lazily by `subscribe()` / `subscribeWithSql()` unless
   * `autoConnect` was set to `false`.
   */
  async connect(): Promise<void> {
    // Deduplicate concurrent connect() calls — only one handshake at a time.
    if (this.connecting) return this.connecting;

    this.connecting = (async () => {
      try {
        await this.initialize();
        if (!this.wasmClient) throw new Error('WASM client not initialized');

        // Auto-login: exchange Basic credentials for JWT before WebSocket connect
        if (this.auth.type === 'basic') {
          await this.login();
        }

        // Forward ping interval to the WASM client before connecting
        if (this.pingIntervalMs !== 30_000) {
          this.wasmClient.setPingInterval(this.pingIntervalMs);
        }

        await this.wasmClient.connect();
      } finally {
        this.connecting = null;
      }
    })();

    return this.connecting;
  }

  /** Disconnect and clean up all subscriptions */
  async disconnect(): Promise<void> {
    if (this.wasmClient) {
      await this.wasmClient.disconnect();
    }
    this.subscriptions.clear();
  }

  /**
   * Async dispose — enables `await using client = createClient(...)`.
   *
   * Automatically disconnects and cleans up all subscriptions when the
   * variable goes out of scope. Requires Node 20+ or a modern browser
   * with Explicit Resource Management support.
   *
   * @example
   * ```typescript
   * async function demo() {
   *   await using client = createClient({ url, auth });
   *   await client.connect();
   *   // ... use client ...
   * } // client.disconnect() called automatically here
   * ```
   */
  async [Symbol.asyncDispose](): Promise<void> {
    await this.disconnect();
  }

  /**
   * Sync dispose — enables `using client = createClient(...)`.
   *
   * Fires disconnect as fire-and-forget (best-effort) since `using`
   * does not await. Prefer `await using` for reliable cleanup.
   */
  [Symbol.dispose](): void {
    void this.disconnect();
  }

  /** Whether the WebSocket connection is active */
  isConnected(): boolean {
    return this.wasmClient?.isConnected() ?? false;
  }

  /* ---------------------------------------------------------------- */
  /*  Connection Event Handlers                                       */
  /* ---------------------------------------------------------------- */

  /**
   * Register a handler called when the WebSocket connection is established.
   *
   * Can also be set via `ClientOptions.onConnect` at construction time.
   *
   * @example
   * ```typescript
   * client.onConnect(() => console.log('Connected!'));
   * ```
   */
  onConnect(callback: OnConnectCallback): void {
    this._onConnect = callback;
    if (this.wasmClient) {
      this.wasmClient.onConnect(callback);
    }
  }

  /**
   * Register a handler called when the WebSocket connection is closed.
   *
   * Can also be set via `ClientOptions.onDisconnect` at construction time.
   *
   * @example
   * ```typescript
   * client.onDisconnect((reason) => console.log('Disconnected:', reason.message));
   * ```
   */
  onDisconnect(callback: OnDisconnectCallback): void {
    this._onDisconnect = callback;
    if (this.wasmClient) {
      this.wasmClient.onDisconnect(callback as unknown as Function);
    }
  }

  /**
   * Register a handler called when a connection error occurs.
   *
   * Can also be set via `ClientOptions.onError` at construction time.
   *
   * @example
   * ```typescript
   * client.onError((err) => console.error('Error:', err.message));
   * ```
   */
  onError(callback: OnErrorCallback): void {
    this._onError = callback;
    if (this.wasmClient) {
      this.wasmClient.onError(callback as unknown as Function);
    }
  }

  /**
   * Register a debug handler called for every raw message received from the server.
   *
   * Can also be set via `ClientOptions.onReceive` at construction time.
   *
   * @example
   * ```typescript
   * client.onReceive((msg) => console.log('[RECV]', msg));
   * ```
   */
  onReceive(callback: OnReceiveCallback): void {
    this._onReceive = callback;
    if (this.wasmClient) {
      this.wasmClient.onReceive(callback as unknown as Function);
    }
  }

  /**
   * Register a debug handler called for every raw message sent to the server.
   *
   * Can also be set via `ClientOptions.onSend` at construction time.
   *
   * @example
   * ```typescript
   * client.onSend((msg) => console.log('[SEND]', msg));
   * ```
   */
  onSend(callback: OnSendCallback): void {
    this._onSend = callback;
    if (this.wasmClient) {
      this.wasmClient.onSend(callback as unknown as Function);
    }
  }

  /** @internal Wire stored event handler callbacks to the WASM client */
  private applyEventHandlers(): void {
    if (!this.wasmClient) return;
    if (this._onConnect) this.wasmClient.onConnect(this._onConnect);
    if (this._onDisconnect) this.wasmClient.onDisconnect(this._onDisconnect as unknown as Function);
    if (this._onError) this.wasmClient.onError(this._onError as unknown as Function);
    if (this._onReceive) this.wasmClient.onReceive(this._onReceive as unknown as Function);
    if (this._onSend) this.wasmClient.onSend(this._onSend as unknown as Function);
  }

  /* ---------------------------------------------------------------- */
  /*  Reconnection                                                    */
  /* ---------------------------------------------------------------- */

  /** Enable/disable automatic reconnection */
  setAutoReconnect(enabled: boolean): void {
    this.requireInit();
    this.wasmClient!.setAutoReconnect(enabled);
  }

  /** Set reconnection delay (initial + max for exponential back-off) */
  setReconnectDelay(initialDelayMs: number, maxDelayMs: number): void {
    this.requireInit();
    this.wasmClient!.setReconnectDelay(BigInt(initialDelayMs), BigInt(maxDelayMs));
  }

  /** Set max reconnection attempts (0 = infinite) */
  setMaxReconnectAttempts(maxAttempts: number): void {
    this.requireInit();
    this.wasmClient!.setMaxReconnectAttempts(maxAttempts);
  }

  /** Current reconnection attempt count (resets after success) */
  getReconnectAttempts(): number {
    this.requireInit();
    return this.wasmClient!.getReconnectAttempts();
  }

  /** Whether a reconnection is currently in progress */
  isReconnecting(): boolean {
    this.requireInit();
    return this.wasmClient!.isReconnecting();
  }

  /** Last received sequence ID for a subscription */
  getLastSeqId(subscriptionId: string): string | undefined {
    this.requireInit();
    return this.wasmClient!.getLastSeqId(subscriptionId) ?? undefined;
  }

  /* ---------------------------------------------------------------- */
  /*  Queries                                                         */
  /* ---------------------------------------------------------------- */

  /**
   * Execute a SQL query with optional parameters.
   *
   * @example
   * ```typescript
   * const result = await client.query('SELECT * FROM users WHERE id = $1', [42]);
   * ```
   */
  async query(sql: string, params?: unknown[]): Promise<QueryResponse> {
    await this.initialize();
    await this.ensureJwtForBasicAuth();
    if (!this.wasmClient) throw new Error('WASM client not initialized');

    const resultStr = params?.length
      ? await this.wasmClient.queryWithParams(sql, JSON.stringify(params))
      : await this.wasmClient.query(sql);

    return JSON.parse(resultStr) as QueryResponse;
  }

  private static escapeSqlStringLiteral(value: string): string {
    return value.replace(/'/g, "''");
  }

  private static wrapExecuteAsUser(sql: string, username: string): string {
    const inner = sql.trim().replace(/;+\s*$/g, '');
    if (!inner) {
      throw new Error('executeAsUser requires a non-empty SQL statement');
    }
    const escapedUsername = KalamDBClient.escapeSqlStringLiteral(username.trim());
    if (!escapedUsername) {
      throw new Error('executeAsUser requires a non-empty username');
    }
    return `EXECUTE AS USER '${escapedUsername}' (${inner})`;
  }

  /**
   * Execute a single SQL statement as another user.
   *
   * Wraps the SQL using:
   * `EXECUTE AS USER 'username' ( <single statement> )`
   */
  async executeAsUser(
    sql: string,
    username: Username | string,
    params?: unknown[],
  ): Promise<QueryResponse> {
    const wrappedSql = KalamDBClient.wrapExecuteAsUser(sql, String(username));
    return this.query(wrappedSql, params);
  }

  /**
   * Execute a SQL query with file uploads (FILE datatype).
   *
   * Uses `fetch()` + multipart/form-data for reliable file upload handling.
   * Auth header is added automatically from WASM client's auth state.
   *
   * @example
   * ```typescript
   * await client.queryWithFiles(
   *   'INSERT INTO users (name, avatar) VALUES ($1, FILE("avatar"))',
   *   { avatar: fileBlob },
   *   ['Alice'],
   * );
   * ```
   */
  async queryWithFiles(
    sql: string,
    files: Record<string, File | Blob>,
    params?: unknown[],
    onProgress?: (progress: UploadProgress) => void,
  ): Promise<QueryResponse> {
    await this.initialize();
    await this.ensureJwtForBasicAuth();

    const formData = new FormData();
    formData.append('sql', sql);

    if (params?.length) {
      formData.append('params', JSON.stringify(params));
    }

    for (const [name, file] of Object.entries(files)) {
      const filename = file instanceof File ? file.name : name;
      formData.append(`file:${name}`, file, filename);
    }

    const headers: Record<string, string> = {};
    const authHeader = buildAuthHeader(this.auth);
    if (authHeader) headers['Authorization'] = authHeader;

    if (onProgress && typeof XMLHttpRequest !== 'undefined') {
      const entries = Object.entries(files);
      const sizes = entries.map(([, file]) => file.size || 0);
      const totalBytes = sizes.reduce((sum, size) => sum + size, 0);

      return await new Promise<QueryResponse>((resolve, reject) => {
        const xhr = new XMLHttpRequest();
        xhr.open('POST', `${this.url}/v1/api/sql`);
        if (authHeader) {
          xhr.setRequestHeader('Authorization', authHeader);
        }
        xhr.upload.onprogress = (event) => {
          if (!event.lengthComputable || totalBytes === 0) return;
          const loaded = Math.min(event.loaded, totalBytes);
          let cumulative = 0;
          let fileIndex = 0;
          while (fileIndex < sizes.length && loaded > cumulative + sizes[fileIndex]) {
            cumulative += sizes[fileIndex];
            fileIndex += 1;
          }
          const currentSize = sizes[fileIndex] || 1;
          const currentLoaded = Math.min(Math.max(loaded - cumulative, 0), currentSize);
          const percent = Math.min(100, Math.round((currentLoaded / currentSize) * 100));
          const [fileName] = entries[fileIndex] || ['unknown'];
          onProgress({
            file_index: fileIndex + 1,
            total_files: entries.length,
            file_name: fileName,
            bytes_sent: currentLoaded,
            total_bytes: currentSize,
            percent,
          });
        };
        xhr.onload = () => {
          try {
            const result = JSON.parse(xhr.responseText) as QueryResponse;
            if (xhr.status < 200 || xhr.status >= 300 || result.status !== 'success') {
              reject(new Error(result.error?.message || `Upload failed: ${xhr.status}`));
              return;
            }
            resolve(result);
          } catch (error) {
            reject(error instanceof Error ? error : new Error('Upload failed'));
          }
        };
        xhr.onerror = () => reject(new Error('Upload failed'));
        xhr.send(formData);
      });
    }

    const response = await fetch(`${this.url}/v1/api/sql`, {
      method: 'POST',
      headers,
      body: formData,
    });

    const result = await response.json();

    if (!response.ok && result.status !== 'success') {
      throw new Error(result.error?.message || `Upload failed: ${response.status}`);
    }

    return result as QueryResponse;
  }

  /* ---------------------------------------------------------------- */
  /*  Convenience Helpers                                             */
  /* ---------------------------------------------------------------- */

  /**
   * Insert data into a table.
   *
   * @example
   * ```typescript
   * await client.insert('todos', { title: 'Buy groceries', completed: false });
   * ```
   */
  async insert(tableName: string, data: Record<string, unknown>): Promise<QueryResponse> {
    await this.initialize();
    await this.ensureJwtForBasicAuth();
    this.requireInit();
    const resultStr = await this.wasmClient!.insert(tableName, JSON.stringify(data));
    return JSON.parse(resultStr) as QueryResponse;
  }

  /**
   * Delete a row by ID.
   *
   * @example
   * ```typescript
   * await client.delete('todos', '123456789');
   * ```
   */
  async delete(tableName: string, rowId: string | number): Promise<void> {
    await this.initialize();
    await this.ensureJwtForBasicAuth();
    this.requireInit();
    await this.wasmClient!.delete(tableName, String(rowId));
  }

  /**
   * Update a row by ID.
   *
   * @example
   * ```typescript
   * await client.update('chat.messages', '123', { status: 'delivered' });
   * ```
   */
  async update(
    tableName: string,
    rowId: string | number,
    data: Record<string, unknown>,
  ): Promise<QueryResponse> {
    const setClauses = Object.entries(data)
      .map(([key, value]) => {
        if (value === null) return `${key} = NULL`;
        if (typeof value === 'string') return `${key} = '${value.replace(/'/g, "''")}'`;
        if (typeof value === 'boolean') return `${key} = ${value}`;
        return `${key} = ${value}`;
      })
      .join(', ');

    return this.query(`UPDATE ${tableName} SET ${setClauses} WHERE id = ${rowId}`);
  }

  /**
   * Query and return the first row, or `null`.
   *
   * @example
   * ```typescript
   * const user = await client.queryOne<User>('SELECT * FROM users WHERE id = $1', [42]);
   * ```
   */
  async queryOne<T extends Record<string, unknown> = Record<string, unknown>>(
    sql: string,
    params?: unknown[],
  ): Promise<T | null> {
    const response = await this.query(sql, params);
    const rows = parseRows<T>(response);
    return rows[0] ?? null;
  }

  /**
   * Query and return all rows.
   *
   * @example
   * ```typescript
   * const msgs = await client.queryAll<Message>('SELECT * FROM chat.messages');
   * ```
   */
  async queryAll<T extends Record<string, unknown> = Record<string, unknown>>(
    sql: string,
    params?: unknown[],
  ): Promise<T[]> {
    const response = await this.query(sql, params);
    return parseRows<T>(response);
  }

  /* ---------------------------------------------------------------- */
  /*  Authentication                                                  */
  /* ---------------------------------------------------------------- */

  /**
   * Login with Basic Auth credentials and switch to JWT.
   *
   * Uses WASM binding to POST to `/v1/api/auth/login`, obtain a JWT token,
   * and automatically update the client to use JWT auth. Must be called
   * before `connect()` for WebSocket subscriptions.
   *
   * @returns The full LoginResponse (access_token, refresh_token, user info, etc.)
   */
  async login(): Promise<LoginResponse> {
    if (this.auth.type !== 'basic') {
      throw new Error('login() requires Basic auth credentials. Use Auth.basic(username, password)');
    }

    await this.initialize();
    if (!this.wasmClient) throw new Error('WASM client not initialized');

    // WASM client's login() returns the full LoginResponse as a JsValue
    const response = (await this.wasmClient.login()) as LoginResponse;

    // Update TypeScript client's auth state to match
    this.auth = { type: 'jwt', token: response.access_token };

    return response;
  }

  /**
   * Refresh the access token using a refresh token.
   *
   * Sends the refresh token to the server to obtain a new access token.
   * The client's auth state is automatically updated.
   *
   * @param refreshToken - The refresh token obtained from a previous login
   * @returns The full LoginResponse with new tokens
   */
  async refreshToken(refreshToken: string): Promise<LoginResponse> {
    await this.initialize();
    if (!this.wasmClient) throw new Error('WASM client not initialized');

    const response = (await this.wasmClient.refresh_access_token(refreshToken)) as LoginResponse;

    // Update TypeScript client's auth state with new access token
    this.auth = { type: 'jwt', token: response.access_token };

    return response;
  }

  /* ---------------------------------------------------------------- */
  /*  Subscriptions                                                   */
  /* ---------------------------------------------------------------- */

  /**
   * Subscribe to real-time changes on a table.
   *
   * @returns An unsubscribe function
   *
   * @example
   * ```typescript
   * const unsub = await client.subscribe('messages', (event) => {
   *   if (event.type === 'change') console.log(event.rows);
   * });
   * await unsub();
   * ```
   */
  async subscribe(
    tableName: string,
    callback: SubscriptionCallback,
    options?: SubscriptionOptions,
  ): Promise<Unsubscribe> {
    return this.subscribeWithSql(`SELECT * FROM ${tableName}`, callback, options);
  }

  /**
   * Subscribe to a SQL query with real-time updates.
   *
   * @example
   * ```typescript
   * const unsub = await client.subscribeWithSql(
   *   'SELECT * FROM chat.messages WHERE conversation_id = 1',
   *   (event) => { ... },
   *   { batch_size: 50 },
   * );
   * ```
   */
  async subscribeWithSql(
    sql: string,
    callback: SubscriptionCallback,
    options?: SubscriptionOptions,
  ): Promise<Unsubscribe> {
    if (this.autoConnect && !this.isConnected()) {
      await this.connect();
    } else {
      await this.initialize();
    }
    if (!this.wasmClient) throw new Error('WASM client not initialized');

    const wrappedCallback = (eventJson: string) => {
      try {
        const event = JSON.parse(eventJson) as ServerMessage;
        callback(event);
      } catch (error) {
        console.error('Failed to parse subscription event:', error);
      }
    };

    const optionsJson = options ? JSON.stringify(options) : undefined;
    const subscriptionId = await this.wasmClient.subscribeWithSql(
      sql,
      optionsJson,
      wrappedCallback as unknown as Function,
    );

    this.subscriptions.set(subscriptionId, {
      id: subscriptionId,
      tableName: sql,
      createdAt: new Date(),
    });

    return async () => {
      await this.unsubscribe(subscriptionId);
    };
  }

  /** Unsubscribe by subscription ID */
  async unsubscribe(subscriptionId: string): Promise<void> {
    if (!this.wasmClient) throw new Error('WASM client not initialized');
    await this.wasmClient.unsubscribe(subscriptionId);
    this.subscriptions.delete(subscriptionId);
  }

  /** Number of active subscriptions */
  getSubscriptionCount(): number {
    return this.subscriptions.size;
  }

  /** Info about all active subscriptions */
  getSubscriptions(): SubscriptionInfo[] {
    return Array.from(this.subscriptions.values());
  }

  /** Whether there is an active subscription for the given table/SQL */
  isSubscribedTo(tableName: string): boolean {
    for (const sub of this.subscriptions.values()) {
      if (sub.tableName === tableName) return true;
    }
    return false;
  }

  /** Unsubscribe from all active subscriptions */
  async unsubscribeAll(): Promise<void> {
    const ids = Array.from(this.subscriptions.keys());
    for (const id of ids) {
      await this.unsubscribe(id);
    }
  }

  /* ---------------------------------------------------------------- */
  /*  Topic Consumer (ergonomic builder API)                          */
  /* ---------------------------------------------------------------- */

  /**
   * Create a consumer handle for a topic.
   *
   * Returns a `ConsumerHandle` with a `.run()` method that continuously
   * polls messages and invokes the handler for each one.
   *
   * @param options - Type-safe consume request options (topic, group_id, etc.)
   * @returns A `ConsumerHandle` with `.run()` and `.stop()` methods
   *
   * @example Auto-ack:
   * ```typescript
   * await client.consumer({
   *   topic: "orders",
   *   group_id: "billing",
   *   auto_ack: true,
   *   batch_size: 10,
   * }).run(async (msg) => {
   *   console.log("Order:", msg.value);
   * });
   * ```
   *
   * @example Manual ack:
   * ```typescript
   * await client.consumer({
   *   topic: "orders",
   *   group_id: "billing",
   * }).run(async (ctx) => {
   *   await processOrder(ctx.message.value);
   *   await ctx.ack();
   * });
   * ```
   */
  consumer(options: ConsumeRequest): ConsumerHandle {
    let running = false;
    let stopRequested = false;

    const handle: ConsumerHandle = {
      run: async (handler: ConsumerHandler): Promise<void> => {
        await this.initialize();
        await this.ensureJwtForBasicAuth();
        if (!this.wasmClient) throw new Error('WASM client not initialized');

        running = true;
        stopRequested = false;

        try {
          while (!stopRequested) {
            // Call the WASM consume method with type-safe options
            const response = (await this.wasmClient.consume(options)) as ConsumeResponse;

            if (response.messages && response.messages.length > 0) {
              for (const message of response.messages) {
                if (stopRequested) break;

                let acked = false;

                const ctx: ConsumeContext = {
                  username: (message.username as Username | undefined) as ConsumeContext['username'],
                  message,
                  ack: async () => {
                    if (acked) return;
                    acked = true;
                    await this.wasmClient!.ack(
                      message.topic,
                      message.group_id,
                      message.partition_id,
                      BigInt(message.offset),
                    );
                  },
                };

                await handler(ctx);

                // Auto-ack if enabled and handler didn't manually ack
                if (options.auto_ack && !acked) {
                  await ctx.ack();
                }
              }
            }

            // If no more messages, pause briefly before next poll
            if (!response.has_more) {
              await new Promise((resolve) => setTimeout(resolve, 1000));
            }
          }
        } finally {
          running = false;
        }
      },

      stop: () => {
        stopRequested = true;
      },
    };

    return handle;
  }

  /**
   * Consume a single batch of messages from a topic (one-shot).
   *
   * For continuous consumption, use `client.consumer(opts).run(handler)`.
   *
   * @param options - Type-safe consume request options
   * @returns Type-safe ConsumeResponse with messages, next_offset, has_more
   *
   * @example
   * ```typescript
   * const result = await client.consumeBatch({
   *   topic: "orders",
   *   group_id: "billing",
   *   batch_size: 5,
   * });
   * console.log(`Got ${result.messages.length} messages`);
   * ```
   */
  async consumeBatch(options: ConsumeRequest): Promise<ConsumeResponse> {
    await this.initialize();
    await this.ensureJwtForBasicAuth();
    if (!this.wasmClient) throw new Error('WASM client not initialized');

    return (await this.wasmClient.consume(options)) as ConsumeResponse;
  }

  /**
   * Acknowledge processed messages on a topic (explicit low-level API).
   *
   * For most use cases, prefer `ctx.ack()` inside a `consumer().run()` handler.
   *
   * @param topic - Topic name
   * @param groupId - Consumer group ID
   * @param partitionId - Partition ID
   * @param uptoOffset - Acknowledge all messages up to and including this offset
   * @returns Type-safe AckResponse
   *
   * @example
   * ```typescript
   * const result = await client.ack("orders", "billing", 0, 42);
   * console.log(result.success, result.acknowledged_offset);
   * ```
   */
  async ack(
    topic: string,
    groupId: string,
    partitionId: number,
    uptoOffset: number,
  ): Promise<AckResponse> {
    await this.initialize();
    await this.ensureJwtForBasicAuth();
    if (!this.wasmClient) throw new Error('WASM client not initialized');

    return (await this.wasmClient.ack(
      topic,
      groupId,
      partitionId,
      BigInt(uptoOffset),
    )) as AckResponse;
  }

  /* ---------------------------------------------------------------- */
  /*  Private helpers                                                 */
  /* ---------------------------------------------------------------- */

  private requireInit(): void {
    if (!this.wasmClient) {
      throw new Error('WASM client not initialized. Call connect() first.');
    }
  }

  private async ensureJwtForBasicAuth(): Promise<void> {
    if (this.auth.type === 'basic') {
      await this.login();
    }
  }

  private async ensureNodeRuntimeCompat(): Promise<void> {
    const isNodeRuntime = typeof process !== 'undefined' && Boolean(process.versions?.node);
    if (!isNodeRuntime) {
      return;
    }

    const runtime = globalThis as unknown as {
      WebSocket?: typeof WebSocket;
      window?: NodeWindowShim;
    };

    if (typeof runtime.WebSocket === 'undefined') {
      try {
        const wsModuleName = 'ws';
        const wsModule = (await import(wsModuleName)) as {
          WebSocket?: typeof WebSocket;
          default?: typeof WebSocket;
        };
        const wsCtor = wsModule.WebSocket ?? wsModule.default;
        if (!wsCtor || typeof wsCtor !== 'function') {
          throw new Error('ws module did not export a WebSocket constructor');
        }
        runtime.WebSocket = wsCtor;
      } catch (error) {
        throw new Error(
          `Node.js runtime is missing WebSocket support. Install "ws" or assign globalThis.WebSocket before creating the client. Cause: ${error}`,
        );
      }
    }

    if (typeof globalThis.fetch !== 'function') {
      throw new Error('Node.js runtime is missing fetch() support. Node.js 18+ is required.');
    }

    if (!this.wasmUrl) {
      try {
        const [{ readFile }, { fileURLToPath }] = await Promise.all([
          import('node:fs/promises'),
          import('node:url'),
        ]);
        const wasmFileUrl = new URL('../.wasm-out/kalam_link_bg.wasm', import.meta.url);
        this.wasmUrl = await readFile(fileURLToPath(wasmFileUrl));
      } catch (error) {
        throw new Error(
          `Node.js runtime could not load bundled WASM file. Build the SDK and ensure dist/.wasm-out/kalam_link_bg.wasm exists. Cause: ${error}`,
        );
      }
    }

    const parsedUrl = new URL(this.url);
    const location = {
      protocol: parsedUrl.protocol,
      hostname: parsedUrl.hostname,
      port: parsedUrl.port,
      href: parsedUrl.href,
    };

    if (!runtime.window) {
      runtime.window = { location, fetch: globalThis.fetch.bind(globalThis) };
      return;
    }

    runtime.window.location = location;
    if (!runtime.window.fetch) {
      runtime.window.fetch = globalThis.fetch.bind(globalThis);
    }
  }
}

/* ================================================================== */
/*  Factory                                                           */
/* ================================================================== */

/**
 * Create a KalamDB client with the given configuration.
 *
 * @example
 * ```typescript
 * import { createClient, Auth } from 'kalam-link';
 *
 * const client = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.basic('admin', 'admin'),
 * });
 * ```
 */
export function createClient(options: ClientOptions): KalamDBClient {
  return new KalamDBClient(options);
}
