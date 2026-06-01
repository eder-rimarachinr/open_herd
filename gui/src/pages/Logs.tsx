import { useCallback, useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { api } from "../api/client";
import styles from "./Logs.module.css";

type Tab = "daemon" | "nginx";

export default function Logs() {
  const [tab,       setTab]       = useState<Tab>("daemon");
  const [daemonLog, setDaemonLog] = useState("");
  const [nginxLog,  setNginxLog]  = useState("");
  const [loading,   setLoading]   = useState(true);
  const bottomRef     = useRef<HTMLDivElement>(null);
  const autoScrollRef = useRef(true);

  const fetchDaemon = useCallback(async () => {
    try { const d = await api.daemon.logs(); setDaemonLog(d.logs); } catch {}
  }, []);

  const fetchNginx = useCallback(async () => {
    try { const i = await api.nginx.info(); setNginxLog(i.error_log || ""); } catch {}
  }, []);

  useEffect(() => {
    Promise.all([fetchDaemon(), fetchNginx()]).finally(() => setLoading(false));
    const iv = setInterval(() => { fetchDaemon(); fetchNginx(); }, 4000);
    return () => clearInterval(iv);
  }, [fetchDaemon, fetchNginx]);

  useEffect(() => {
    if (autoScrollRef.current) bottomRef.current?.scrollIntoView({ behavior: "instant" });
  }, [daemonLog, nginxLog, tab]);

  const log   = tab === "daemon" ? daemonLog : nginxLog;
  const empty = tab === "daemon" ? "No daemon logs yet." : "No nginx errors logged.";

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <h1 className="page-title">Logs</h1>
        <button className="btn-ghost" onClick={() => { fetchDaemon(); fetchNginx(); }}
          style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12 }}>
          <RefreshCw size={13} /> Refresh
        </button>
      </div>

      <div className={styles.tabs}>
        <button className={tab === "daemon" ? styles.tabActive : styles.tab} onClick={() => setTab("daemon")}>
          Daemon
        </button>
        <button className={tab === "nginx" ? styles.tabActive : styles.tab} onClick={() => setTab("nginx")}>
          Nginx errors
        </button>
      </div>

      <div className={styles.logBox}>
        {loading ? (
          <div className={styles.empty}>Loading…</div>
        ) : log ? (
          <>
            <pre className={styles.log}>{log}</pre>
            <div ref={bottomRef} />
          </>
        ) : (
          <div className={styles.empty}>{empty}</div>
        )}
      </div>

      <div className={styles.footer}>
        <label className={styles.autoScroll}>
          <input type="checkbox" defaultChecked onChange={(e) => { autoScrollRef.current = e.target.checked; }} />
          Auto-scroll to bottom
        </label>
      </div>
    </div>
  );
}
