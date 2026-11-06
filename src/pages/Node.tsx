import { useCallback, useEffect, useRef, useState } from "react";
import { RefreshCw, Download, AlertCircle, AlertTriangle, Trash2, Hexagon, ChevronRight } from "lucide-react";
import { api, NodeInstallProgress, NodeReleaseEntry, NodeStatus } from "../api/client";
import { useToast } from "../context/ToastContext";
import styles from "./PHP.module.css";
import node from "./Node.module.css";
import list from "./VersionList.module.css";

const POLL_MS = 1500;

const EOL_TITLE = "No longer maintained: no bug or security fixes";

const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));

const MODE_LABEL: Record<NodeStatus["mode"], string> = {
  "nvm-windows": "Managed through nvm-windows",
  "nvm":         "Managed through nvm (default alias)",
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
  const [showEol,    setShowEol]    = useState(false);
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
    const done = () => toast(`Node ${version} is now active. Open a new terminal to use it.`, "success");
    try {
      setStatus(await api.node.activate(version));
      fetchReleases();
      done();
    } catch (e) {
      // The switch may have happened even though the answer never arrived (a
      // dropped connection, the app restarting): show what is really active.
      const now = await api.node.status().catch(() => null);
      if (now) setStatus(now);
      if (now?.active === version) done();
      else toast(errMsg(e), "error");
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
  const installed = [...(status?.installed ?? [])].sort((a, b) => compareVersions(b.version, a.version));
  const releaseOfMajor = (major: number) => releases.find((r) => r.major === major);
  const majorOf = (version: string) => Number(version.split(".")[0]);
  // The newest installed patch of each line carries the Update button for that line.
  const newestInstalled = new Set(
    installed.filter((inst, i) => installed.findIndex((o) => majorOf(o.version) === majorOf(inst.version)) === i)
      .map((inst) => inst.version),
  );
  // Lines with nothing installed; an installed line offers its update on its own row.
  const available = releases.filter((r) => !installed.some((i) => majorOf(i.version) === r.major));
  const supported = available.filter((r) => !r.end_of_life);
  const endOfLife = available.filter((r) => r.end_of_life);

  const dismissError = (version: string) =>
    setInstalls((p) => { const n = { ...p }; delete n[version]; return n; });

  /** Install / Update button, or the running download's progress, for `rel`. */
  function installControl(rel: NodeReleaseEntry, update: boolean) {
    const prog = installs[rel.version];
    if (prog && prog.state !== "done" && prog.state !== "error") {
      return (
        <div className={list.progress}>
          <div className={list.progressTrack}><div className={list.progressFill} style={{ width: `${prog.percent}%` }} /></div>
          <span className={list.progressLabel}>{prog.message}</span>
        </div>
      );
    }
    if (prog?.state === "error") {
      return (
        <button className={styles.btnError} title="Click to dismiss" onClick={() => dismissError(rel.version)}>
          <AlertCircle size={12} /> {prog.error ?? "Install failed"}
        </button>
      );
    }
    const activates = update && activeMajor === rel.major;
    return (
      <button
        className={update ? styles.btnUpdate : styles.btnInstall}
        onClick={() => handleInstall(rel.version, activates)}
        title={activates ? `Install ${rel.version} and make it the active version` : `Install ${rel.version}`}
        aria-label={`Install Node ${rel.version}`}
      >
        <Download size={13} /> {update ? `Update to ${rel.version}` : "Install"}
      </button>
    );
  }

  function lineBadges(rel: NodeReleaseEntry | undefined) {
    if (!rel) return null;
    return (
      <>
        {rel.lts
          ? <span className={`${styles.tag} ${styles.tagActive}`}>LTS {rel.lts}</span>
          : <span className={`${styles.tag} ${styles.tagSecurity}`}>Current</span>}
        {rel.security && <span className={`${styles.tag} ${styles.tagSecurity}`}>security</span>}
        {rel.end_of_life && <span className={`${styles.tag} ${styles.tagEol}`} title={EOL_TITLE}>End of life</span>}
      </>
    );
  }

  function releaseRow(rel: NodeReleaseEntry) {
    return (
      <div key={rel.version} className={[list.row, rel.end_of_life ? list.rowEol : ""].join(" ")} data-testid={`node-release-${rel.major}`}>
        <span className={list.ver}>{rel.version}</span>
        <div className={list.info}>
          <div className={list.badges}>{lineBadges(rel)}</div>
          <div className={list.meta}>Released {rel.date}</div>
        </div>
        <div className={list.actions}>{installControl(rel, false)}</div>
      </div>
    );
  }

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
      <div className="section-label">Installed</div>
      <div className={list.list}>
        {installed.length === 0 && (
          <div className={list.empty}>No Node versions installed yet. Install one from the list below.</div>
        )}
        {installed.map((inst) => {
          const isActive = inst.version === active;
          const rel = releaseOfMajor(majorOf(inst.version));
          const update = rel && newestInstalled.has(inst.version) && compareVersions(rel.version, inst.version) > 0 ? rel : null;
          return (
            <div
              key={inst.version}
              className={[list.row, isActive ? list.rowActive : "", rel?.end_of_life ? list.rowEol : ""].join(" ")}
              data-testid={`node-installed-${inst.version}`}
            >
              <span className={list.ver}>{inst.version}</span>
              <div className={list.info}>
                <div className={list.badges}>{lineBadges(rel)}</div>
                <div className={list.meta} title={inst.path}>{inst.path}</div>
              </div>
              <div className={list.actions}>
                {update && installControl(update, true)}
                {isActive ? (
                  <span className={list.activeLabel}><span className="status-dot on" /> Active</span>
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
                <button
                  className={[styles.btnIcon, styles.btnIconStop].join(" ")}
                  onClick={() => handleUninstall(inst.version)}
                  disabled={isActive || busy !== null}
                  title={isActive ? "Activate another version before uninstalling this one" : `Uninstall Node ${inst.version}`}
                  aria-label={`Uninstall Node ${inst.version}`}
                >
                  <Trash2 size={13} />
                </button>
              </div>
            </div>
          );
        })}
      </div>

      {/* ── Available ──────────────────────────────────────────────── */}
      <div className="section-label">Available from nodejs.org</div>
      {relError && <div className={node.error}><AlertCircle size={14} /> {relError}</div>}
      {!relError && (
        <div className={list.list}>
          {supported.map(releaseRow)}
          {supported.length === 0 && endOfLife.length === 0 && (
            <div className={list.empty}>Every maintained Node line is already installed.</div>
          )}
          {endOfLife.length > 0 && (
            <button
              className={[list.eolToggle, showEol ? list.eolToggleOpen : ""].join(" ")}
              onClick={() => setShowEol((v) => !v)}
              aria-expanded={showEol}
            >
              <ChevronRight size={14} />
              {showEol ? "Hide" : "Show"} {endOfLife.length} end-of-life {endOfLife.length === 1 ? "version" : "versions"}
              <span className={list.meta}>({endOfLife.map((r) => r.major).join(", ")})</span>
            </button>
          )}
          {showEol && endOfLife.length > 0 && (
            <>
              <div className={list.eolNote}>{EOL_TITLE}. Use them only for projects that still need them.</div>
              {endOfLife.map(releaseRow)}
            </>
          )}
        </div>
      )}
    </div>
  );
}

/** Numeric comparison of `a.b.c` versions: negative when `a` is older. */
function compareVersions(a: string, b: string): number {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}
