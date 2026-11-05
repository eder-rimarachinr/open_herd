import { useCallback, useEffect, useRef, useState } from "react";
import { RefreshCw, Check, Download, AlertCircle, AlertTriangle, Trash2, Hexagon } from "lucide-react";
import { api, NodeInstallProgress, NodeReleaseEntry, NodeStatus } from "../api/client";
import { useToast } from "../context/ToastContext";
import styles from "./PHP.module.css";
import node from "./Node.module.css";

const POLL_MS = 1500;

const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));

const MODE_LABEL: Record<NodeStatus["mode"], string> = {
  "nvm-windows": "Managed through nvm-windows",
  "open-herd":   "Managed by Open Herd",
};

export default function Node() {
  const { toast } = useToast();
  const [status,     setStatus]     = useState<NodeStatus | null>(null);
  const [loading,    setLoading]    = useState(true);
  const [releases,   setReleases]   = useState<NodeReleaseEntry[]>([]);
  const [relError,   setRelError]   = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [busy,       setBusy]       = useState<string | null>(null); // version being activated / removed
  const [installs,   setInstalls]   = useState<Record<string, NodeInstallProgress>>({});
  const pollRefs = useRef<Record<string, ReturnType<typeof setInterval>>>({});

  const fetchStatus = useCallback(async () => {
    try { setStatus(await api.node.status()); }
    catch (e) { toast(errMsg(e), "error"); }
  }, [toast]);

  const fetchReleases = useCallback(async (refresh = false) => {
    try { setReleases(await api.node.releases(refresh)); setRelError(null); }
    catch (e) { setRelError(errMsg(e)); }
  }, []);

  useEffect(() => {
    Promise.all([fetchStatus(), fetchReleases()]).finally(() => setLoading(false));
    const polls = pollRefs.current;
    return () => Object.values(polls).forEach(clearInterval);
  }, [fetchStatus, fetchReleases]);

  async function handleRefresh() {
    setRefreshing(true);
    await Promise.all([fetchStatus(), fetchReleases(true)]);
    setRefreshing(false);
  }

  async function handleActivate(version: string) {
    setBusy(version);
    try {
      setStatus(await api.node.activate(version));
      fetchReleases();
      toast(`Node ${version} is now active. Open a new terminal to use it.`, "success");
    } catch (e) {
      toast(errMsg(e), "error");
    } finally {
      setBusy(null);
    }
  }

  async function handleUninstall(version: string) {
    if (!window.confirm(`Uninstall Node ${version}? Its folder will be deleted.`)) return;
    setBusy(version);
    try {
      await api.node.uninstall(version);
      await Promise.all([fetchStatus(), fetchReleases()]);
    } catch (e) {
      toast(errMsg(e), "error");
    } finally {
      setBusy(null);
    }
  }

  function stopPolling(version: string) {
    clearInterval(pollRefs.current[version]);
    delete pollRefs.current[version];
  }

  /** `activateWhenDone`: an "Update" of the active major switches to the new
   *  patch, so the user really ends up on it. */
  async function handleInstall(version: string, activateWhenDone = false) {
    setInstalls((p) => ({ ...p, [version]: { version, state: "pending", message: "Starting…", percent: 0 } }));
    try {
      await api.node.install(version);
    } catch (e) {
      setInstalls((p) => ({ ...p, [version]: { version, state: "error", message: "", percent: 0, error: errMsg(e) } }));
      return;
    }
    pollRefs.current[version] = setInterval(async () => {
      try {
        const prog = await api.node.installProgress(version);
        setInstalls((p) => ({ ...p, [version]: prog }));
        if (prog.state === "done" || prog.state === "error" || prog.state === "idle") {
          stopPolling(version);
          if (prog.state !== "error") {
            setInstalls((p) => { const n = { ...p }; delete n[version]; return n; });
            if (activateWhenDone) await handleActivate(version);
            else await Promise.all([fetchStatus(), fetchReleases()]);
          }
        }
      } catch { /* daemon busy: try again on the next tick */ }
    }, POLL_MS);
  }

  if (loading) return <div className={styles.loading}>Loading…</div>;

  const active = status?.active ?? null;
  const activeMajor = active ? Number(active.split(".")[0]) : null;
  const ltsOf = (version: string) => releases.find((r) => r.version === version)?.lts ?? null;

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <div>
          <h1 className="page-title">Node</h1>
          <p className="page-sub">
            One Node.js version is active for the whole system. Terminals opened after a switch use it.
          </p>
        </div>
        <button className={styles.btnGhost} onClick={handleRefresh} disabled={refreshing}>
          <RefreshCw size={14} style={{ animation: refreshing ? "spin 0.6s linear infinite" : "none" }} />
          {refreshing ? "Refreshing…" : "Refresh list"}
        </button>
      </div>

      {/* ── Active version ─────────────────────────────────────────── */}
      <div className={["panel", node.hero].join(" ")} data-testid="node-active">
        <Hexagon size={28} className={node.heroIcon} />
        <div className={node.heroMain}>
          <div className={node.heroLabel}>Active version</div>
          <div className={node.heroVersion}>{active ? `v${active}` : "None"}</div>
        </div>
        {status && (
          <div className={node.heroMeta}>
            <span>{MODE_LABEL[status.mode]}</span>
            <code>{status.link_path}</code>
          </div>
        )}
      </div>

      {status?.warnings.map((w) => (
        <div key={w} className={node.warning}><AlertTriangle size={14} /> {w}</div>
      ))}

      {/* ── Installed ──────────────────────────────────────────────── */}
      <div className="section-label">Installed versions</div>
      {status && status.installed.length === 0 && (
        <div className={styles.emptyTable}>No Node versions installed yet. Install one from the list below.</div>
      )}
      <div className={styles.grid}>
        {status?.installed.map((inst) => {
          const isActive = inst.version === active;
          const lts = ltsOf(inst.version);
          return (
            <div key={inst.version} className={[styles.card, isActive ? styles.cardSelected : ""].join(" ")}>
              <div className={styles.cardTop}>
                <span className={styles.major}>{inst.version}</span>
                <span className={styles.chips}>
                  {lts && <span className={`${styles.tag} ${styles.tagActive}`}>LTS {lts}</span>}
                </span>
              </div>
              <div className={styles.patch} title={inst.path}>{inst.path}</div>
              <div className={styles.cardFoot}>
                {isActive ? (
                  <span className={styles.running}><span className="status-dot on" /> Active</span>
                ) : (
                  <button
                    className={styles.btnUpdate}
                    onClick={() => handleActivate(inst.version)}
                    disabled={busy !== null}
                    aria-label={`Activate Node ${inst.version}`}
                  >
                    {busy === inst.version ? "Switching…" : "Activate"}
                  </button>
                )}
                <span className={styles.actions}>
                  <button
                    className={[styles.btnIcon, styles.btnIconStop].join(" ")}
                    onClick={() => handleUninstall(inst.version)}
                    disabled={isActive || busy !== null}
                    title={isActive ? "Activate another version before uninstalling this one" : `Uninstall Node ${inst.version}`}
                    aria-label={`Uninstall Node ${inst.version}`}
                  >
                    <Trash2 size={13} />
                  </button>
                </span>
              </div>
            </div>
          );
        })}
      </div>

      {/* ── Available ──────────────────────────────────────────────── */}
      <div className="section-label">Available from nodejs.org</div>
      {relError && <div className={node.error}><AlertCircle size={14} /> {relError}</div>}
      <div className={styles.grid}>
        {releases.map((rel) => {
          const prog = installs[rel.version];
          const installing = prog && prog.state !== "done" && prog.state !== "error";
          return (
            <div key={rel.version} className={styles.card}>
              <div className={styles.cardTop}>
                <span className={styles.major}>{rel.version}</span>
                {rel.installed && <span className={styles.checkmark} title="Installed"><Check size={14} strokeWidth={2.5} /></span>}
                <span className={styles.chips}>
                  {rel.security && <span className={`${styles.tag} ${styles.tagSecurity}`}>security</span>}
                  {rel.lts
                    ? <span className={`${styles.tag} ${styles.tagActive}`}>LTS {rel.lts}</span>
                    : <span className={`${styles.tag} ${styles.tagSecurity}`}>Current</span>}
                </span>
              </div>
              <div className={styles.patch}>
                Released {rel.date}
                {rel.update_available && <span className={styles.updateNote}> · newer than your {rel.major}.x</span>}
              </div>
              {installing ? (
                <div className={styles.progress}>
                  <div className={styles.progressTrack}>
                    <div className={styles.progressFill} style={{ width: `${prog.percent}%` }} />
                  </div>
                  <span className={styles.progressLabel}>{prog.message}</span>
                </div>
              ) : (
                <div className={styles.cardFoot}>
                  {rel.installed
                    ? <span className={styles.notInstalled}>{rel.active ? "Installed · active" : "Installed"}</span>
                    : <span className={styles.notInstalled}>Not installed</span>}
                  <span className={styles.actions}>
                    {prog?.state === "error" && (
                      <button
                        className={styles.btnError}
                        title="Click to dismiss"
                        onClick={() => setInstalls((p) => { const n = { ...p }; delete n[rel.version]; return n; })}
                      >
                        <AlertCircle size={12} /> {prog.error ?? "Install failed"}
                      </button>
                    )}
                    {prog?.state !== "error" && !rel.installed && (
                      <button
                        className={rel.update_available ? styles.btnUpdate : styles.btnInstall}
                        onClick={() => handleInstall(rel.version, rel.update_available && activeMajor === rel.major)}
                        title={rel.update_available && activeMajor === rel.major
                          ? `Install ${rel.version} and make it the active version`
                          : `Install ${rel.version}`}
                        aria-label={`Install Node ${rel.version}`}
                      >
                        <Download size={13} /> {rel.update_available ? "Update" : "Install"}
                      </button>
                    )}
                  </span>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
