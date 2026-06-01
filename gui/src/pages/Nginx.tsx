import { useCallback, useEffect, useRef, useState } from "react";
import { Play, Square, RotateCcw, Download } from "lucide-react";
import { api, NginxInfo } from "../api/client";
import { useToast } from "../context/ToastContext";
import styles from "./Nginx.module.css";

export default function Nginx() {
  const { toast } = useToast();
  const [info,         setInfo]         = useState<NginxInfo | null>(null);
  const [loading,      setLoading]      = useState(true);
  const [busy,         setBusy]         = useState<string | null>(null);
  const [error,        setError]        = useState<string | null>(null);
  const [dlProgress,   setDlProgress]   = useState<{ state: string; message: string; percent: number } | null>(null);
  const dlPollRef = useRef<ReturnType<typeof setInterval>>();

  const fetchInfo = useCallback(async () => {
    try { setInfo(await api.nginx.info()); } catch {}
  }, []);

  // Poll download progress while downloading
  function startDlPoll() {
    if (dlPollRef.current) return;
    dlPollRef.current = setInterval(async () => {
      try {
        const p = await api.nginx.downloadProgress();
        setDlProgress({ state: p.state, message: p.message, percent: guessPercent(p.state) });
        if (p.state === "done" || p.state === "error") {
          clearInterval(dlPollRef.current);
          dlPollRef.current = undefined;
          await fetchInfo();
          if (p.state === "done") {
            toast("Nginx downloaded and ready");
            setDlProgress(null);
          } else {
            toast(p.error ?? "Nginx download failed", "error");
          }
        }
      } catch {
        clearInterval(dlPollRef.current);
        dlPollRef.current = undefined;
      }
    }, 700);
  }

  useEffect(() => {
    fetchInfo().finally(() => setLoading(false));
    const iv = setInterval(fetchInfo, 5000);
    return () => {
      clearInterval(iv);
      clearInterval(dlPollRef.current);
    };
  }, [fetchInfo]);

  async function act(action: string, fn: () => Promise<unknown>, successMsg?: string) {
    setBusy(action); setError(null);
    try {
      await fn();
      await fetchInfo();
      if (successMsg) toast(successMsg);
    } catch (e: any) {
      setError(e.message);
      toast(e.message, "error");
    } finally { setBusy(null); }
  }

  async function handleDownload() {
    setDlProgress({ state: "pending", message: "Starting download…", percent: 2 });
    await act("download", () => api.nginx.download());
    startDlPoll();
  }

  if (loading) return <div className={styles.loading}>Loading…</div>;

  const running   = info?.running   ?? false;
  const installed = info?.installed ?? false;
  const downloading = dlProgress && dlProgress.state !== "done" && dlProgress.state !== "error";

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <h1 className="page-title">Nginx</h1>
      </div>

      {/* Status card */}
      <section className={styles.card}>
        <div className={styles.statusRow}>
          <div className={styles.statusGroup}>
            <span className={installed ? styles.dotGreen : styles.dotRed} />
            <span className={styles.statusLabel}>{installed ? "Installed" : "Not installed"}</span>
            {info?.version && <span className={styles.version}>{info.version}</span>}
          </div>
          <div className={styles.statusGroup}>
            <span className={running ? styles.dotGreen : styles.dotRed} />
            <span className={styles.statusLabel}>{running ? "Running" : "Stopped"}</span>
          </div>
          {info?.config_valid != null && (
            <div className={styles.statusGroup}>
              <span className={info.config_valid ? styles.dotGreen : styles.dotRed} />
              <span className={styles.statusLabel}>{info.config_valid ? "Config OK" : "Config error"}</span>
            </div>
          )}
        </div>

        {/* Download progress bar */}
        {downloading && dlProgress && (
          <div className={styles.dlProgress}>
            <div className={styles.dlBar}>
              <div className={styles.dlFill} style={{ width: `${dlProgress.percent}%` }} />
            </div>
            <span className={styles.dlLabel}>{dlProgress.message}</span>
          </div>
        )}

        {error && <div className={styles.error}>{error}</div>}

        <div className={styles.actions}>
          {!installed && !downloading && (
            (!info || info.os === "windows" || info.downloadable) ? (
              <div className={styles.downloadSection}>
                <p className={styles.installDesc}>
                  {info ? "Nginx will be downloaded and installed automatically." : "Connecting to daemon…"}
                </p>
                <button
                  className={`${styles.actionBtn} ${styles.btnDownload}`}
                  disabled={busy !== null || !info}
                  onClick={handleDownload}
                >
                  <Download size={13} />
                  {busy === "download" ? "Starting…" : "Download Nginx"}
                </button>
              </div>
            ) : (
              <span className={styles.installHint}>
                Install via your package manager: <code>apt install nginx</code>
              </span>
            )
          )}

          {installed && !running && !downloading && (
            <button
              className={`${styles.actionBtn} ${styles.btnStart}`}
              disabled={busy !== null}
              onClick={() => act("start", () => api.nginx.start(), "Nginx started")}
            >
              <Play size={13} fill="currentColor" />
              {busy === "start" ? "Starting…" : "Start"}
            </button>
          )}

          {installed && running && (
            <>
              <button
                className={`${styles.actionBtn} ${styles.btnReload}`}
                disabled={busy !== null}
                onClick={() => act("reload", () => api.nginx.reload(), "Nginx reloaded")}
              >
                <RotateCcw size={13} />
                {busy === "reload" ? "Reloading…" : "Reload"}
              </button>
              <button
                className={`${styles.actionBtn} ${styles.btnStop}`}
                disabled={busy !== null}
                onClick={() => act("stop", () => api.nginx.stop(), "Nginx stopped")}
              >
                <Square size={13} />
                {busy === "stop" ? "Stopping…" : "Stop"}
              </button>
            </>
          )}
        </div>
      </section>

      {info?.config_error && (
        <section className={styles.card}>
          <div className={styles.sectionTitle}>Config error</div>
          <pre className={styles.log}>{info.config_error}</pre>
        </section>
      )}

      {info?.error_log && (
        <section className={styles.card}>
          <div className={styles.sectionTitle}>Error log</div>
          <pre className={styles.log}>{info.error_log}</pre>
        </section>
      )}

      {!info?.error_log && installed && (
        <section className={styles.card}>
          <div className={styles.sectionTitle}>Error log</div>
          <p className={styles.empty}>No errors logged.</p>
        </section>
      )}
    </div>
  );
}

function guessPercent(state: string): number {
  const map: Record<string, number> = {
    pending: 2, downloading: 40, extracting: 75, configuring: 90, done: 100,
  };
  return map[state] ?? 50;
}
