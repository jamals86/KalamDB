export interface StreamingOffsetsFilter {
  topicId?: string;
  groupId?: string;
  limit?: number;
}

function escapeSqlLiteral(value: string): string {
  return value.replace(/'/g, "''");
}

export function buildStreamingTopicsQuery(): string {
  return `
    SELECT topic_id, name, partitions, retention_seconds, retention_max_bytes, routes, created_at, updated_at
    FROM system.topics
    ORDER BY topic_id
  `;
}

export function buildStreamingConsumerGroupsQuery(): string {
  return `
    SELECT
      group_id,
      COUNT(*) AS partition_count,
      COUNT(DISTINCT topic_id) AS topic_count,
      MAX(updated_at) AS last_updated_at
    FROM system.topic_offsets
    GROUP BY group_id
    ORDER BY last_updated_at DESC, group_id ASC
    LIMIT 500
  `;
}

export function buildStreamingOffsetsQuery(filters: StreamingOffsetsFilter = {}): string {
  const conditions: string[] = [];
  const limit = Math.min(Math.max(filters.limit ?? 1000, 1), 5000);

  if (filters.topicId) {
    conditions.push(`topic_id = '${escapeSqlLiteral(filters.topicId)}'`);
  }
  if (filters.groupId) {
    conditions.push(`group_id = '${escapeSqlLiteral(filters.groupId)}'`);
  }

  const whereClause = conditions.length > 0 ? `WHERE ${conditions.join(" AND ")}` : "";
  return `
    SELECT topic_id, group_id, partition_id, last_acked_offset, updated_at
    FROM system.topic_offsets
    ${whereClause}
    ORDER BY updated_at DESC, topic_id ASC, group_id ASC, partition_id ASC
    LIMIT ${limit}
  `;
}

export function buildTopicSqlSnippet(topicId: string): string {
  const escapedTopicId = escapeSqlLiteral(topicId);
  return [
    `SELECT * FROM system.topics WHERE topic_id = '${escapedTopicId}';`,
    `SELECT topic_id, group_id, partition_id, last_acked_offset, updated_at`,
    `FROM system.topic_offsets WHERE topic_id = '${escapedTopicId}'`,
    `ORDER BY group_id, partition_id;`,
  ].join("\n");
}

export function buildGroupSqlSnippet(groupId: string): string {
  const escapedGroupId = escapeSqlLiteral(groupId);
  return [
    `SELECT topic_id, group_id, partition_id, last_acked_offset, updated_at`,
    `FROM system.topic_offsets WHERE group_id = '${escapedGroupId}'`,
    `ORDER BY topic_id, partition_id;`,
  ].join("\n");
}

export function buildConsumeSqlSnippet(
  topicId: string,
  groupId: string,
  startMode: "Latest" | "Earliest" | "Offset",
  offset?: number,
  limit: number = 100,
): string {
  const escapedGroupId = escapeSqlLiteral(groupId);
  const safeLimit = Math.min(Math.max(limit, 1), 1000);
  let startClause = "FROM LATEST";
  if (startMode === "Earliest") {
    startClause = "FROM EARLIEST";
  }
  if (startMode === "Offset") {
    startClause = `FROM ${Math.max(0, offset ?? 0)}`;
  }
  return `CONSUME FROM ${topicId} GROUP '${escapedGroupId}' ${startClause} LIMIT ${safeLimit};`;
}
