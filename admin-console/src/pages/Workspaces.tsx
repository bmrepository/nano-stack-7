import { FormEvent, useState } from "react";
import { api, CLIENT_INSTALLER_URL, Workspace } from "../api";
import { formatUnixTime, useApiData } from "../hooks";

export default function Workspaces() {
  const { data: workspaces, error, loading, refetch } = useApiData(api.workspaces);
  const [newName, setNewName] = useState("");
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [copiedId, setCopiedId] = useState<string | null>(null);

  async function handleCreate(e: FormEvent) {
    e.preventDefault();
    if (!newName.trim()) return;
    setCreating(true);
    setCreateError(null);
    try {
      await api.createWorkspace(newName.trim());
      setNewName("");
      refetch();
    } catch (err) {
      setCreateError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  }

  async function handleDelete(w: Workspace) {
    if (!confirm(`Delete workspace "${w.name}"? This immediately revokes all ${w.device_count} enrolled device(s).`)) {
      return;
    }
    await api.deleteWorkspace(w.id);
    refetch();
  }

  async function handleRenameSubmit(id: string) {
    if (!renameValue.trim()) return;
    await api.renameWorkspace(id, renameValue.trim());
    setRenamingId(null);
    refetch();
  }

  function copyId(id: string) {
    navigator.clipboard.writeText(id).then(() => {
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 1500);
    });
  }

  return (
    <div>
      <h1>Workspaces</h1>
      <p className="subtitle">
        Each workspace's ID doubles as its enrollment credential — copy it into the client's{" "}
        <code>WORKSPACE_ID</code> environment variable during setup. No separate token; the ID
        doesn't exist until you create the workspace here.
      </p>

      {error && <div className="banner banner-error">Couldn't load workspaces ({error}).</div>}

      <form className="inline-form" onSubmit={handleCreate}>
        <input
          placeholder="New workspace name"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
        />
        <button type="submit" disabled={creating || !newName.trim()}>
          {creating ? "Creating…" : "Create workspace"}
        </button>
      </form>
      {createError && <div className="banner banner-error">{createError}</div>}

      {loading && <p>Loading…</p>}

      {workspaces && workspaces.length === 0 && (
        <div className="banner">No workspaces yet — create one above to enroll devices.</div>
      )}

      {workspaces && workspaces.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>Name</th>
              <th>Workspace ID</th>
              <th>Devices</th>
              <th>Created</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {workspaces.map((w) => (
              <tr key={w.id}>
                <td>
                  {renamingId === w.id ? (
                    <input
                      value={renameValue}
                      onChange={(e) => setRenameValue(e.target.value)}
                      onKeyDown={(e) => e.key === "Enter" && handleRenameSubmit(w.id)}
                      autoFocus
                    />
                  ) : (
                    w.name
                  )}
                </td>
                <td className="mono">
                  {w.id}{" "}
                  <button className="link-button" onClick={() => copyId(w.id)}>
                    {copiedId === w.id ? "copied" : "copy"}
                  </button>
                </td>
                <td>{w.device_count}</td>
                <td>{formatUnixTime(w.created_at_unix)}</td>
                <td className="actions">
                  {renamingId === w.id ? (
                    <>
                      <button className="link-button" onClick={() => handleRenameSubmit(w.id)}>
                        Save
                      </button>
                      <button className="link-button" onClick={() => setRenamingId(null)}>
                        Cancel
                      </button>
                    </>
                  ) : (
                    <button
                      className="link-button"
                      onClick={() => {
                        setRenamingId(w.id);
                        setRenameValue(w.name);
                      }}
                    >
                      Rename
                    </button>
                  )}
                  <a className="link-button" href={CLIENT_INSTALLER_URL}>
                    Download client
                  </a>
                  <button className="link-button link-button-danger" onClick={() => handleDelete(w)}>
                    Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
