import { Check, X } from 'lucide-react';

interface BooleanDisplayProps {
  value: boolean;
}

export function BooleanDisplay({ value }: BooleanDisplayProps) {
  if (value === null || value === undefined) {
    return <span className="text-muted-foreground italic">null</span>;
  }

  if (value === true) {
    return (
      <div className="flex items-center gap-1">
        <Check className="h-4 w-4 text-green-500" />
        <span className="text-green-600 dark:text-green-400 font-medium">true</span>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-1">
      <X className="h-4 w-4 text-red-500" />
      <span className="text-red-600 dark:text-red-400 font-medium">false</span>
    </div>
  );
}
