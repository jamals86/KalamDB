function trimTrailingSlashes(value: string): string {
  return value.replace(/\/+$/, "");
}

function resolveConfiguredOrigin(): string | null {
  const configured = import.meta.env.VITE_API_URL?.trim();
  if (!configured) {
    return null;
  }

  return trimTrailingSlashes(configured);
}

function resolveDefaultOrigin(): string {
  if (import.meta.env.DEV) {
    return "http://localhost:8080";
  }

  if (typeof window !== "undefined") {
    return trimTrailingSlashes(window.location.origin);
  }

  return "http://localhost:8080";
}

export function getBackendOrigin(): string {
  const configuredOrigin = resolveConfiguredOrigin();
  if (configuredOrigin) {
    return configuredOrigin;
  }

  return resolveDefaultOrigin();
}

export function getApiBaseUrl(): string {
  return `${getBackendOrigin()}/v1/api`;
}
