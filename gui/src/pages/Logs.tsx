import { useEffect, useState } from "react";
import { api } from "../api/client";
import styles from "./Nginx.module.css"; // Reuse log styles

export default function Logs() {
  const [logs, setLogs] = useState("");
  const [loading, setLoading] = useState(true);

  const fetchLogs = async () => {
    try {
      const data = await api.daemon.logs();
      setLogs(data.logs);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchLogs();
    const iv = setInterval(fetchLogs, 4000);
    return () => clearInterval(iv);
  }, []);

  return (
    <div className={styles.page}>
      <h1 className={styles.title}>Daemon Logs</h1>
      <section className={styles.card}>
        <h2 className={styles.sectionTitle}>Last 100 lines of daemon.log</h2>
        {loading ? (
          <div className={styles.loading}>Loading logs…</div>
        ) : (
          <pre className={styles.log} style={{ color: "#a78bfa", maxHeight: "600px" }}>
            {logs || "No logs yet."}
          </pre>
        )}
      </section>
      <div style={{ marginTop: "10px", textAlign: "right" }}>
        <button className="btn-ghost" onClick={fetchLogs}>↺ Refresh now</button>
      </div>
    </div>
  );
}
