import { customType } from 'drizzle-orm/pg-core';
import { FileRef } from '@kalamdb/client';

export const file = customType<{ data: FileRef | null; driverData: string | null }>({
  dataType() {
    return 'file';
  },
  fromDriver(value: string | null): FileRef | null {
    return FileRef.from(value);
  },
  toDriver(value: FileRef | null): string | null {
    if (!value) return null;
    return JSON.stringify({
      id: value.id,
      sub: value.sub,
      name: value.name,
      size: value.size,
      mime: value.mime,
      sha256: value.sha256,
      shard: value.shard,
    });
  },
});

function parseNumberArray(value: unknown): number[] | null {
  const raw = typeof value === 'string'
    ? (() => {
        try {
          return JSON.parse(value) as unknown;
        } catch {
          return null;
        }
      })()
    : value;

  if (!Array.isArray(raw)) return null;
  const values: number[] = [];
  for (const item of raw) {
    const numberValue = typeof item === 'number' ? item : Number(item);
    if (!Number.isFinite(numberValue)) return null;
    values.push(numberValue);
  }
  return values;
}

function parseByteArray(value: unknown): Uint8Array | null {
  if (value instanceof Uint8Array) return value;
  const values = parseNumberArray(value);
  if (!values) return null;
  for (const byte of values) {
    if (!Number.isInteger(byte) || byte < 0 || byte > 255) return null;
  }
  return Uint8Array.from(values);
}

export const bytes = customType<{ data: Uint8Array | null; driverData: number[] | string | null }>({
  dataType() {
    return 'bytes';
  },
  fromDriver(value: number[] | string | null): Uint8Array | null {
    return parseByteArray(value);
  },
  toDriver(value: Uint8Array | null): number[] | null {
    return value ? Array.from(value) : null;
  },
});

const embeddingColumn = customType<{
  data: number[] | null;
  driverData: string | number[] | null;
  config: { dimensions: number };
  configRequired: true;
}>({
  dataType(config) {
    return `embedding(${config.dimensions})`;
  },
  fromDriver(value: string | number[] | null): number[] | null {
    return parseNumberArray(value);
  },
  toDriver(value: number[] | null): string | null {
    return value ? JSON.stringify(value) : null;
  },
});

export function embedding<TName extends string>(name: TName, dimensions: number) {
  if (!Number.isInteger(dimensions) || dimensions < 1 || dimensions > 8192) {
    throw new Error(`embedding(): dimensions must be between 1 and 8192, got ${dimensions}`);
  }

  return embeddingColumn(name, { dimensions });
}
