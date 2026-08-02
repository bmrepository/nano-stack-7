import { NavLink, Route, Routes } from "react-router-dom";
import Dashboard from "./pages/Dashboard";
import Devices from "./pages/Devices";
import Workspace from "./pages/Workspace";

export default function App() {
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
