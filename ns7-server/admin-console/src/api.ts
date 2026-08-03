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

export interface Workspace {
  id: string;
  name: string;
  created_at_unix: number;
  device_count: number;
}

export interface AuthStatus {
  admin_exists: boolean;
}

export interface Session {
  token: string;
}

// Matches the asset name published by .github/workflows/release-client.yml.
export const CLIENT_INSTALLER_URL =
  "https://github.com/bmrepository/nano-stack-7/releases/latest/download/nano-stack-7-client-installer.msi";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const token = getToken();
  const res = await fetch(path, {
    ...init,
    headers: {
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...(init?.body ? { "Content-Type": "application/json" } : {}),
      ...init?.headers,
    },
  });
  if (res.status === 401) {
    clearToken();
    throw new Error("unauthorized");
  }
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `${path} returned ${res.status}`);
  }
  if (res.status === 204) {
    return undefined as T;
  }
  return res.json() as Promise<T>;
}

export const api = {
  devices: () => request<Device[]>("/api/devices"),
  workspaces: () => request<Workspace[]>("/api/workspaces"),
  createWorkspace: (name: string) =>
    request<Workspace>("/api/workspaces", { method: "POST", body: JSON.stringify({ name }) }),
  renameWorkspace: (id: string, name: string) =>
    request<void>(`/api/workspaces/${id}`, { method: "PATCH", body: JSON.stringify({ name }) }),
  deleteWorkspace: (id: string) => request<void>(`/api/workspaces/${id}`, { method: "DELETE" }),
  authStatus: () => request<AuthStatus>("/api/auth/status"),
  setup: (username: string, password: string) =>
    request<Session>("/api/auth/setup", { method: "POST", body: JSON.stringify({ username, password }) }),
  login: (username: string, password: string) =>
    request<Session>("/api/auth/login", { method: "POST", body: JSON.stringify({ username, password }) }),
};
