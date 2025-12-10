import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  type ReactNode,
} from "react";
import { authApi, type UserInfo, type LoginRequest, ApiRequestError } from "./api";
import { setClientToken, clearClient } from "./kalam-client";

interface AuthContextValue {
  user: UserInfo | null;
  isLoading: boolean;
  isAuthenticated: boolean;
  accessToken: string | null;
  login: (credentials: LoginRequest) => Promise<void>;
  logout: () => Promise<void>;
  refresh: () => Promise<void>;
  error: string | null;
}

const AuthContext = createContext<AuthContextValue | null>(null);

// Refresh token 5 minutes before expiry
const REFRESH_BUFFER_MS = 5 * 60 * 1000;

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<UserInfo | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expiresAt, setExpiresAt] = useState<Date | null>(null);
  const [accessToken, setAccessToken] = useState<string | null>(null);

  // Silent refresh before token expires
  const refresh = useCallback(async () => {
    try {
      const response = await authApi.refresh();
      setUser(response.user);
      setExpiresAt(new Date(response.expires_at));
      setAccessToken(response.access_token);
      // Initialize SDK client with new token
      await setClientToken(response.access_token);
      setError(null);
    } catch (err) {
      // If refresh fails, user is logged out
      setUser(null);
      setExpiresAt(null);
      setAccessToken(null);
      clearClient();
      if (err instanceof ApiRequestError) {
        setError(err.apiError.message);
      }
    }
  }, []);

  // Login
  const login = useCallback(async (credentials: LoginRequest) => {
    setIsLoading(true);
    setError(null);
    try {
      const response = await authApi.login(credentials);
      setUser(response.user);
      setExpiresAt(new Date(response.expires_at));
      setAccessToken(response.access_token);
      // Initialize SDK client with token
      await setClientToken(response.access_token);
    } catch (err) {
      if (err instanceof ApiRequestError) {
        setError(err.apiError.message);
        throw err;
      }
      setError("Login failed");
      throw err;
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Logout
  const logout = useCallback(async () => {
    try {
      await authApi.logout();
    } catch {
      // Ignore logout errors
    } finally {
      setUser(null);
      setExpiresAt(null);
      setAccessToken(null);
      setError(null);
      // Clear SDK client
      clearClient();
    }
  }, []);

  // Initial auth check on mount
  useEffect(() => {
    async function checkAuth() {
      try {
        const userInfo = await authApi.me();
        setUser(userInfo);
        // We don't know expiry from /me, trigger a refresh to get it
        await refresh();
      } catch {
        // Not authenticated
        setUser(null);
      } finally {
        setIsLoading(false);
      }
    }
    checkAuth();
  }, [refresh]);

  // Setup refresh timer
  useEffect(() => {
    if (!expiresAt || !user) return;

    const now = Date.now();
    const expiryTime = expiresAt.getTime();
    const refreshTime = expiryTime - REFRESH_BUFFER_MS;
    const delay = Math.max(0, refreshTime - now);

    if (delay > 0) {
      const timer = setTimeout(() => {
        refresh();
      }, delay);
      return () => clearTimeout(timer);
    } else if (expiryTime > now) {
      // Token is close to expiry, refresh now
      refresh();
    }
  }, [expiresAt, user, refresh]);

  const value: AuthContextValue = {
    user,
    isLoading,
    isAuthenticated: !!user,
    accessToken,
    login,
    logout,
    refresh,
    error,
  };

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used within an AuthProvider");
  }
  return context;
}
