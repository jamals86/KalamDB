/**
 * UI Configuration Settings
 * 
 * Centralized configuration for the KalamDB Admin UI.
 * Modify these settings to customize display behavior.
 */

// =============================================================================
// DATE & TIME FORMATTING
// =============================================================================

/**
 * Default timestamp format used throughout the UI.
 * 
 * Options:
 * - 'iso8601'          → 2024-12-14T15:30:45.123Z
 * - 'iso8601-date'     → 2024-12-14
 * - 'iso8601-datetime' → 2024-12-14T15:30:45Z (no milliseconds)
 * - 'locale'           → Uses browser's locale (e.g., 12/14/2024, 3:30:45 PM)
 * - 'locale-short'     → Uses browser's short locale (e.g., 12/14/24 3:30 PM)
 * - 'relative'         → 2 hours ago, 5 minutes ago
 * - 'unix-ms'          → Raw milliseconds
 * - 'unix-sec'         → Raw seconds
 */
export type TimestampFormat = 
  | 'iso8601'
  | 'iso8601-date'
  | 'iso8601-datetime'
  | 'locale'
  | 'locale-short'
  | 'relative'
  | 'unix-ms'
  | 'unix-sec';

export const DEFAULT_TIMESTAMP_FORMAT: TimestampFormat = 'iso8601-datetime';

/**
 * Timezone for displaying timestamps.
 * - 'local'  → Use browser's local timezone
 * - 'utc'    → Display in UTC
 * - Or a specific IANA timezone like 'America/New_York'
 */
export type TimezoneOption = 'local' | 'utc' | string;

export const DEFAULT_TIMEZONE: TimezoneOption = 'local';

/**
 * Whether to show milliseconds in timestamp display.
 */
export const SHOW_MILLISECONDS = false;

// =============================================================================
// TABLE DISPLAY
// =============================================================================

/**
 * Maximum rows to display in the results table (for performance).
 */
export const MAX_DISPLAY_ROWS = 10000;

/**
 * Default page size for paginated tables.
 */
export const DEFAULT_PAGE_SIZE = 50;

/**
 * Available page size options.
 */
export const PAGE_SIZE_OPTIONS = [25, 50, 100, 200];

// =============================================================================
// DATA TYPE DETECTION
// =============================================================================

/**
 * Data types that should be formatted as timestamps.
 * These patterns match the data_type field from the schema.
 * 
 * KalamDB uses Arrow DataTypes which serialize as:
 * - "Timestamp(Microsecond, None)"
 * - "Timestamp(Millisecond, None)"
 * - "Timestamp(Nanosecond, None)"
 * - "Date32"
 * - "Date64"
 */
export const TIMESTAMP_DATA_TYPES = [
  /^Timestamp\(/i,
  /^Date32$/i,
  /^Date64$/i,
];

/**
 * Check if a data type represents a timestamp/date.
 */
export function isTimestampType(dataType: string): boolean {
  return TIMESTAMP_DATA_TYPES.some((pattern) => pattern.test(dataType));
}

/**
 * Extract the time unit from a Timestamp data type.
 * Returns: 'microsecond' | 'millisecond' | 'nanosecond' | 'second' | null
 */
export function extractTimestampUnit(dataType: string): 'microsecond' | 'millisecond' | 'nanosecond' | 'second' | null {
  const match = dataType.match(/^Timestamp\((\w+)/i);
  if (match) {
    const unit = match[1].toLowerCase();
    if (unit === 'microsecond') return 'microsecond';
    if (unit === 'millisecond') return 'millisecond';
    if (unit === 'nanosecond') return 'nanosecond';
    if (unit === 'second') return 'second';
  }
  // Date32/Date64 are in days
  if (/^Date32$/i.test(dataType) || /^Date64$/i.test(dataType)) {
    return null; // Handled specially
  }
  return null;
}

// =============================================================================
// NUMERIC FORMATTING
// =============================================================================

/**
 * Maximum decimal places for floating point numbers.
 */
export const MAX_DECIMAL_PLACES = 6;

// =============================================================================
// UI THEME
// =============================================================================

/**
 * Color classes for different data types in the table.
 */
export const DATA_TYPE_COLORS: Record<string, string> = {
  // Numeric types
  Int8: 'text-blue-600',
  Int16: 'text-blue-600',
  Int32: 'text-blue-600',
  Int64: 'text-blue-600',
  UInt8: 'text-blue-600',
  UInt16: 'text-blue-600',
  UInt32: 'text-blue-600',
  UInt64: 'text-blue-600',
  Float16: 'text-cyan-600',
  Float32: 'text-cyan-600',
  Float64: 'text-cyan-600',
  // String types
  Utf8: 'text-green-600',
  LargeUtf8: 'text-green-600',
  // Boolean
  Boolean: 'text-purple-600',
  // Binary
  Binary: 'text-orange-600',
  LargeBinary: 'text-orange-600',
  // Temporal
  Date32: 'text-amber-600',
  Date64: 'text-amber-600',
  // Default for timestamps
  Timestamp: 'text-amber-600',
};

/**
 * Get color class for a data type.
 */
export function getDataTypeColor(dataType: string): string {
  // Check for exact match first
  if (dataType in DATA_TYPE_COLORS) {
    return DATA_TYPE_COLORS[dataType];
  }
  // Check for Timestamp pattern
  if (/^Timestamp\(/i.test(dataType)) {
    return DATA_TYPE_COLORS['Timestamp'];
  }
  // Check for numeric types (partial match)
  if (/^(Int|UInt)(8|16|32|64)$/i.test(dataType)) {
    return 'text-blue-600';
  }
  // Check for float types
  if (/^Float(16|32|64)$/i.test(dataType)) {
    return 'text-cyan-600';
  }
  // Check for string types
  if (/^(Utf8|LargeUtf8|Str|String)$/i.test(dataType)) {
    return 'text-green-600';
  }
  // Check for boolean
  if (/^Bool(ean)?$/i.test(dataType)) {
    return 'text-purple-600';
  }
  // Check for binary types
  if (/^(Binary|LargeBinary)$/i.test(dataType)) {
    return 'text-orange-600';
  }
  // Check for date types
  if (/^Date(32|64)$/i.test(dataType)) {
    return DATA_TYPE_COLORS['Date32'];
  }
  // Default
  return 'text-gray-500';
}

/**
 * Abbreviate long data type names for display.
 * E.g., "Timestamp(Microsecond, None)" → "Timestamp(μs)"
 */
export function abbreviateDataType(dataType: string): string {
  return dataType
    .replace(/Timestamp\(Microsecond,\s*None\)/gi, 'Timestamp(μs)')
    .replace(/Timestamp\(Millisecond,\s*None\)/gi, 'Timestamp(ms)')
    .replace(/Timestamp\(Nanosecond,\s*None\)/gi, 'Timestamp(ns)')
    .replace(/Timestamp\(Second,\s*None\)/gi, 'Timestamp(s)');
}
