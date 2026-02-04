interface BooleanDisplayProps {
  value: boolean;
}

export function BooleanDisplay({ value }: BooleanDisplayProps) {
  if (value === null || value === undefined) {
    return <span className="text-muted-foreground italic">null</span>;
  }

  return (
    <span className="font-mono text-sm">
      {value ? 'true' : 'false'}
    </span>
  );
}
