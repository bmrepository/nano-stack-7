import { useEffect, useState } from "react";
import { NavLink, Route, Routes } from "react-router-dom";
import Dashboard from "./pages/Dashboard";
import Devices from "./pages/Devices";
import Login from "./pages/Login";
import Setup from "./pages/Setup";
import Workspace from "./pages/Workspace";
import { api } from "./api";
import { clearToken, getToken } from "./auth";

type Gate = "loading" | "setup" | "login" | "authenticated";

export default function App() {
  const [gate, setGate] = useState<Gate>("loading");

  async function checkAuth() {
    try {
      const status = await api.authStatus();
      if (!status.admin_exists) {
        setGate("setup");
        return;
      }
      if (!getToken()) {
        setGate("login");
        return;
      }
      // Confirm the stored token is still valid (e.g. survives a page
      // reload, but not a server restart, since sessions are in-memory).
      await api.workspace();
      setGate("authenticated");
    } catch {
      setGate("login");
    }
  }

  useEffect(() => {
    checkAuth();
  }, []);

  if (gate === "loading") return <div className="auth-screen">Loading…</div>;
  if (gate === "setup") return <Setup onDone={checkAuth} />;
  if (gate === "login") return <Login onDone={checkAuth} />;

  function handleLogout() {
    clearToken();
    setGate("login");
  }

  return (
    <div className="layout">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">N7</span>
          <span className="brand-name">Nano Stack 7</span>
        </div>
        <nav>
          <NavLink to="/" end className={({ isActive }) => (isActive ? "active" : "")}>
            Dashboard
          </NavLink>
          <NavLink to="/devices" className={({ isActive }) => (isActive ? "active" : "")}>
            Devices
          </NavLink>
          <NavLink to="/workspace" className={({ isActive }) => (isActive ? "active" : "")}>
            Workspace
          </NavLink>
          <span className="nav-disabled" title="Phase 3">
            Plugins
          </span>
          <span className="nav-disabled" title="Phase 3">
            Audit Log
          </span>
        </nav>
        <button className="logout-button" onClick={handleLogout}>
          Log out
        </button>
        <div className="sidebar-footer">Phase 1 PoC build</div>
      </aside>
      <main className="content">
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/devices" element={<Devices />} />
          <Route path="/workspace" element={<Workspace />} />
        </Routes>
      </main>
    </div>
  );
}
