import { api } from "../api";
import { useApiData } from "../hooks";

export default function Workspace() {
  const { data: workspace, error, loading } = useApiData(api.workspace);

  return (
    <div>
      <h1>Workspace</h1>
      <p className="subtitle">
        Single hardcoded workspace for the PoC — the real Workspace Manager (multi-workspace, key
        rotation, cascade deletion) is Phase 3 scope.
      </p>

      {error && <div className="banner banner-error">Couldn't load workspace info ({error}).</div>}
      {loading && <p>Loading…</p>}

      {workspace && (
        <div className="cards">
          <div className="card">
            <div className="card-value mono">{workspace.workspace_id}</div>
            <div className="card-label">Workspace ID</div>
          </div>
          <div className="card">
            <div className="card-value">{workspace.enrollment_token_configured ? "Configured" : "Dev default"}</div>
            <div className="card-label">Enrollment token</div>
          </div>
          <div className="card">
            <div className="card-value">{workspace.device_count}</div>
            <div className="card-label">Devices</div>
          </div>
        </div>
      )}
    </div>
  );
}
