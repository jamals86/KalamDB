interface NumberDisplayProps {
  value: number;
  dataType?: 'INT' | 'BIGINT' | 'SMALLINT' | 'FLOAT' | 'DOUBLE' | 'DECIMAL';
}

export function NumberDisplay({ value, dataType }: NumberDisplayProps) {
  if (value === null || value === undefined) {
    return <span className="text-muted-foreground italic">null</span>;
  }

  // Format based on data type
  let formatted: string;
  if (dataType === 'FLOAT' || dataType === 'DOUBLE' || dataType === 'DECIMAL') {
    formatted = Number(value).toLocaleString(undefined, {
      minimumFractionDigits: 0,
      maximumFractionDigits: 6,
    });
  } else {
    formatted = Number(value).toLocaleString();
  }

  return (
    <span className="font-mono text-sm tabular-nums" title={String(value)}>
      {formatted}
    </span>
  );
}
