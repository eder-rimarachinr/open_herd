import { useCallback, useEffect, useRef, useState } from "react";
import { RefreshCw, Search } from "lucide-react";
import { api } from "../api/client";
import LogView from "../components/LogView";
import styles from "./Logs.module.css";

type Tab = "daemon" | "nginx";

const POLL_MS = 4000;

export default function Logs() {
  const [tab,       setTab]       = useState<Tab>("daemon");
  const [daemonLog, setDaemonLog] = useState("");
  const [nginxLog,  setNginxLog]  = useState("");
  const [loading,   setLoading]   = useState(true);
  const [filter,    setFilter]    = useState("");
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
    const iv = setInterval(() => { fetchDaemon(); fetchNginx(); }, POLL_MS);
    return () => clearInterval(iv);
  }, [fetchDaemon, fetchNginx]);

  useEffect(() => {
    if (autoScrollRef.current) bottomRef.current?.scrollIntoView({ behavior: "instant" });
  }, [daemonLog, nginxLog, tab]);

  const log   = tab === "daemon" ? daemonLog : nginxLog;
  const empty = tab === "daemon" ? "No daemon logs yet." : "No nginx errors logged.";
  const nginxCount = nginxLog.split("\n").filter((l) => l.trim()).length;

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <div>
          <h1 className="page-title">Logs</h1>
          <p className="page-sub">Open Herd's own activity and nginx's error log.</p>
        </div>
        <div className={styles.headerActions}>
          <span className={styles.live}><span className="status-dot on" />Live · every {POLL_MS / 1000} s</span>
          <button className={styles.btnGhost} onClick={() => { fetchDaemon(); fetchNginx(); }}>
            <RefreshCw size={14} /> Refresh
          </button>
        </div>
      </div>

      <div className={styles.toolbar}>
        <div className={styles.seg} role="tablist">
          <button role="tab" aria-selected={tab === "daemon"} className={tab === "daemon" ? styles.segActive : styles.segBtn} onClick={() => setTab("daemon")}>
            Daemon
          </button>
          <button role="tab" aria-selected={tab === "nginx"} className={tab === "nginx" ? styles.segActive : styles.segBtn} onClick={() => setTab("nginx")}>
            Nginx errors
            {nginxCount > 0 && <span className={styles.count}>{nginxCount}</span>}
          </button>
        </div>
        <label className={styles.search}>
          <Search size={15} className={styles.searchIcon} />
          <input
            className={styles.searchInput}
            placeholder="Filter lines…"
            aria-label="Filter log lines"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </label>
      </div>

      <div className={["panel", styles.logBox].join(" ")}>
        {loading ? (
          <div className={styles.empty}>Loading…</div>
        ) : log ? (
          <>
            <LogView text={log} filter={filter} className={styles.logView} />
            <div ref={bottomRef} />
          </>
        ) : (
          <div className={styles.empty}>{empty}</div>
        )}
      </div>

      <label className={styles.autoScroll}>
        <input type="checkbox" defaultChecked onChange={(e) => { autoScrollRef.current = e.target.checked; }} />
        Auto-scroll to bottom
      </label>
    </div>
  );
}
