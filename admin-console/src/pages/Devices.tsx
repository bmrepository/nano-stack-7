import { api } from "../api";
import { formatUnixTime, useApiData } from "../hooks";

export default function Devices() {
  const { data: devices, error, loading } = useApiData(api.devices);

  return (
    <div>
      <h1>Devices</h1>
      <p className="subtitle">Devices enrolled in this workspace, from the in-memory device registry.</p>

      {error && <div className="banner banner-error">Couldn't load devices ({error}).</div>}
      {loading && <p>Loading…</p>}

      {devices && devices.length === 0 && (
        <div className="banner">
          No devices enrolled yet. Run the client against this server to enroll one.
        </div>
      )}

      {devices && devices.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>Hostname</th>
              <th>OS</th>
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
