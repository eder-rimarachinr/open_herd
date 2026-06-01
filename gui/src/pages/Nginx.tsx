import { useCallback, useEffect, useState } from "react";
import { Play, Square, RotateCcw, Download } from "lucide-react";
import { api, NginxInfo } from "../api/client";
import styles from "./Nginx.module.css";

export default function Nginx() {
  const [info,    setInfo]    = useState<NginxInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy,    setBusy]    = useState<string | null>(null);
  const [error,   setError]   = useState<string | null>(null);

  const fetchInfo = useCallback(async () => {
    try { setInfo(await api.nginx.info()); } catch {}
  }, []);

  useEffect(() => {
    fetchInfo().finally(() => setLoading(false));
    const iv = setInterval(fetchInfo, 3000);
    return () => clearInterval(iv);
  }, [fetchInfo]);

  async function act(action: string, fn: () => Promise<unknown>) {
    setBusy(action);
    setError(null);
    try { await fn(); await fetchInfo(); }
    catch (e: any) { setError(e.message); }
    finally { setBusy(null); }
  }

  if (loading) return <div className={styles.loading}>Loading…</div>;

  const running   = info?.running   ?? false;
  const installed = info?.installed ?? false;

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

        {error && <div className={styles.error}>{error}</div>}

        <div className={styles.actions}>
          {!installed && (
            <>
              {(!info || info.os === "windows" || info.downloadable) ? (
                <div className={styles.downloadSection}>
                  <p className={styles.installDesc}>
                    {info
                      ? "Nginx will be downloaded and installed automatically."
                      : "Connecting to daemon…"}
                  </p>
                  <button
                    className={`${styles.actionBtn} ${styles.btnDownload}`}
                    disabled={busy !== null || !info}
                    onClick={() => act("download", () => api.nginx.download())}
                  >
                    <Download size={13} />
                    {busy === "download" ? "Downloading…" : "Download Nginx"}
                  </button>
                </div>
              ) : (
                <span className={styles.installHint}>
                  Nginx not found. Install via your package manager: <code>apt install nginx</code>
                </span>
              )}
            </>
          )}

          {installed && !running && (
            <button
              className={`${styles.actionBtn} ${styles.btnStart}`}
              disabled={busy !== null}
              onClick={() => act("start", () => api.nginx.start())}
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
                onClick={() => act("reload", () => api.nginx.reload())}
              >
                <RotateCcw size={13} />
                {busy === "reload" ? "Reloading…" : "Reload"}
              </button>
              <button
                className={`${styles.actionBtn} ${styles.btnStop}`}
                disabled={busy !== null}
                onClick={() => act("stop", () => api.nginx.stop())}
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
          <div className={styles.sectionTitle}>Error log (last 40 lines)</div>
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
