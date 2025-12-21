/**
 * Hook to fetch and cache the Arrow→KalamDB data type mappings from system.datatypes
 * 
 * This allows the UI to display user-friendly SQL type names (TEXT, BIGINT, TIMESTAMP)
 * instead of Arrow internal types (Utf8, Int64, Timestamp(Microsecond, None))
 */

import { useState, useEffect, useCallback } from 'react';
import { executeSql } from '@/lib/kalam-client';

export interface DataTypeMapping {
  arrow_type: string;
  kalam_type: string;
  sql_name: string;
  description: string;
}

interface DataTypesState {
  mappings: DataTypeMapping[];
  arrowToSql: Map<string, string>;
  arrowToKalam: Map<string, string>;
  isLoading: boolean;
  error: string | null;
}

// Singleton cache - persists across component mounts
let cachedMappings: DataTypeMapping[] | null = null;
let cachedArrowToSql: Map<string, string> | null = null;
let cachedArrowToKalam: Map<string, string> | null = null;

/**
 * Hook to access data type mappings from system.datatypes
 * 
 * @example
 * ```tsx
 * const { toSqlType, toKalamType, isLoading } = useDataTypes();
 * 
 * // Convert Arrow type to SQL display name
 * const displayType = toSqlType('Utf8'); // Returns 'TEXT'
 * const displayType2 = toSqlType('Int64'); // Returns 'BIGINT'
 * ```
 */
export function useDataTypes() {
  const [state, setState] = useState<DataTypesState>({
    mappings: cachedMappings ?? [],
    arrowToSql: cachedArrowToSql ?? new Map(),
    arrowToKalam: cachedArrowToKalam ?? new Map(),
    isLoading: !cachedMappings,
    error: null,
  });

  const loadMappings = useCallback(async () => {
    // Skip if already cached
    if (cachedMappings) {
      return;
    }

    setState(prev => ({ ...prev, isLoading: true, error: null }));

    try {
      const rows = await executeSql('SELECT * FROM system.datatypes');
      
      const mappings: DataTypeMapping[] = rows.map((row) => ({
        arrow_type: row.arrow_type as string,
        kalam_type: row.kalam_type as string,
        sql_name: row.sql_name as string,
        description: row.description as string,
      }));

      // Build lookup maps
      const arrowToSql = new Map<string, string>();
      const arrowToKalam = new Map<string, string>();
      
      for (const mapping of mappings) {
        arrowToSql.set(mapping.arrow_type, mapping.sql_name);
        arrowToKalam.set(mapping.arrow_type, mapping.kalam_type);
      }

      // Cache for future use
      cachedMappings = mappings;
      cachedArrowToSql = arrowToSql;
      cachedArrowToKalam = arrowToKalam;

      setState({
        mappings,
        arrowToSql,
        arrowToKalam,
        isLoading: false,
        error: null,
      });
    } catch (err) {
      console.error('Failed to load data type mappings:', err);
      setState(prev => ({
        ...prev,
        isLoading: false,
        error: err instanceof Error ? err.message : 'Failed to load data types',
      }));
    }
  }, []);

  useEffect(() => {
    loadMappings();
  }, [loadMappings]);

  /**
   * Convert an Arrow type to its SQL display name
   * Falls back to the original Arrow type if no mapping exists
   */
  const toSqlType = useCallback((arrowType: string): string => {
    // Direct match
    if (state.arrowToSql.has(arrowType)) {
      return state.arrowToSql.get(arrowType)!;
    }
    
    // Pattern matching for parameterized types like Timestamp(Microsecond, None)
    if (/^Timestamp\(/i.test(arrowType)) {
      // Try to find any timestamp mapping
      for (const [key, value] of state.arrowToSql) {
        if (key.startsWith('Timestamp(')) {
          return value; // Return TIMESTAMP for any timestamp variant
        }
      }
      return 'TIMESTAMP';
    }
    
    // Fall back to Arrow type with abbreviation
    return abbreviateArrowType(arrowType);
  }, [state.arrowToSql]);

  /**
   * Convert an Arrow type to its KalamDB internal type name
   * Falls back to the original Arrow type if no mapping exists
   */
  const toKalamType = useCallback((arrowType: string): string => {
    if (state.arrowToKalam.has(arrowType)) {
      return state.arrowToKalam.get(arrowType)!;
    }
    
    // Pattern matching for parameterized types
    if (/^Timestamp\(/i.test(arrowType)) {
      return 'Timestamp';
    }
    
    return arrowType;
  }, [state.arrowToKalam]);

  /**
   * Refresh the mappings from the server
   */
  const refresh = useCallback(async () => {
    cachedMappings = null;
    cachedArrowToSql = null;
    cachedArrowToKalam = null;
    await loadMappings();
  }, [loadMappings]);

  return {
    mappings: state.mappings,
    isLoading: state.isLoading,
    error: state.error,
    toSqlType,
    toKalamType,
    refresh,
  };
}

/**
 * Abbreviate long Arrow type names for display when no mapping is available.
 * E.g., "Timestamp(Microsecond, None)" → "Timestamp(μs)"
 */
function abbreviateArrowType(arrowType: string): string {
  return arrowType
    .replace(/Timestamp\(Microsecond,\s*None\)/gi, 'Timestamp(μs)')
    .replace(/Timestamp\(Millisecond,\s*None\)/gi, 'Timestamp(ms)')
    .replace(/Timestamp\(Nanosecond,\s*None\)/gi, 'Timestamp(ns)')
    .replace(/Timestamp\(Second,\s*None\)/gi, 'Timestamp(s)');
}

/**
 * Non-hook version for use outside of React components
 * Uses cached data if available, otherwise returns the original Arrow type
 */
export function getDisplayType(arrowType: string): string {
  if (cachedArrowToSql?.has(arrowType)) {
    return cachedArrowToSql.get(arrowType)!;
  }
  
  // Pattern matching for parameterized types
  if (/^Timestamp\(/i.test(arrowType)) {
    // Check cache for any timestamp mapping
    if (cachedArrowToSql) {
      for (const [key, value] of cachedArrowToSql) {
        if (key.startsWith('Timestamp(')) {
          return value;
        }
      }
    }
    return 'TIMESTAMP';
  }
  
  return abbreviateArrowType(arrowType);
}
