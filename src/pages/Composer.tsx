import { useCallback, useEffect, useRef, useState } from "react";
import { NavLink } from "react-router-dom";
import { RefreshCw, Download, AlertCircle, AlertTriangle, Package, Wrench, Puzzle } from "lucide-react";
import { api, ComposerInstallProgress, ComposerStatus } from "../api/client";
import { useToast } from "../context/ToastContext";
import styles from "./PHP.module.css";
import node from "./Node.module.css";
import list from "./VersionList.module.css";

const POLL_MS = 1000;

const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e));

export default function Composer() {
  const { toast } = useToast();
  const [status,     setStatus]     = useState<ComposerStatus | null>(null);
  const [loadError,  setLoadError]  = useState<string | null>(null);
  const [loading,    setLoading]    = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [progress,   setProgress]   = useState<ComposerInstallProgress | null>(null);
  const [enabling,   setEnabling]   = useState(false);
  const poll = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchStatus = useCallback(async () => {
    try { setStatus(await api.composer.status()); setLoadError(null); }
    catch (e) { setLoadError(errMsg(e)); }
  }, []);

  const stopPolling = () => {
    if (poll.current) clearInterval(poll.current);
    poll.current = null;
  };

  const follow = useCallback(() => {
    stopPolling();
    poll.current = setInterval(async () => {
      try {
        const prog = await api.composer.installProgress();
        setProgress(prog);
        if (prog.state === "done" || prog.state === "error" || prog.state === "idle") {
          stopPolling();
          if (prog.state !== "error") {
            setProgress(null);
            await fetchStatus();
            toast("Composer is ready. Open a new terminal and run `composer`.", "success");
          }
        }
      } catch { /* daemon busy: try again on the next tick */ }
    }, POLL_MS);
  }, [fetchStatus, toast]);

  useEffect(() => {
    // Pick up a download started before the page was opened.
    api.composer.installProgress().then((p) => {
      if (p.state !== "idle" && p.state !== "done" && p.state !== "error") { setProgress(p); follow(); }
    }).catch(() => {});
    fetchStatus().finally(() => setLoading(false));
    return stopPolling;
  }, [fetchStatus, follow]);

  async function handleRefresh() {
    setRefreshing(true);
    await fetchStatus();
    setRefreshing(false);
  }

  async function handleInstall() {
    setProgress({ state: "pending", message: "Starting…", percent: 0 });
    try {
      const { state } = await api.composer.install();
      if (state === "done") {
        // Already the right release: only the command was rewritten.
        setProgress(null);
        await fetchStatus();
        toast("The composer command was rewritten for the default PHP.", "success");
        return;
      }
      follow();
    } catch (e) {
      setProgress({ state: "error", message: "", percent: 0, error: errMsg(e) });
    }
  }

  async function handleEnableExtensions() {
    setEnabling(true);
    try {
      const next = await api.composer.enableExtensions();
      setStatus(next);
      if (next.missing_extensions.length === 0) toast("Extensions enabled in php.ini.", "success");
      else toast(`Still not loaded: ${next.missing_extensions.join(", ")}. Check the PHP page.`, "error");
    } catch (e) {
      toast(errMsg(e), "error");
    } finally {
      setEnabling(false);
    }
  }

  if (loading) return <div className={styles.loading}>Loading…</div>;

  const busy = progress !== null && progress.state !== "error";
  const phpMissing = status !== null && status.php_version === null;

  function installControl(s: ComposerStatus) {
    if (progress && progress.state !== "error") {
      return (
        <div className={list.progress} data-testid="composer-progress">
          <div className={list.progressTrack}><div className={list.progressFill} style={{ width: `${progress.percent}%` }} /></div>
          <span className={list.progressLabel}>{progress.message}</span>
        </div>
      );
    }
    if (progress?.state === "error") {
      return (
        <button className={styles.btnError} title="Click to dismiss" onClick={() => setProgress(null)}>
          <AlertCircle size={12} /> {progress.error ?? progress.message ?? "Install failed"}
        </button>
      );
    }
    if (!s.installed) {
      return (
        <button className={styles.btnInstall} onClick={handleInstall} disabled={phpMissing}
          title={phpMissing ? `Install PHP ${s.default_php} first` : undefined}>
          <Download size={13} /> Install{s.latest ? ` ${s.latest}` : ""}
        </button>
      );
    }
    if (s.update_available && s.latest) {
      return (
        <button className={styles.btnUpdate} onClick={handleInstall} disabled={phpMissing}>
          <Download size={13} /> Update to {s.latest}
        </button>
      );
    }
    return (
      <button className={styles.btnGhost} onClick={handleInstall} disabled={phpMissing}
        title="Rewrite the composer command for the default PHP and add it to the PATH again">
        <Wrench size={13} /> Repair
      </button>
    );
  }

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <div>
          <h1 className="page-title">Composer</h1>
          <p className="page-sub">
            The <code>composer</code> command, available in every new terminal. It runs with the default PHP version from <NavLink to="/settings">Settings</NavLink>.
          </p>
        </div>
        <button className={styles.btnGhost} onClick={handleRefresh} disabled={refreshing || busy}>
          <RefreshCw size={14} style={{ animation: refreshing ? "spin 0.6s linear infinite" : "none" }} />
          {refreshing ? "Refreshing…" : "Refresh"}
        </button>
      </div>

      {loadError && <div className={node.error}><AlertCircle size={14} /> {loadError}</div>}

      {status && (
        <>
          <div className={["panel", node.hero].join(" ")} data-testid="composer-status">
            <Package size={28} className={node.heroIcon} />
            <div className={node.heroMain}>
              <div className={node.heroLabel}>Installed version</div>
              <div className={node.heroVersion}>
                {status.installed ? (status.version ?? "Unknown") : "Not installed"}
              </div>
            </div>
            <div className={node.heroMeta}>
              <span>
                PHP {status.default_php}
                {status.php_version ? ` (${status.php_version})` : " — not installed"}
              </span>
              {status.installed && <code title={status.command_path}>{status.command_path}</code>}
            </div>
            <div className={list.actions}>{installControl(status)}</div>
          </div>

          {phpMissing && !status.installed && (
            <div className={node.warning}>
              <AlertTriangle size={14} /> Composer needs the default PHP ({status.default_php}). Install it on the <NavLink to="/php">PHP page</NavLink> or pick another default in <NavLink to="/settings">Settings</NavLink>.
            </div>
          )}

          {status.missing_extensions.length > 0 && (
            <div className={node.warning} data-testid="composer-extensions">
              <Puzzle size={14} />
              <span style={{ flex: 1 }}>
                PHP {status.php_version} does not load {status.missing_extensions.join(", ")}: Composer needs {status.missing_extensions.length === 1 ? "it" : "them"} to download packages.
                {!status.can_enable_extensions && " Enable them in that PHP's php.ini."}
              </span>
              {status.can_enable_extensions && (
                <button className={styles.btnUpdate} onClick={handleEnableExtensions} disabled={enabling}>
                  {enabling ? "Enabling…" : "Enable"}
                </button>
              )}
            </div>
          )}

          {status.warnings.map((w) => (
            <div key={w} className={node.warning}><AlertTriangle size={14} /> {w}</div>
          ))}

          {status.installed && (
            <>
              <div className="section-label">Details</div>
              <div className={list.list}>
                <div className={list.row}>
                  <span className={list.meta}>composer.phar</span>
                  <div className={list.info}><code className={list.meta}>{status.phar_path}</code></div>
                </div>
                <div className={list.row}>
                  <span className={list.meta}>Runs with</span>
                  <div className={list.info}><code className={list.meta}>{status.command_php ?? "—"}</code></div>
                </div>
                <div className={list.row}>
                  <span className={list.meta}>On the PATH</span>
                  <div className={list.info}><code className={list.meta}>{status.bin_dir}</code></div>
                </div>
              </div>
            </>
          )}
        </>
      )}
    </div>
  );
}
