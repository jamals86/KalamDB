/**
 * Type definitions for kalam-link SDK
 *
 * Model types are auto-generated from Rust via tsify-next and exported from the
 * WASM bindings. Client-only types (no Rust equivalent) are defined here.
 */

/* ================================================================== */
/*  Re-exported WASM-generated types (single source of truth in Rust) */
/* ================================================================== */

/**
 * `JsonValue` is used by tsify for `serde_json::Value` fields.
 * In TypeScript, this corresponds to any valid JSON value.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type JsonValue = any;

import type { FieldFlag as WasmFieldFlag } from '../.wasm-out/kalam_link.js';

export type {
  AckResponse,
  BatchControl,
  BatchStatus,
  ChangeTypeRaw,
  ConsumeMessage,
  ConsumeRequest,
  ConsumeResponse,
  ErrorDetail,
  HealthCheckResponse,
  HttpVersion,
  KalamDataType,
  LoginResponse,
  LoginUserInfo,
  QueryResponse,
  QueryResult,
  ResponseStatus,
  SchemaField,
  SeqId,
  ServerMessage,
  SubscriptionOptions,
  TimestampFormat,
  UploadProgress,
} from '../.wasm-out/kalam_link.js';

export type FieldFlag = WasmFieldFlag;
export type FieldFlags = FieldFlag[];

/* ================================================================== */
/*  Convenience enums (for runtime values, not just types)            */
/* ================================================================== */

/**
 * Message type enum for WebSocket subscription events.
 * Use these constants when comparing `ServerMessage.type` at runtime.
 */
export enum MessageType {
  SubscriptionAck = 'subscription_ack',
  InitialDataBatch = 'initial_data_batch',
  Change = 'change',
  Error = 'error',
}

/**
 * Change type enum for live subscription change events.
 * Runtime-usable values matching `ChangeTypeRaw`.
 */
export enum ChangeType {
  Insert = 'insert',
  Update = 'update',
  Delete = 'delete',
}

/* ================================================================== */
/*  Client-only types (TypeScript SDK specific, no Rust equivalent)   */
/* ================================================================== */

/**
 * Subscription callback function type
 */
export type SubscriptionCallback = (event: import('../.wasm-out/kalam_link.js').ServerMessage) => void;

/**
 * Typed subscription callback for convenience.
 *
 * @example
 * ```typescript
 * interface ChatMessage { id: string; content: string; sender: string }
 *
 * const handleEvent: TypedSubscriptionCallback<ChatMessage> = (event) => {
 *   if (event.type === 'change' && event.rows) {
 *     const messages: ChatMessage[] = event.rows;
 *   }
 * };
 * ```
 */
export type TypedSubscriptionCallback<T extends Record<string, unknown>> = (
  event: import('../.wasm-out/kalam_link.js').ServerMessage & { rows?: T[]; old_values?: T[] },
) => void;

/**
 * Function to unsubscribe from a subscription (Firebase/Supabase style)
 */
export type Unsubscribe = () => Promise<void>;

/**
 * Information about an active subscription
 */
export interface SubscriptionInfo {
  /** Unique subscription ID */
  id: string;
  /** Table name or SQL query being subscribed to */
  tableName: string;
  /** Timestamp when subscription was created */
  createdAt: Date;
}

/* ================================================================== */
/*  Consumer Types (TypeScript SDK specific)                          */
/* ================================================================== */

/**
 * Type-safe username wrapper.
 *
 * Prevents confusion between usernames and other string identifiers.
 * Use `.toString()` or template literals to get the raw string value.
 */
export type Username = string & { readonly __brand: unique symbol };

/**
 * Create a type-safe Username from a raw string.
 */
export function Username(value: string): Username {
  return value as Username;
}

/**
 * Context passed to the consumer handler callback.
 *
 * Contains the username of the user who triggered the event and
 * a method for manual message acknowledgment.
 */
export interface ConsumeContext {
  /** Username of the user who produced this message/event */
  readonly username: Username | undefined;
  /** The consumed message with decoded payload */
  readonly message: import('../.wasm-out/kalam_link.js').ConsumeMessage;
  /** Acknowledge the current message (manual ack mode) */
  ack: () => Promise<void>;
}

/**
 * Consumer handler function signature.
 *
 * @param ctx - Context with username, message, and ack method
 */
export type ConsumerHandler = (
  ctx: ConsumeContext,
) => Promise<void>;

/**
 * Handle returned by `client.consumer()`. Call `.run()` to start consuming.
 */
export interface ConsumerHandle {
  /**
   * Start consuming messages, invoking the handler for each message.
   *
   * If `autoAck` was set in the consumer options, messages are acknowledged
   * automatically after the handler resolves. Otherwise, call `ctx.ack()`
   * inside the handler for manual acknowledgment.
   *
   * The returned promise resolves when `stop()` is called or the consumer
   * encounters an unrecoverable error.
   *
   * @example Auto-ack:
   * ```typescript
   * await client.consumer({ topic: "orders", group_id: "billing", auto_ack: true })
   *   .run(async (ctx) => {
   *     console.log("Order from:", ctx.username);
   *     console.log("Data:", ctx.message.value);
   *   });
   * ```
   *
   * @example Manual ack:
   * ```typescript
   * await client.consumer({ topic: "orders", group_id: "billing" })
   *   .run(async (ctx) => {
   *     await processOrder(ctx.message.value);
   *     await ctx.ack();
   *   });
   * ```
   */
  run: (handler: ConsumerHandler) => Promise<void>;

  /** Stop consuming (signals the run loop to break after the current batch) */
  stop: () => void;
}

/* ================================================================== */
/*  Client Options                                                    */
/* ================================================================== */

/**
 * Configuration options for KalamDB client
 *
 * @example
 * ```typescript
 * const client = createClient({
 *   url: 'http://localhost:8080',
 *   auth: Auth.basic('admin', 'admin')
 * });
 * ```
 */
export interface ClientOptions {
  /** Server URL (e.g., 'http://localhost:8080') */
  url: string;
  /** Authentication credentials (type-safe) */
  auth: import('./auth.js').AuthCredentials;
  /**
   * Explicit URL or buffer for the WASM file.
   * - Browser: string URL like '/wasm/kalam_link_bg.wasm'
   * - Node.js: BufferSource (fs.readFileSync result)
   * Required in bundled environments where import.meta.url doesn't resolve.
   */
  wasmUrl?: string | BufferSource;
  /**
   * Automatically connect (and login if using Basic auth) the first time
   * a WebSocket operation is needed (subscribe, subscribeWithSql).
   *
   * Defaults to `true`. Set to `false` if you want to control the
   * connection lifecycle manually via `connect()` / `disconnect()`.
   */
  autoConnect?: boolean;
}

/* ================================================================== */
/*  Agent Runtime Types                                               */
/* ================================================================== */

export type {
  AgentContext,
  AgentFailureContext,
  AgentFailureHandler,
  AgentLLMAdapter,
  AgentLLMContext,
  AgentLLMInput,
  AgentLLMMessage,
  AgentLLMRole,
  AgentRetryPolicy,
  AgentRowParser,
  AgentRunKeyFactory,
  LangChainChatModelLike,
  RunAgentOptions,
  RunConsumerOptions,
} from './agent.js';
