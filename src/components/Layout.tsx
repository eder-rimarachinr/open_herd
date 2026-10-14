import { useCallback, useEffect, useRef, useState } from "react";
import { NavLink, Outlet } from "react-router-dom";
import {
  Globe, Cpu, Server, ShieldCheck, Terminal,
  Settings, Sun, Moon, Play, Square, Power,
  Circle, AlertTriangle, X,
} from "lucide-react";
import { api, ServiceStatus } from "../api/client";
import { phpPort } from "../api/ports";
import { useTheme } from "../hooks/useTheme";
import Logo from "./Logo";
import styles from "./Layout.module.css";

const OS_LABEL: Record<string, string> = { windows: "Windows", linux: "Linux", macos: "macOS" };

const NAV = [
  { to: "/",         label: "Sites",    Icon: Globe },
  { to: "/php",      label: "PHP",      Icon: Cpu },
  { to: "/nginx",    label: "Nginx",    Icon: Server },
  { to: "/ssl",      label: "SSL",      Icon: ShieldCheck },
  { to: "/logs",     label: "Logs",     Icon: Terminal },
  { to: "/settings", label: "Settings", Icon: Settings },
];

const POLL_INTERVAL = 2000;       // cada 2s
const OFFLINE_THRESHOLD = 15;     // 15 fallos × 2s = 30s antes de marcar offline

export default function Layout() {
  const { theme, toggle } = useTheme();
  const [status,      setStatus]      = useState<ServiceStatus | null>(null);
  const [busy,        setBusy]        = useState(false);
  const [initialized, setInitialized] = useState(false);
  const [startError,  setStartError]  = useState<string | null>(null);
  const [, setFailCount] = useState(0);
  const [loadWarnings, setLoadWarnings] = useState<string[]>([]);
  const [warningsDismissed, setWarningsDismissed] = useState(false);
  const [about, setAbout] = useState<{ version: string; os: string } | null>(null);
  const [ports, setPorts] = useState<{ http: number; https: number } | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval>>();

  const fetchStatus = useCallback(async () => {
    try {
      const s = await api.services.status();
      setStatus(s);
      setFailCount(0);
      setInitialized(true);
      // Served from the client cache; Settings invalidates it when ports change.
      api.config.get()
        .then((c) => setPorts((p) => p?.http === c.http_port && p.https === c.https_port ? p : { http: c.http_port, https: c.https_port }))
        .catch(() => {});
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

  const daemonUp   = status !== null;

  // Load warnings, version and ports only change when the daemon restarts (or
  // in Settings), so fetch them once per connection instead of on every poll.
  useEffect(() => {
    if (!daemonUp) return;
    api.status()
      .then((s) => {
        setLoadWarnings(s.warnings ?? []);
        setAbout({ version: s.version, os: s.os });
      })
      .catch(() => {});
  }, [daemonUp]);

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

  const allRunning = status?.all_running ?? false;

  return (
    <div className={styles.shell}>
      {/* ── Sidebar ──────────────────────────────────────────────────────── */}
      <aside className={styles.sidebar}>

        <div className={styles.logo}>
          <Logo />
          <div className={styles.logoText}>
            <span className={styles.logoName}>Open Herd</span>
            <span className={styles.logoSub}>
              {about ? `v${about.version} · ${OS_LABEL[about.os] ?? about.os}` : "Local PHP"}
            </span>
          </div>
        </div>

        <nav className={styles.nav} aria-label="Main">
          {NAV.map(({ to, label, Icon }) => (
            <NavLink
              key={to} to={to} end={to === "/"}
              className={({ isActive }) =>
                [styles.navItem, isActive ? styles.navActive : ""].join(" ")
              }
            >
              <Icon size={16} strokeWidth={1.8} className={styles.navIcon} />
              {label}
            </NavLink>
          ))}
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
              <ServiceRow
                label="Nginx"
                meta={ports ? `:${ports.http} :${ports.https}` : undefined}
                running={status?.nginx ?? false}
              />
              {status?.php_versions.map((v) => {
                const port = about?.os === "windows" ? phpPort(v.major) : null;
                return (
                  <ServiceRow
                    key={v.major}
                    label={`PHP ${v.major}`}
                    title={v.version}
                    meta={port ? `:${port}` : undefined}
                    running={v.running}
                  />
                );
              })}
            </ul>
          )}

          {startError && (
            <div className={styles.startError} title={startError}>{startError}</div>
          )}

          <div className={styles.serviceActions}>
            {allRunning ? (
              <button className={styles.btnStop} onClick={handleStop} disabled={busy || !daemonUp}>
                <Square size={12} strokeWidth={2.2} />
                {busy ? "Stopping…" : "Stop all"}
              </button>
            ) : (
              <button className={styles.btnStart} onClick={handleStart} disabled={busy || !daemonUp}>
                <Play size={12} strokeWidth={2.2} fill="currentColor" />
                {busy ? "Starting…" : "Start all"}
              </button>
            )}
          </div>
        </div>

        {/* ── Bottom bar ───────────────────────────────────────────────── */}
        <div className={styles.bottomBar}>
          <button
            className={styles.btnGhost}
            onClick={toggle}
            title={`Switch to ${theme === "dark" ? "light" : "dark"} mode`}
          >
            {theme === "dark" ? <Sun size={14} strokeWidth={1.8} /> : <Moon size={14} strokeWidth={1.8} />}
            {theme === "dark" ? "Light" : "Dark"}
          </button>
          <button className={[styles.btnGhost, styles.btnQuit].join(" ")} onClick={handleQuit} disabled={busy}>
            <Power size={14} strokeWidth={1.8} /> Quit
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
        {/* Data that could not be loaded at startup (kept on disk, see Logs) */}
        {daemonUp && loadWarnings.length > 0 && !warningsDismissed && (
          <div className={styles.warningBanner} role="alert">
            <AlertTriangle size={14} className={styles.warningIcon} />
            <div className={styles.warningBody}>
              <strong>Some saved data could not be loaded.</strong> Nothing was deleted — the
              originals were kept on disk. <NavLink to="/logs">Open logs</NavLink>
              <ul className={styles.warningList}>
                {loadWarnings.map((w) => <li key={w}>{w}</li>)}
              </ul>
            </div>
            <button
              className={styles.warningDismiss}
              onClick={() => setWarningsDismissed(true)}
              aria-label="Dismiss"
              title="Dismiss"
            >
              <X size={14} />
            </button>
          </div>
        )}
        <Outlet />
      </main>
    </div>
  );
}

function ServiceRow({
  label, title, meta, running,
}: { label: string; title?: string; meta?: string; running: boolean }) {
  return (
    <li className={styles.serviceRow} title={title}>
      <span className={["status-dot", running ? "on" : "off"].join(" ")} />
      <span className={styles.serviceLabel}>{label}</span>
      {meta && <span className={styles.serviceMeta}>{meta}</span>}
      <span className={[styles.serviceStatus, running ? styles.statusRunning : styles.statusStopped].join(" ")}>
        {running ? "on" : "off"}
      </span>
    </li>
  );
}
