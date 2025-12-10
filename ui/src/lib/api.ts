// API client with cookie-based authentication
// All requests include credentials to send/receive HttpOnly cookies

const API_BASE = "/v1/api";

export interface ApiError {
  error: string;
  message: string;
  details?: Record<string, unknown>;
}

export class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string = API_BASE) {
    this.baseUrl = baseUrl;
  }

  private async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`;
    const response = await fetch(url, {
      ...options,
      credentials: "include", // Important: Send cookies with every request
      headers: {
        "Content-Type": "application/json",
        ...options.headers,
      },
    });

    if (!response.ok) {
      const error: ApiError = await response.json().catch(() => ({
        error: "unknown_error",
        message: `Request failed with status ${response.status}`,
      }));
      throw new ApiRequestError(error, response.status);
    }

    // Handle empty responses
    const text = await response.text();
    if (!text) {
      return {} as T;
    }
    return JSON.parse(text);
  }

  async get<T>(endpoint: string): Promise<T> {
    return this.request<T>(endpoint, { method: "GET" });
  }

  async post<T>(endpoint: string, body?: unknown): Promise<T> {
    return this.request<T>(endpoint, {
      method: "POST",
      body: body ? JSON.stringify(body) : undefined,
    });
  }

  async put<T>(endpoint: string, body?: unknown): Promise<T> {
    return this.request<T>(endpoint, {
      method: "PUT",
      body: body ? JSON.stringify(body) : undefined,
    });
  }

  async delete<T>(endpoint: string): Promise<T> {
    return this.request<T>(endpoint, { method: "DELETE" });
  }
}

export class ApiRequestError extends Error {
  constructor(
    public apiError: ApiError,
    public status: number
  ) {
    super(apiError.message);
    this.name = "ApiRequestError";
  }

  get isUnauthorized(): boolean {
    return this.status === 401;
  }

  get isForbidden(): boolean {
    return this.status === 403;
  }
}

// Singleton instance
export const api = new ApiClient();

// SQL execution types
export interface SqlRequest {
  sql: string;
  namespace?: string;
}

// Query result alias for hooks
export interface QueryResult {
  columns: SqlColumn[];
  rows: unknown[][];
  row_count: number;
  truncated: boolean;
  execution_time_ms: number;
}

export interface SqlColumn {
  name: string;
  data_type: string;
}

export interface SqlResponse {
  columns: SqlColumn[];
  rows: unknown[][];
  row_count: number;
  truncated: boolean;
  execution_time_ms: number;
}

export async function executeSql(sql: string, namespace?: string): Promise<SqlResponse> {
  return api.post<SqlResponse>("/sql", { sql, namespace });
}

// Auth API helpers
export interface LoginRequest {
  username: string;
  password: string;
}

export interface UserInfo {
  id: string;
  username: string;
  role: string;
  email: string | null;
  created_at: string;
  updated_at: string;
}

export interface LoginResponse {
  user: UserInfo;
  expires_at: string;
  access_token: string;
}

export const authApi = {
  login: (credentials: LoginRequest) =>
    api.post<LoginResponse>("/auth/login", credentials),
  
  logout: () => api.post("/auth/logout"),
  
  refresh: () => api.post<LoginResponse>("/auth/refresh"),
  
  me: () => api.get<UserInfo>("/auth/me"),
};
