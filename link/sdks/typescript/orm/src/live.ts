import type { Table, InferSelectModel, SQLWrapper } from 'drizzle-orm';
import { getTableColumns, sql } from 'drizzle-orm';
import type {
  KalamDBClient,
  LiveRowsOptions,
  RowData,
  ServerMessage,
  SubscriptionOptions,
  Unsubscribe,
} from '@kalamdb/client';
import { normalizeDateValue, normalizeTemporalValue, normalizeTimeValue } from './driver.js';
import { compileInlineQuery } from './sql.js';

type TableQueryOptions = {
  where?: SQLWrapper;
};

type LiveTableOptions<T> = Omit<LiveRowsOptions<T>, 'mapRow'> & TableQueryOptions;

type SubscribeTableOptions = SubscriptionOptions & TableQueryOptions;

export type TableSubscriptionEvent<TTable extends Table> = ServerMessage & {
  rows?: InferSelectModel<TTable>[];
  old_values?: InferSelectModel<TTable>[];
};

function unwrapCellValue(cell: unknown): unknown {
  if (cell == null) return null;
  if (typeof cell === 'object' && 'toJson' in cell) {
    return (cell as { toJson: () => unknown }).toJson();
  }
  return cell;
}

function buildTableQuery(table: Table, where?: SQLWrapper): string {
  const statement = sql`SELECT * FROM ${table}`;
  if (where) {
    statement.append(sql` WHERE ${where}`);
  }

  return compileInlineQuery(statement).sql;
}

type ColumnNormalizer = (value: unknown) => unknown;

function normalizerForColumn(column: { dataType?: unknown; columnType?: unknown }): ColumnNormalizer | undefined {
  const dataType = String(column.dataType ?? '').toLowerCase();
  const columnType = String(column.columnType ?? '').toLowerCase();

  if (columnType.includes('timestamp')) return normalizeTemporalValue;
  if (dataType === 'date' || columnType.includes('date')) return normalizeDateValue;
  if (columnType.includes('time')) return normalizeTimeValue;
  return undefined;
}

function mapTableRow<TTable extends Table>(table: TTable, row: RowData): InferSelectModel<TTable> {
  const columns = getTableColumns(table);
  const mapped: Record<string, unknown> = {};

  for (const [key, col] of Object.entries(columns)) {
    const raw = unwrapCellValue(row[col.name]);
    const normalizer = normalizerForColumn(col);
    const driverValue = raw !== undefined && normalizer ? normalizer(raw) : raw;

    if (raw !== undefined && 'mapFromDriverValue' in col) {
      mapped[key] = (col as { mapFromDriverValue: (v: unknown) => unknown }).mapFromDriverValue(driverValue);
    } else {
      mapped[key] = driverValue ?? null;
    }
  }

  return mapped as InferSelectModel<TTable>;
}

export function liveTable<TTable extends Table>(
  client: KalamDBClient,
  table: TTable,
  callback: (rows: InferSelectModel<TTable>[]) => void,
  options: LiveTableOptions<InferSelectModel<TTable>> = {},
): Promise<Unsubscribe> {
  const { where, ...liveOptions } = options;
  const mapRow = (row: RowData): InferSelectModel<TTable> => mapTableRow(table, row);

  return client.connect().then(() => client.live<InferSelectModel<TTable>>(
    buildTableQuery(table, where),
    callback,
    { ...liveOptions, mapRow },
  ));
}

export function subscribeTable<TTable extends Table>(
  client: KalamDBClient,
  table: TTable,
  callback: (event: TableSubscriptionEvent<TTable>) => void,
  options: SubscribeTableOptions = {},
): Promise<Unsubscribe> {
  const { where, ...subscriptionOptions } = options;
  const mapRow = (row: RowData): InferSelectModel<TTable> => mapTableRow(table, row);

  return client.connect().then(() => client.subscribeWithSql(
    buildTableQuery(table, where),
    (event) => {
      const typedEvent = { ...event } as TableSubscriptionEvent<TTable>;

      if ('rows' in typedEvent && typedEvent.rows) {
        typedEvent.rows = (typedEvent.rows as unknown as RowData[]).map(mapRow);
      }

      if ('old_values' in typedEvent && typedEvent.old_values) {
        typedEvent.old_values = (typedEvent.old_values as unknown as RowData[]).map(mapRow);
      }

      callback(typedEvent);
    },
    subscriptionOptions,
  ));
}
