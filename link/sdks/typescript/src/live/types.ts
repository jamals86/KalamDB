export interface SubscriptionConfig {
  id: string;  // Required: Unique client-generated subscription identifier
  sql: string;
  options?: SubscriptionOptions;
  ws_url?: string;
}

export interface SubscriptionOptions {
  _reserved?: string;
  batch_size?: number;
}

export interface BatchControl {
  batch_num: number;
  total_batches?: number;
  has_more: boolean;
  status: 'loading' | 'loading_batch' | 'ready';
  last_seq_id?: string;
  snapshot_end_seq?: string;
}

export type ChangeEvent = 
  | { type: 'ack', subscription_id: string, total_rows: number, batch_control: BatchControl }
  | { type: 'initial_data_batch', subscription_id: string, rows: any[], batch_control: BatchControl }
  | { type: 'insert', subscription_id: string, rows: any[] }
  | { type: 'update', subscription_id: string, rows: any[], old_rows: any[] }
  | { type: 'delete', subscription_id: string, old_rows: any[] }
  | { type: 'error', subscription_id: string, code: string, message: string };

export interface LiveConnection {
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  subscribe(config: SubscriptionConfig): Promise<string>;
  unsubscribe(subscriptionId: string): Promise<void>;
  listSubscriptions(): Promise<string[]>;
}
