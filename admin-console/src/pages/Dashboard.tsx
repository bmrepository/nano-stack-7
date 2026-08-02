import { api } from "../api";
import { useApiData } from "../hooks";

export default function Dashboard() {
  const { data: devices, error: devicesError, loading: devicesLoading } = useApiData(api.devices);
  const { data: workspace, error: workspaceError } = useApiData(api.workspace);

  const findingCount = devices?.reduce((sum, d) => sum + d.findings.length, 0) ?? 0;
  const devicesWithFindings = devices?.filter((d) => d.findings.length > 0).length ?? 0;

  return (
    <div>
      <h1>Dashboard</h1>
      <p className="subtitle">Phase 1 PoC — single-workspace overview.</p>

      {(devicesError || workspaceError) && (
        <div className="banner banner-error">
          Couldn't reach the server API ({devicesError ?? workspaceError}). Is the stack running?
        </div>
      )}

      <div className="cards">
        <div className="card">
          <div className="card-value">{devicesLoading ? "…" : devices?.length ?? 0}</div>
          <div className="card-label">Enrolled devices</div>
        </div>
        <div className="card">
          <div className="card-value">{workspace?.workspace_id ?? "—"}</div>
          <div className="card-label">Workspace</div>
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
