export interface StreamingTopicRoute {
  tableId: string;
  op: string;
  payloadMode: string;
  filterExpr: string | null;
  partitionKeyExpr: string | null;
}

export interface StreamingTopic {
  topicId: string;
  name: string;
  partitions: number;
  retentionSeconds: number | null;
  retentionMaxBytes: number | null;
  routeCount: number;
  routes: StreamingTopicRoute[];
  createdAt: string | null;
  updatedAt: string | null;
}

export interface StreamingConsumerGroup {
  groupId: string;
  topicCount: number;
  partitionCount: number;
  lastUpdatedAt: string | null;
}

export interface StreamingOffset {
  topicId: string;
  groupId: string;
  partitionId: number;
  lastAckedOffset: number;
  nextOffset: number;
  updatedAt: string | null;
}

export type ConsumeStartMode = "Latest" | "Earliest" | "Offset";

export interface ConsumeMessagesInput {
  topicId: string;
  groupId: string;
  partitionId: number;
  startMode: ConsumeStartMode;
  offset?: number;
  limit: number;
  timeoutSeconds?: number;
}

export interface ConsumeApiMessage {
  topic_id: string;
  partition_id: number;
  offset: number;
  payload: string;
  key: string | null;
  timestamp_ms: number;
  username?: string | null;
  op: string;
}

export interface ConsumeApiResponse {
  messages: ConsumeApiMessage[];
  next_offset: number;
  has_more: boolean;
}

export interface StreamingMessage {
  topicId: string;
  partitionId: number;
  offset: number;
  payloadBase64: string;
  key: string | null;
  timestampMs: number;
  username: string | null;
  op: string;
}

export interface ConsumedMessageBatch {
  messages: StreamingMessage[];
  nextOffset: number;
  hasMore: boolean;
}

export type PayloadDecodeMode = "auto-json" | "text" | "base64";

export interface DecodedPayload {
  mode: PayloadDecodeMode;
  text: string | null;
  prettyJson: string | null;
  base64: string;
  error: string | null;
}

