import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { generateSchema } from '../dist/index.js';

const columnsJson = JSON.stringify([
  { column_name: 'id', ordinal_position: 1, data_type: 'Text', is_nullable: false, is_primary_key: true, default_value: 'None' },
  { column_name: 'body', ordinal_position: 2, data_type: 'Text', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: '_seq', ordinal_position: 3, data_type: 'BigInt', is_nullable: false, is_primary_key: false, default_value: 'None' },
]);

const allTypesColumnsJson = JSON.stringify([
  { column_name: 'id', ordinal_position: 1, data_type: 'BigInt', is_nullable: false, is_primary_key: true, default_value: 'FunctionCall' },
  { column_name: 'flag', ordinal_position: 2, data_type: 'Boolean', is_nullable: false, is_primary_key: false, default_value: 'None' },
  { column_name: 'small_count', ordinal_position: 3, data_type: 'SmallInt', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'count', ordinal_position: 4, data_type: 'Int', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'score', ordinal_position: 5, data_type: 'Double', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'ratio', ordinal_position: 6, data_type: 'Float', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'payload', ordinal_position: 7, data_type: 'Json', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'raw_bytes', ordinal_position: 8, data_type: 'Bytes', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'doc_file', ordinal_position: 9, data_type: 'File', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'vector', ordinal_position: 10, data_type: 'Embedding(384)', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'public_id', ordinal_position: 11, data_type: 'Uuid', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'amount', ordinal_position: 12, data_type: 'Decimal(10, 2)', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'created_on', ordinal_position: 13, data_type: 'Date', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'created_at', ordinal_position: 14, data_type: 'Timestamp', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'updated_at', ordinal_position: 15, data_type: 'DateTime', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'scheduled_for', ordinal_position: 16, data_type: 'Time', is_nullable: true, is_primary_key: false, default_value: 'None' },
  { column_name: 'select', ordinal_position: 17, data_type: 'Text', is_nullable: true, is_primary_key: false, default_value: 'None' },
]);

describe('generateSchema unit behavior', () => {
  it('emits Kalam table factories and keeps hidden columns opt-in', async () => {
    const client = {
      async query(sql) {
        assert.equal(sql, 'SHOW TABLES');
        return {
          results: [{
            named_rows: [
              {
                table_id: 'chat:messages',
                table_name: 'messages',
                namespace_id: 'chat',
                table_type: 'Shared',
                columns: columnsJson,
              },
              {
                table_id: 'chat:inbox',
                table_name: 'inbox',
                namespace_id: 'chat',
                table_type: 'Stream',
                columns: columnsJson,
              },
              {
                table_id: 'chat:all_types',
                table_name: 'all_types',
                namespace_id: 'chat',
                table_type: 'Shared',
                columns: allTypesColumnsJson,
              },
            ],
          }],
        };
      },
    };

    const schema = await generateSchema(client, { includeSystemColumns: true });

    assert.ok(schema.includes('kSystemColumns'));
    assert.ok(schema.includes('kTable'));
    assert.ok(schema.includes('export const chat_messages = kTable.shared("chat.messages"'));
    assert.ok(schema.includes('export const chat_messagesConfig = getKalamTableConfig(chat_messages)!;'));
    assert.ok(schema.includes('export const chat_inbox = kTable.stream("chat.inbox"'));
    assert.ok(schema.includes('...kSystemColumns(["_seq","_deleted"] as const),'));
    assert.ok(schema.includes('...kSystemColumns(["_seq"] as const),'));
    assert.ok(schema.includes('{ systemColumns: true }'));
    assert.ok(schema.includes('export type ChatMessages = typeof chat_messages.$inferSelect;'));
  });

  it('maps KalamDB datatypes to Drizzle column builders', async () => {
    const client = {
      async query(sql) {
        assert.equal(sql, 'SHOW TABLES');
        return {
          results: [{
            named_rows: [{
              table_id: 'chat:all_types',
              table_name: 'all_types',
              namespace_id: 'chat',
              table_type: 'Shared',
              columns: allTypesColumnsJson,
            }],
          }],
        };
      },
    };

    const schema = await generateSchema(client);

    assert.ok(schema.includes("import { bytes, embedding, file, getKalamTableConfig, kTable } from '@kalamdb/orm';"));
    assert.ok(schema.includes('boolean("flag")'));
    assert.ok(schema.includes('smallint("small_count")'));
    assert.ok(schema.includes('integer("count")'));
    assert.ok(schema.includes('text("id").default(sql``).primaryKey()'));
    assert.ok(schema.includes('doublePrecision("score")'));
    assert.ok(schema.includes('real("ratio")'));
    assert.ok(schema.includes('jsonb("payload")'));
    assert.ok(schema.includes('bytes("raw_bytes")'));
    assert.ok(schema.includes('file("doc_file")'));
    assert.ok(schema.includes('embedding("vector", 384)'));
    assert.ok(schema.includes('uuid("public_id")'));
    assert.ok(schema.includes('numeric("amount", { precision: 10, scale: 2 })'));
    assert.ok(schema.includes('date("created_on", { mode: \'date\' })'));
    assert.ok(schema.includes('timestamp("created_at", { mode: \'date\' })'));
    assert.ok(schema.includes('timestamp("updated_at", { mode: \'date\' })'));
    assert.ok(schema.includes('time("scheduled_for")'));
    assert.ok(schema.includes('select: text("select")'));
  });
});
