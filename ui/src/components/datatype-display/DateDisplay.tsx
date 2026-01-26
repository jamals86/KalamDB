import { formatDate } from '@/lib/formatters';

interface DateDisplayProps {
  value: string | number;
}

export function DateDisplay({ value }: DateDisplayProps) {
  if (value === null || value === undefined) {
    return <span className="text-muted-foreground italic">null</span>;
  }

  try {
    const formatted = formatDate(new Date(value), 'iso8601-date');
    return (
      <span className="font-mono text-sm" title={String(value)}>
        {formatted}
      </span>
    );
  } catch (error) {
    return <span className="text-red-500 text-xs">Invalid date</span>;
  }
}
