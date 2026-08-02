import { clearToken, getToken } from "./auth";

export interface Finding {
  plugin_id: string;
  app_name: string;
  installed_version: string;
  recommended_version: string;
  description: string;
}

export interface Device {
  device_id: string;
  workspace_id: string;
  hostname: string;
  os_version: string;
  enrolled_at_unix: number;
  last_checkin_unix: number | null;
  findings: Finding[];
}

export interface WorkspaceInfo {
  workspace_id: string;
  enrollment_token_configured: boolean;
  device_count: number;
}

export interface AuthStatus {
  admin_exists: boolean;
}

export interface Session {
  token: string;
}

async function getJson<T>(path: string): Promise<T> {
  const token = getToken();
  const res = await fetch(path, {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });
  if (res.status === 401) {
    clearToken();
    throw new Error("unauthorized");
  }
  if (!res.ok) {
    throw new Error(`${path} returned ${res.status}`);
  }
  return res.json() as Promise<T>;
}

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `${path} returned ${res.status}`);
  }
  return res.json() as Promise<T>;
}

export const api = {
  devices: () => getJson<Device[]>("/api/devices"),
  workspace: () => getJson<WorkspaceInfo>("/api/workspace"),
  authStatus: () => getJson<AuthStatus>("/api/auth/status"),
  setup: (username: string, password: string) =>
    postJson<Session>("/api/auth/setup", { username, password }),
  login: (username: string, password: string) =>
    postJson<Session>("/api/auth/login", { username, password }),
};
