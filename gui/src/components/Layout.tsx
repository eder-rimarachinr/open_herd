import { useCallback, useEffect, useRef, useState } from "react";
import { NavLink, Outlet } from "react-router-dom";
import {
  Globe, Cpu, Server, ShieldCheck, Terminal,
  Database, Settings, Sun, Moon, Play, Square, Power,
  Circle, AlertTriangle,
} from "lucide-react";
import { api, ServiceStatus } from "../api/client";
import { useTheme } from "../hooks/useTheme";
import styles from "./Layout.module.css";

const NAV = [
  { to: "/",         label: "Sites",    Icon: Globe },
  { to: "/php",      label: "PHP",      Icon: Cpu },
  { to: "/nginx",    label: "Nginx",    Icon: Server },
  { to: "/ssl",      label: "SSL",      Icon: ShieldCheck },
  { to: "/database", label: "Database", Icon: Database },
  { to: "/logs",     label: "Logs",     Icon: Terminal },
];

const POLL_INTERVAL = 4000;
const OFFLINE_THRESHOLD = 3;

export default function Layout() {
  const { theme, toggle } = useTheme();
  const [status,      setStatus]      = useState<ServiceStatus | null>(null);
  const [busy,        setBusy]        = useState(false);
  const [initialized, setInitialized] = useState(false);
  const [startError,  setStartError]  = useState<string | null>(null);
  const [, setFailCount] = useState(0);
  const intervalRef = useRef<ReturnType<typeof setInterval>>();

  const fetchStatus = useCallback(async () => {
    try {
      const s = await api.services.status();
      setStatus(s);
      setFailCount(0);
      setInitialized(true);
    } catch {
      setFailCount((n) => {
        const next = n + 1;
        if (next >= OFFLINE_THRESHOLD) { setStatus(null); setInitialized(true); }
        return next;
      });
    }
  }, []);

  useEffect(() => {
    fetchStatus();
    intervalRef.current = setInterval(fetchStatus, POLL_INTERVAL);
    return () => clearInterval(intervalRef.current);
  }, [fetchStatus]);

  async function handleStart() {
    setBusy(true); setStartError(null);
    try { const s = await api.services.start(); setStatus(s); }
    catch (e: any) { setStartError(e.message); }
    finally { setBusy(false); }
  }

  async function handleStop() {
    setBusy(true); setStartError(null);
    try { const s = await api.services.stop(); setStatus(s); }
    catch { /* ignore */ }
    finally { setBusy(false); }
  }

  async function handleQuit() {
    setBusy(true);
    try { await api.daemon.quit(); window.close(); }
    finally { setBusy(false); }
  }

  const daemonUp   = status !== null;
  const allRunning = status?.all_running ?? false;

  return (
    <div className={styles.shell}>
      {/* ── Sidebar ──────────────────────────────────────────────────────── */}
      <aside className={styles.sidebar}>

        <div className={styles.logo}>
          <div className={styles.logoMark}><span>OH</span></div>
          <div className={styles.logoText}>
            <span className={styles.logoName}>Open Herd</span>
            <span className={styles.logoSub}>Local PHP</span>
          </div>
        </div>

        <nav className={styles.nav}>
          <p className={styles.navLabel}>Workspace</p>
          {NAV.map(({ to, label, Icon }) => (
            <NavLink
              key={to} to={to} end={to === "/"}
              className={({ isActive }) =>
                [styles.navItem, isActive ? styles.navActive : ""].join(" ")
              }
            >
              <Icon size={15} strokeWidth={1.8} className={styles.navIcon} />
              {label}
            </NavLink>
          ))}

          <p className={styles.navLabel} style={{ marginTop: 10 }}>System</p>
          <NavLink
            to="/settings"
            className={({ isActive }) =>
              [styles.navItem, isActive ? styles.navActive : ""].join(" ")
            }
          >
            <Settings size={15} strokeWidth={1.8} className={styles.navIcon} />
            Settings
          </NavLink>
        </nav>

        {/* ── Services ─────────────────────────────────────────────────── */}
        <div className={styles.services}>
          <p className={styles.navLabel}>Services</p>

          {!daemonUp ? (
            <div className={styles.offline}>
              <Circle size={7} fill="currentColor" />
              {!initialized ? "Connecting…" : "Daemon offline"}
            </div>
          ) : (
            <ul className={styles.serviceList}>
              <ServiceRow label="Nginx" running={status?.nginx ?? false} />
              {status?.php_versions.map((v) => (
                <ServiceRow
                  key={v.major}
                  label={`PHP ${v.major}`}
                  sublabel={v.version !== v.major ? v.version : undefined}
                  running={v.running}
                />
              ))}
            </ul>
          )}

          {startError && (
            <div className={styles.startError} title={startError}>{startError}</div>
          )}

          <div className={styles.serviceActions}>
            {allRunning ? (
              <button className={styles.btnStop} onClick={handleStop} disabled={busy || !daemonUp}>
                <Square size={11} strokeWidth={2.5} />
                {busy ? "Stopping…" : "Stop all"}
              </button>
            ) : (
              <button className={styles.btnStart} onClick={handleStart} disabled={busy || !daemonUp}>
                <Play size={11} strokeWidth={2.5} fill="currentColor" />
                {busy ? "Starting…" : "Start all"}
              </button>
            )}
          </div>
        </div>

        {/* ── Bottom bar ───────────────────────────────────────────────── */}
        <div className={styles.bottomBar}>
          <button
            className={styles.themeToggle}
            onClick={toggle}
            title={`Switch to ${theme === "dark" ? "light" : "dark"} mode`}
          >
            {theme === "dark" ? <Sun size={15} strokeWidth={1.8} /> : <Moon size={15} strokeWidth={1.8} />}
          </button>
          <button className={styles.btnQuit} onClick={handleQuit} disabled={busy}>
            <Power size={13} strokeWidth={1.8} /> Quit
          </button>
        </div>
      </aside>

      {/* ── Main content ─────────────────────────────────────────────────── */}
      <main className={styles.content}>
        {/* Daemon offline banner */}
        {!daemonUp && initialized && (
          <div className={styles.daemonBanner}>
            <AlertTriangle size={14} />
            Daemon offline — launch the app or run <code>phpenv open</code> in your terminal.
          </div>
        )}
        <Outlet />
      </main>
    </div>
  );
}

function ServiceRow({
  label, sublabel, running,
}: { label: string; sublabel?: string; running: boolean }) {
  return (
    <li className={styles.serviceRow}>
      <span className={running ? styles.dotGreen : styles.dotAmber} />
      <span className={styles.serviceLabel}>
        {label}
        {sublabel && <span className={styles.serviceVersion}>{sublabel}</span>}
      </span>
      <span className={[styles.serviceStatus, running ? styles.statusRunning : styles.statusStopped].join(" ")}>
        {running ? "on" : "off"}
      </span>
    </li>
  );
}
