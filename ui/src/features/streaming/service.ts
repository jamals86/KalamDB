import { api } from "@/lib/api";
import { executeSql } from "@/lib/kalam-client";
import {
  buildStreamingConsumerGroupsQuery,
  buildStreamingOffsetsQuery,
  buildStreamingTopicsQuery,
  type StreamingOffsetsFilter,
} from "@/features/streaming/sql";
import type {
  ConsumedMessageBatch,
  ConsumeApiResponse,
  ConsumeMessagesInput,
  DecodedPayload,
  PayloadDecodeMode,
  StreamingConsumerGroup,
  StreamingMessage,
  StreamingOffset,
  StreamingTopic,
  StreamingTopicRoute,
} from "@/features/streaming/types";

function asString(value: unknown): string {
  if (value === null || value === undefined) {
    return "";
  }
  return String(value);
}

function asNullableString(value: unknown): string | null {
  if (value === null || value === undefined) {
    return null;
  }
  const text = String(value);
  return text.length > 0 ? text : null;
}

function asNumber(value: unknown): number {
  if (typeof value === "number") {
    return Number.isFinite(value) ? value : 0;
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : 0;
  }
  return 0;
}

function asNullableNumber(value: unknown): number | null {
  if (value === null || value === undefined) {
    return null;
  }
  const parsed = asNumber(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function tableIdToString(value: unknown): string {
  if (typeof value === "string") {
    return value;
  }
  if (value && typeof value === "object") {
    const item = value as {
      namespace_id?: unknown;
      namespace?: unknown;
      table_name?: unknown;
      table?: unknown;
    };
    const namespace = asString(item.namespace_id ?? item.namespace);
    const tableName = asString(item.table_name ?? item.table);
    if (namespace && tableName) {
      return `${namespace}.${tableName}`;
    }
  }
  return "";
}

function parseJsonString(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

export function parseTopicRoutes(value: unknown): StreamingTopicRoute[] {
  const fromObject = (route: Record<string, unknown>): StreamingTopicRoute => ({
    tableId: tableIdToString(route.table_id),
    op: asString(route.op),
    payloadMode: asString(route.payload_mode),
    filterExpr: asNullableString(route.filter_expr),
    partitionKeyExpr: asNullableString(route.partition_key_expr),
  });

  if (Array.isArray(value)) {
    return value.filter((item): item is Record<string, unknown> => typeof item === "object" && item !== null).map(fromObject);
  }
  if (typeof value === "string") {
    const parsed = parseJsonString(value);
    if (Array.isArray(parsed)) {
      return parsed
        .filter((item): item is Record<string, unknown> => typeof item === "object" && item !== null)
        .map(fromObject);
    }
  }
  return [];
}

export function mapTopicRows(rows: Record<string, unknown>[]): StreamingTopic[] {
  return rows.map((row) => {
    const routes = parseTopicRoutes(row.routes);
    return {
      topicId: asString(row.topic_id),
      name: asString(row.name || row.topic_id),
      partitions: Math.max(1, asNumber(row.partitions)),
      retentionSeconds: asNullableNumber(row.retention_seconds),
      retentionMaxBytes: asNullableNumber(row.retention_max_bytes),
      routeCount: routes.length,
      routes,
      createdAt: asNullableString(row.created_at),
      updatedAt: asNullableString(row.updated_at),
    };
  });
}

export function mapConsumerGroupRows(rows: Record<string, unknown>[]): StreamingConsumerGroup[] {
  return rows.map((row) => ({
    groupId: asString(row.group_id),
    topicCount: asNumber(row.topic_count),
    partitionCount: asNumber(row.partition_count),
    lastUpdatedAt: asNullableString(row.last_updated_at),
  }));
}

export function mapOffsetRows(rows: Record<string, unknown>[]): StreamingOffset[] {
  return rows.map((row) => {
    const lastAckedOffset = asNumber(row.last_acked_offset);
    return {
      topicId: asString(row.topic_id),
      groupId: asString(row.group_id),
      partitionId: asNumber(row.partition_id),
      lastAckedOffset,
      nextOffset: lastAckedOffset + 1,
      updatedAt: asNullableString(row.updated_at),
    };
  });
}

export async function fetchStreamingTopics(): Promise<StreamingTopic[]> {
  const rows = await executeSql(buildStreamingTopicsQuery());
  return mapTopicRows(rows);
}

export async function fetchStreamingConsumerGroups(): Promise<StreamingConsumerGroup[]> {
  const rows = await executeSql(buildStreamingConsumerGroupsQuery());
  return mapConsumerGroupRows(rows);
}

export async function fetchStreamingOffsets(filters?: StreamingOffsetsFilter): Promise<StreamingOffset[]> {
  const rows = await executeSql(buildStreamingOffsetsQuery(filters));
  return mapOffsetRows(rows);
}

export function buildConsumeRequestBody(input: ConsumeMessagesInput): Record<string, unknown> {
  const body: Record<string, unknown> = {
    topic_id: input.topicId,
    group_id: input.groupId,
    partition_id: input.partitionId,
    limit: input.limit,
    timeout_seconds: input.timeoutSeconds,
  };

  if (input.startMode === "Offset") {
    body.start = { Offset: Math.max(0, input.offset ?? 0) };
    return body;
  }

  body.start = input.startMode;
  return body;
}

export async function consumeTopicMessages(input: ConsumeMessagesInput): Promise<ConsumedMessageBatch> {
  const response = await api.post<ConsumeApiResponse>("/topics/consume", buildConsumeRequestBody(input));
  return {
    messages: response.messages.map((message): StreamingMessage => ({
      topicId: message.topic_id,
      partitionId: Number(message.partition_id ?? 0),
      offset: Number(message.offset ?? 0),
      payloadBase64: message.payload,
      key: message.key ?? null,
      timestampMs: Number(message.timestamp_ms ?? 0),
      username: message.username ?? null,
      op: message.op,
    })),
    nextOffset: Number(response.next_offset ?? 0),
    hasMore: Boolean(response.has_more),
  };
}

function decodeBase64(payloadBase64: string): Uint8Array {
  if (typeof atob === "function") {
    const binary = atob(payloadBase64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }

  const nodeBuffer = (globalThis as { Buffer?: { from: (input: string, encoding: string) => Uint8Array } }).Buffer;
  if (nodeBuffer) {
    return nodeBuffer.from(payloadBase64, "base64");
  }
  throw new Error("No base64 decoder available");
}

function decodeAsUtf8(payloadBase64: string): string {
  const bytes = decodeBase64(payloadBase64);
  return new TextDecoder().decode(bytes);
}

export function decodeTopicPayload(payloadBase64: string, mode: PayloadDecodeMode): DecodedPayload {
  if (mode === "base64") {
    return {
      mode,
      text: null,
      prettyJson: null,
      base64: payloadBase64,
      error: null,
    };
  }

  try {
    const text = decodeAsUtf8(payloadBase64);
    if (mode === "text") {
      return {
        mode,
        text,
        prettyJson: null,
        base64: payloadBase64,
        error: null,
      };
    }

    const parsed = parseJsonString(text);
    if (parsed !== null) {
      return {
        mode,
        text,
        prettyJson: JSON.stringify(parsed, null, 2),
        base64: payloadBase64,
        error: null,
      };
    }

    return {
      mode,
      text,
      prettyJson: null,
      base64: payloadBase64,
      error: null,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : "Failed to decode payload";
    return {
      mode,
      text: null,
      prettyJson: null,
      base64: payloadBase64,
      error: message,
    };
  }
}

