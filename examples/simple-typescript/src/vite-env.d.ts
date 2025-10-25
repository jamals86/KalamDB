/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_KALAMDB_URL: string
  readonly VITE_KALAMDB_API_KEY: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}

// Declare module for WASM imports
declare module '../wasm/kalam_link.js' {
  export default function init(input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module): Promise<void>;
  
  export class KalamClient {
    constructor(url: string, api_key: string);
    connect(): Promise<void>;
    disconnect(): Promise<void>;
    isConnected(): boolean;
    insert(table_name: string, data: string): Promise<string>;
    delete(table_name: string, row_id: string): Promise<string>;
    query(sql: string): Promise<string>;
    subscribe(table_name: string, callback: (event: string) => void): Promise<string>;
    unsubscribe(subscription_id: string): Promise<void>;
  }
}
