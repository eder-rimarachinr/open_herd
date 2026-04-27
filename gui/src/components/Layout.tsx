import { useCallback, useEffect, useRef, useState } from "react";
import { NavLink, Outlet } from "react-router-dom";
import { api, ServiceStatus } from "../api/client";
import styles from "./Layout.module.css";

const nav = [
  { to: "/", label: "Sites", icon: "🌐" },
  { to: "/php", label: "PHP", icon: "🐘" },
  { to: "/nginx", label: "Nginx", icon: "⚙️" },
  { to: "/database", label: "Database", icon: "🗄️" },
  { to: "/ssl", label: "SSL", icon: "🔒" },
];

const POLL_INTERVAL = 4000;

export default function Layout() {
  const [status, setStatus] = useState<ServiceStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [initialized, setInitialized] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval>>();

  const fetchStatus = useCallback(async () => {
    try {
      const s = await api.services.status();
      setStatus(s);
    } catch {
      setStatus(null);
    } finally {
      setInitialized(true);
    }
  }, []);

  useEffect(() => {
    fetchStatus();
    intervalRef.current = setInterval(fetchStatus, POLL_INTERVAL);
    return () => clearInterval(intervalRef.current);
  }, [fetchStatus]);

  async function handleStart() {
    setBusy(true);
    setStartError(null);
    try {
      // Start may take longer on first run (nginx download ~15 MB).
      const s = await api.services.start();
      setStatus(s);
    } catch (e: any) {
      setStartError(e.message);
    } finally {
      setBusy(false);
    }
  }

  async function handleStop() {
    setBusy(true);
    setStartError(null);
    try {
      const s = await api.services.stop();
      setStatus(s);
    } catch {
      // ignore stop errors
    } finally {
      setBusy(false);
    }
  }

  async function handleQuit() {
    if (!confirm("Stop all services and quit phpenv?")) return;
    setBusy(true);
    try {
      await api.daemon.quit();
      window.close();
    } finally {
      setBusy(false);
    }
  }

  const daemonUp = status !== null;
  const allRunning = status?.all_running ?? false;

  return (
    <div className={styles.shell}>
      <aside className={styles.sidebar}>
        {/* Logo */}
        <div className={styles.logo}>
          <span className={styles.logoIcon}>🐃</span>
          <span className={styles.logoText}>Open Herd</span>
        </div>

        {/* Navigation */}
        <nav className={styles.nav}>
          {nav.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === "/"}
              className={({ isActive }) =>
                [styles.navItem, isActive ? styles.active : ""].join(" ")
              }
            >
              <span className={styles.navIcon}>{item.icon}</span>
              {item.label}
            </NavLink>
          ))}
        </nav>

        {/* Service status + controls */}
        <div className={styles.servicePanel}>
          <div className={styles.serviceTitle}>Services</div>

          {!daemonUp ? (
            <div className={styles.daemonDown}>
              {!initialized ? "Connecting…" : "Daemon offline"}
            </div>
          ) : (
            <ul className={styles.serviceList}>
              <li className={styles.serviceItem}>
                <span className={status?.nginx ? styles.dotGreen : styles.dotRed} />
                nginx
              </li>
              {status?.php_versions.map((v) => (
                <li key={v.major} className={styles.serviceItem}>
                  <span className={v.running ? styles.dotGreen : styles.dotRed} />
                  PHP {v.major}
                </li>
              ))}
            </ul>
          )}

          {startError && (
            <div className={styles.startError} title={startError}>
              ✕ {startError}
            </div>
          )}

          <div className={styles.controls}>
            {allRunning ? (
              <button
                className={styles.btnStop}
                onClick={handleStop}
                disabled={busy || !daemonUp}
              >
                {busy ? "…" : "Stop all"}
              </button>
            ) : (
              <button
                className={styles.btnStart}
                onClick={handleStart}
                disabled={busy || !daemonUp}
              >
                {busy ? "Setting up…" : "Start all"}
              </button>
            )}
            <button
              className={styles.btnQuit}
              onClick={handleQuit}
              disabled={busy}
            >
              Quit
            </button>
          </div>
        </div>
      </aside>

      <main className={styles.content}>
        <Outlet />
      </main>
    </div>
  );
}
