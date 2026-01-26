/**
 * Datatype Display Components
 * 
 * Provides specialized rendering for each KalamDB datatype.
 * Each component handles formatting, styling, and interaction for its specific type.
 */

import { TimestampDisplay } from './TimestampDisplay';
import { DateDisplay } from './DateDisplay';
import { FileDisplay } from './FileDisplay';
import { JsonDisplay } from './JsonDisplay';
import { BooleanDisplay } from './BooleanDisplay';
import { NumberDisplay } from './NumberDisplay';
import { TextDisplay } from './TextDisplay';

export { TimestampDisplay, DateDisplay, FileDisplay, JsonDisplay, BooleanDisplay, NumberDisplay, TextDisplay };

/**
 * Main dispatcher component that routes to the appropriate display component
 * based on the column's data type.
 */
interface CellDisplayProps {
  value: any;
  dataType?: string;
  namespace?: string;
  tableName?: string;
}

export function CellDisplay({ value, dataType, namespace, tableName }: CellDisplayProps) {
  // Handle null/undefined
  if (value === null || value === undefined) {
    return <span className="text-muted-foreground italic">null</span>;
  }

  // Route to appropriate display component based on dataType
  switch (dataType?.toUpperCase()) {
    case 'TIMESTAMP':
    case 'DATETIME':
      return <TimestampDisplay value={value} />;
    
    case 'DATE':
      return <DateDisplay value={value} />;
    
    case 'FILE':
      return <FileDisplay value={value} namespace={namespace} tableName={tableName} />;
    
    case 'JSON':
      return <JsonDisplay value={value} />;
    
    case 'BOOLEAN':
      return <BooleanDisplay value={value} />;
    
    case 'INT':
    case 'BIGINT':
    case 'SMALLINT':
    case 'FLOAT':
    case 'DOUBLE':
    case 'DECIMAL':
      return <NumberDisplay value={value} dataType={dataType as any} />;
    
    case 'TEXT':
    case 'STRING':
    case 'VARCHAR':
      return <TextDisplay value={value} />;
    
    default:
      // Fallback for unknown types
      if (typeof value === 'object') {
        return <JsonDisplay value={value} />;
      }
      return <TextDisplay value={String(value)} />;
  }
}
