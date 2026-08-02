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

async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(path);
  if (!res.ok) {
    throw new Error(`${path} returned ${res.status}`);
  }
  return res.json() as Promise<T>;
}

export const api = {
  devices: () => getJson<Device[]>("/api/devices"),
  workspace: () => getJson<WorkspaceInfo>("/api/workspace"),
};
