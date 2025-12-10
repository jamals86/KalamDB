import { useState, useCallback, useRef } from 'react';
import { executeQuery, type QueryResponse } from '../lib/kalam-client';

export interface QueryState {
  isExecuting: boolean;
  result: QueryResponse | null;
  error: string | null;
  executionTime: number | null;
}

export function useQueryExecution() {
  const [state, setState] = useState<QueryState>({
    isExecuting: false,
    result: null,
    error: null,
    executionTime: null,
  });
  
  const abortControllerRef = useRef<AbortController | null>(null);

  const execute = useCallback(async (sql: string) => {
    // Cancel any previous request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    
    abortControllerRef.current = new AbortController();
    const startTime = performance.now();
    
    setState({
      isExecuting: true,
      result: null,
      error: null,
      executionTime: null,
    });

    try {
      const response = await executeQuery(sql);
      const endTime = performance.now();
      
      if (response.status === 'error' && response.error) {
        setState({
          isExecuting: false,
          result: null,
          error: response.error.message,
          executionTime: endTime - startTime,
        });
        throw new Error(response.error.message);
      }
      
      setState({
        isExecuting: false,
        result: response,
        error: null,
        executionTime: response.took ?? (endTime - startTime),
      });
      
      return response;
    } catch (err) {
      const endTime = performance.now();
      const errorMessage = err instanceof Error ? err.message : 'Query execution failed';
      
      setState({
        isExecuting: false,
        result: null,
        error: errorMessage,
        executionTime: endTime - startTime,
      });
      
      throw err;
    }
  }, []);

  const cancel = useCallback(() => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
      abortControllerRef.current = null;
    }
    setState(prev => ({
      ...prev,
      isExecuting: false,
    }));
  }, []);

  const reset = useCallback(() => {
    setState({
      isExecuting: false,
      result: null,
      error: null,
      executionTime: null,
    });
  }, []);

  return {
    ...state,
    execute,
    cancel,
    reset,
  };
}
