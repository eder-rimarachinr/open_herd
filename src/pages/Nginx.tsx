import { useCallback, useEffect, useRef, useState } from "react";
import { Play, Square, RotateCcw, Download, Server, Check, AlertTriangle } from "lucide-react";
import { api, NginxInfo } from "../api/client";
import { useToast } from "../context/ToastContext";
import LogView from "../components/LogView";
import styles from "./Nginx.module.css";

export default function Nginx() {
  const { toast } = useToast();
  const [info,         setInfo]         = useState<NginxInfo | null>(null);
  const [loading,      setLoading]      = useState(true);
  const [busy,         setBusy]         = useState<string | null>(null);
  const [error,        setError]        = useState<string | null>(null);
  const [dlProgress,   setDlProgress]   = useState<{ state: string; message: string; percent: number } | null>(null);
  const [ports,        setPorts]        = useState<{ http: number; https: number } | null>(null);
  const [siteCount,    setSiteCount]    = useState<{ total: number; https: number } | null>(null);
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
    // Both are cached by the client and only change from other pages.
    api.config.get().then((c) => setPorts({ http: c.http_port, https: c.https_port })).catch(() => {});
    api.sites.list()
      .then((s) => setSiteCount({ total: s.length, https: s.filter((x) => x.ssl_enabled).length }))
      .catch(() => {});
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
  const state = !installed ? "missing" : running ? "on" : "off";

  return (
    <div className={styles.page}>
      <div>
        <h1 className="page-title">Nginx</h1>
        <p className="page-sub">
          Serves every <code>.test</code> site. Its config is regenerated on each start.
        </p>
      </div>

      {/* Status */}
      <section className={[styles.hero, styles[state]].join(" ")}>
        <span className={styles.heroIcon}><Server size={24} /></span>
        <div className={styles.heroText}>
          <h2 className={styles.heroTitle}>
            {!installed ? "Not installed" : running ? "Running" : "Stopped"}
            {installed && <span className={["badge", running ? "badge-green" : "badge-red"].join(" ")}>{running ? "listening" : "not listening"}</span>}
          </h2>
          <p className={styles.heroPath}>
            {installed
              ? [info?.version && `nginx/${info.version}`, info?.binary_path].filter(Boolean).join(" · ")
              : info ? "Nginx will be downloaded and installed automatically." : "Connecting to daemon…"}
          </p>
        </div>

        <div className={styles.actions}>
          {!installed && !downloading && (
            (!info || info.os === "windows" || info.downloadable) ? (
              <button className="btn-primary" disabled={busy !== null || !info} onClick={handleDownload}>
                <Download size={14} />
                {busy === "download" ? "Starting…" : "Download Nginx"}
              </button>
            ) : (
              <span className={styles.installHint}>
                Install via your package manager: <code>apt install nginx</code>
              </span>
            )
          )}

          {installed && !running && !downloading && (
            <button
              className="btn-primary"
              disabled={busy !== null}
              onClick={() => act("start", () => api.nginx.start(), "Nginx started")}
            >
              <Play size={14} fill="currentColor" />
              {busy === "start" ? "Starting…" : "Start"}
            </button>
          )}

          {installed && running && (
            <>
              <button
                className={styles.btn}
                disabled={busy !== null}
                onClick={() => act("reload", () => api.nginx.reload(), "Nginx reloaded")}
              >
                <RotateCcw size={14} />
                {busy === "reload" ? "Reloading…" : "Reload"}
              </button>
              <button
                className={[styles.btn, styles.btnStop].join(" ")}
                disabled={busy !== null}
                onClick={() => act("stop", () => api.nginx.stop(), "Nginx stopped")}
              >
                <Square size={13} />
                {busy === "stop" ? "Stopping…" : "Stop"}
              </button>
            </>
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
      </section>

      {error && (
        <div className={styles.alert} role="alert">
          <AlertTriangle size={16} className={styles.alertIcon} />
          <span>{error}</span>
        </div>
      )}

      {installed && (
        <div className={styles.stats}>
          <Stat label="HTTP" value={ports ? `:${ports.http}` : "—"} note={running ? "listening" : "not listening"} />
          <Stat label="HTTPS" value={ports ? `:${ports.https}` : "—"} note={running ? "listening" : "not listening"} />
          <Stat label="Sites" value={siteCount ? String(siteCount.total) : "—"} note={siteCount ? `${siteCount.https} with HTTPS` : ""} />
          {info?.config_valid != null && (
            <Stat
              label="Config"
              value={info.config_valid ? "Config OK" : "Config error"}
              tone={info.config_valid ? "ok" : "bad"}
              icon={info.config_valid ? <Check size={17} /> : <AlertTriangle size={16} />}
              note="nginx -t"
            />
          )}
        </div>
      )}

      {info?.config_error && (
        <section className="panel">
          <div className="panel-head">
            <AlertTriangle size={15} className={styles.badIcon} /> Config error
          </div>
          <pre className={styles.log}>{info.config_error}</pre>
        </section>
      )}

      {installed && (
        <section className="panel">
          <div className="panel-head">
            Error log <span className="sub">· logs/error.log</span>
          </div>
          {info?.error_log
            ? <LogView text={info.error_log} className={styles.logView} />
            : <p className={styles.empty}>No errors logged.</p>}
        </section>
      )}
    </div>
  );
}

function Stat({ label, value, note, tone, icon }: {
  label: string; value: string; note?: string; tone?: "ok" | "bad"; icon?: React.ReactNode;
}) {
  return (
    <div className={styles.stat}>
      <span className={styles.statLabel}>{label}</span>
      <b className={[styles.statValue, tone ? styles[tone] : ""].join(" ")}>{icon}{value}</b>
      {note && <small className={styles.statNote}>{note}</small>}
    </div>
  );
}

function guessPercent(state: string): number {
  const map: Record<string, number> = {
    pending: 2, downloading: 40, extracting: 75, configuring: 90, done: 100,
  };
  return map[state] ?? 50;
}
