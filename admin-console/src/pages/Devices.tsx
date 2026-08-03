import { api } from "../api";
import { formatUnixTime, useApiData } from "../hooks";

export default function Devices() {
  const { data: devices, error: devicesError, loading } = useApiData(api.devices);
  const { data: workspaces } = useApiData(api.workspaces);

  const workspaceName = (id: string) => workspaces?.find((w) => w.id === id)?.name ?? id;

  return (
    <div>
      <h1>Devices</h1>
      <p className="subtitle">Devices enrolled across all workspaces, from the in-memory device registry.</p>

      {devicesError && <div className="banner banner-error">Couldn't load devices ({devicesError}).</div>}
      {loading && <p>Loading…</p>}

      {devices && devices.length === 0 && (
        <div className="banner">
          No devices enrolled yet. Create a workspace, then run the client against this server with its
          Workspace ID to enroll one.
        </div>
      )}

      {devices && devices.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>Hostname</th>
              <th>OS</th>
              <th>Workspace</th>
              <th>Device ID</th>
              <th>Enrolled</th>
              <th>Last check-in</th>
              <th>Findings</th>
            </tr>
          </thead>
          <tbody>
            {devices.map((d) => (
              <tr key={d.device_id}>
                <td>{d.hostname}</td>
                <td>{d.os_version}</td>
                <td>{workspaceName(d.workspace_id)}</td>
                <td className="mono">{d.device_id}</td>
                <td>{formatUnixTime(d.enrolled_at_unix)}</td>
                <td>{formatUnixTime(d.last_checkin_unix)}</td>
                <td>
                  {d.findings.length === 0 ? (
                    <span className="badge badge-ok">none</span>
                  ) : (
                    d.findings.map((f) => (
                      <span className="badge badge-warn" key={f.plugin_id} title={f.description}>
                        {f.app_name} outdated
                      </span>
                    ))
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
