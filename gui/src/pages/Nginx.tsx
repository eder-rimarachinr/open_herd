import { useCallback, useEffect, useState } from "react";
import { api, NginxInfo } from "../api/client";
import styles from "./Nginx.module.css";

export default function Nginx() {
  const [info, setInfo] = useState<NginxInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null); // which action is in flight
  const [error, setError] = useState<string | null>(null);

  const fetchInfo = useCallback(async () => {
    try {
      const data = await api.nginx.info();
      setInfo(data);
    } catch {
      // keep last state if daemon unreachable
    }
  }, []);

  useEffect(() => {
    fetchInfo().finally(() => setLoading(false));
    const iv = setInterval(fetchInfo, 3000);
    return () => clearInterval(iv);
  }, [fetchInfo]);

  async function act(action: string, fn: () => Promise<unknown>) {
    setBusy(action);
    setError(null);
    try {
      await fn();
      await fetchInfo();
    } catch (e: any) {
      setError(e.message);
    } finally {
      setBusy(null);
    }
  }

  if (loading) return <div className={styles.loading}>Loading…</div>;

  const running = info?.running ?? false;
  const installed = info?.installed ?? false;

  return (
    <div className={styles.page}>
      <h1 className={styles.title}>Nginx</h1>

      {/* ── Status card ── */}
      <section className={styles.card}>
        <div className={styles.statusRow}>
          <div className={styles.statusGroup}>
            <span className={installed ? styles.dotGreen : styles.dotRed} />
            <span className={styles.statusLabel}>
              {installed ? "Installed" : "Not installed"}
            </span>
            {info?.version && (
              <span className={styles.version}>{info.version}</span>
            )}
          </div>

          <div className={styles.statusGroup}>
            <span className={running ? styles.dotGreen : styles.dotRed} />
            <span className={styles.statusLabel}>
              {running ? "Running" : "Stopped"}
            </span>
          </div>

          {info?.config_valid != null && (
            <div className={styles.statusGroup}>
              <span className={info.config_valid ? styles.dotGreen : styles.dotRed} />
              <span className={styles.statusLabel}>
                {info.config_valid ? "Config OK" : "Config error"}
              </span>
            </div>
          )}
        </div>

        {error && <div className={styles.error}>{error}</div>}

        {/* ── Action buttons ── */}
        <div className={styles.actions}>
          {!installed && (
            <div className={styles.installActions}>
              {info?.os === "windows" || info?.downloadable ? (
                <div className={styles.downloadSection}>
                  <p className={styles.installDesc}>
                    Nginx will be downloaded and installed automatically into the application folder.
                  </p>
                  <button
                    className={styles.btnDownload}
                    disabled={busy !== null}
                    onClick={() => act("download", () => api.nginx.download())}
                  >
                    {busy === "download" ? "Downloading…" : "⬇ Download and Setup Nginx"}
                  </button>
                </div>
              ) : (
                <span className={styles.installHint}>
                  Nginx not found. Please install it via your package manager (e.g. <code>apt install nginx</code>).
                </span>
              )}
            </div>
          )}

          {installed && !running && (
            <button
              className={styles.btnStart}
              disabled={busy !== null}
              onClick={() => act("start", () => api.nginx.start())}
            >
              {busy === "start" ? "Starting…" : "▶ Start"}
            </button>
          )}

          {installed && running && (
            <>
              <button
                className={styles.btnReload}
                disabled={busy !== null}
                onClick={() => act("reload", () => api.nginx.reload())}
              >
                {busy === "reload" ? "Reloading…" : "↺ Reload"}
              </button>
              <button
                className={styles.btnStop}
                disabled={busy !== null}
                onClick={() => act("stop", () => api.nginx.stop())}
              >
                {busy === "stop" ? "Stopping…" : "■ Stop"}
              </button>
            </>
          )}
        </div>
      </section>

      {/* ── Config error detail ── */}
      {info?.config_error && (
        <section className={styles.card}>
          <h2 className={styles.sectionTitle}>Config error</h2>
          <pre className={styles.log}>{info.config_error}</pre>
        </section>
      )}

      {/* ── Error log ── */}
      {info?.error_log && (
        <section className={styles.card}>
          <h2 className={styles.sectionTitle}>Error log (last 40 lines)</h2>
          <pre className={styles.log}>{info.error_log}</pre>
        </section>
      )}

      {!info?.error_log && installed && (
        <section className={styles.card}>
          <h2 className={styles.sectionTitle}>Error log</h2>
          <p className={styles.empty}>No errors logged.</p>
        </section>
      )}
    </div>
  );
}
