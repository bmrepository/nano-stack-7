import { FormEvent, useState } from "react";
import { api } from "../api";
import { setToken } from "../auth";

export default function Setup({ onDone }: { onDone: () => void }) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError(null);

    if (password !== confirm) {
      setError("Passwords do not match.");
      return;
    }

    setSubmitting(true);
    try {
      const session = await api.setup(username, password);
      setToken(session.token);
      onDone();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="auth-screen">
      <form className="auth-card" onSubmit={handleSubmit}>
        <h1>Create the admin account</h1>
        <p className="subtitle">
          This is a new Nano Stack 7 server — create the initial admin account to continue.
        </p>
        {error && <div className="banner banner-error">{error}</div>}
        <label>Username</label>
        <input value={username} onChange={(e) => setUsername(e.target.value)} autoFocus required />
        <label>Password</label>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          required
          minLength={8}
        />
        <label>Confirm password</label>
        <input
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          required
          minLength={8}
        />
        <button type="submit" disabled={submitting}>
          {submitting ? "Creating…" : "Create admin account"}
        </button>
      </form>
    </div>
  );
}
