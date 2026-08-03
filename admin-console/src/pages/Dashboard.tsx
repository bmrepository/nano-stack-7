import { api } from "../api";
import { useApiData } from "../hooks";

export default function Dashboard() {
  const { data: devices, error: devicesError, loading: devicesLoading } = useApiData(api.devices);
  const { data: workspaces, error: workspacesError } = useApiData(api.workspaces);

  const findingCount = devices?.reduce((sum, d) => sum + d.findings.length, 0) ?? 0;
  const devicesWithFindings = devices?.filter((d) => d.findings.length > 0).length ?? 0;

  return (
    <div>
      <h1>Dashboard</h1>
      <p className="subtitle">Phase 1 PoC — overview across all workspaces.</p>

      {(devicesError || workspacesError) && (
        <div className="banner banner-error">
          Couldn't reach the server API ({devicesError ?? workspacesError}). Is the stack running?
        </div>
      )}

      <div className="cards">
        <div className="card">
          <div className="card-value">{devicesLoading ? "…" : devices?.length ?? 0}</div>
          <div className="card-label">Enrolled devices</div>
        </div>
        <div className="card">
          <div className="card-value">{workspaces?.length ?? 0}</div>
          <div className="card-label">Workspaces</div>
        </div>
        <div className="card">
          <div className="card-value">{findingCount}</div>
          <div className="card-label">Open findings</div>
        </div>
        <div className="card">
          <div className="card-value">{devicesWithFindings}</div>
          <div className="card-label">Devices needing attention</div>
        </div>
      </div>
    </div>
  );
}
