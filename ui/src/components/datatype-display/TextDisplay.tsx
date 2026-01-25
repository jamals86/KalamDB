interface TextDisplayProps {
  value: string;
  maxLength?: number;
}

export function TextDisplay({ value, maxLength = 50 }: TextDisplayProps) {
  if (value === null || value === undefined) {
    return <span className="text-muted-foreground italic">null</span>;
  }

  const stringValue = String(value);
  const isTruncated = stringValue.length > maxLength;

  return (
    <span className="font-mono text-sm" title={isTruncated ? stringValue : undefined}>
      {isTruncated ? `${stringValue.slice(0, maxLength)}...` : stringValue}
    </span>
  );
}
