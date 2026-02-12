import { formatTimestamp } from '@/lib/formatters';

interface TimestampDisplayProps {
  value: number | string;
  dataType?: string;
}

export function TimestampDisplay({ value, dataType }: TimestampDisplayProps) {
  if (value === null || value === undefined) {
    return <span className="text-muted-foreground italic">null</span>;
  }

  try {
    const formatted = formatTimestamp(value, dataType, 'iso8601-datetime', 'utc');
    return (
      <span className="font-mono text-sm" title={String(value)}>
        {formatted}
      </span>
    );
  } catch (error) {
    return <span className="text-red-500 text-xs">Invalid timestamp</span>;
  }
}
