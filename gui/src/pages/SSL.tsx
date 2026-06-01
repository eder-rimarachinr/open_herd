import { useCallback, useEffect, useRef, useState } from "react";
import { ShieldCheck, ShieldOff, ShieldAlert, Circle } from "lucide-react";
import { api, AsyncTask, Site } from "../api/client";
import styles from "./SSL.module.css";

type SslOp = { state: AsyncTask["state"]; message: string; error?: string };

export default function SSL() {
  const [sites,   setSites]   = useState<Site[]>([]);
  const [loading, setLoading] = useState(true);
  const [ops,     setOps]     = useState<Record<string, SslOp>>({});
  const pollRefs = useRef<Record<string, ReturnType<typeof setInterval>>>({});

  const fetchSites = useCallback(async () => {
    api.sites.invalidate();
    const data = await api.sites.list();
    setSites(data.sort((a, b) => a.domain.localeCompare(b.domain)));
  }, []);

  useEffect(() => {
    fetchSites().finally(() => setLoading(false));
    return () => { Object.values(pollRefs.current).forEach(clearInterval); };
  }, [fetchSites]);

  function startPolling(id: string) {
    if (pollRefs.current[id]) return;
    pollRefs.current[id] = setInterval(async () => {
      try {
        const task = await api.sites.sslProgress(id);
        setOps((prev) => ({ ...prev, [id]: { state: task.state, message: task.message, error: task.error } }));
        if (task.state === "done" || task.state === "error") {
          clearInterval(pollRefs.current[id]);
          delete pollRefs.current[id];
          if (task.state === "done") {
            await fetchSites();
            setOps((prev) => { const n = { ...prev }; delete n[id]; return n; });
          }
        }
      } catch {
        clearInterval(pollRefs.current[id]);
        delete pollRefs.current[id];
      }
    }, 800);
  }

  async function handleEnable(id: string) {
    setOps((prev) => ({ ...prev, [id]: { state: "pending", message: "Starting…" } }));
    try {
      const task = await api.sites.enableSSL(id);
      setOps((prev) => ({ ...prev, [id]: { state: task.state, message: task.message } }));
      startPolling(id);
    } catch (e: any) {
      setOps((prev) => ({ ...prev, [id]: { state: "error", message: "", error: e.message } }));
    }
  }

  async function handleDisable(id: string) {
    setOps((prev) => ({ ...prev, [id]: { state: "pending", message: "Disabling…" } }));
    try {
      await api.sites.disableSSL(id);
      await fetchSites();
      setOps((prev) => { const n = { ...prev }; delete n[id]; return n; });
    } catch (e: any) {
      setOps((prev) => ({ ...prev, [id]: { state: "error", message: "", error: e.message } }));
    }
  }

  if (loading) return <div className={styles.loading}>Loading…</div>;

  const active = sites.filter((s) => s.ssl_enabled).length;

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <h1 className="page-title">SSL Certificates</h1>
        <span className={styles.count}>{active} active</span>
      </div>

      {sites.length === 0 ? (
        <div className={styles.empty}>
          <ShieldAlert size={32} style={{ color: "var(--border-2)", marginBottom: 12 }} />
          No sites registered yet.<br />
          Add a site from the Sites page first.
        </div>
      ) : (
        <div className={styles.list}>
          {sites.map((site) => {
            const op   = ops[site.id];
            const busy = op && op.state !== "done" && op.state !== "error";

            return (
              <div key={site.id} className={styles.row}>
                <div className={styles.info}>
                  <span className={styles.domain}>{site.domain}</span>
                  <span className="badge badge-gray" style={{ fontSize: 10 }}>
                    {site.project_type}
                  </span>
                </div>

                <div className={styles.statusCol}>
                  {busy ? (
                    <span className={styles.progress}>{op.message || "Working…"}</span>
                  ) : op?.state === "error" ? (
                    <span
                      className={styles.errorBadge}
                      title="Click to dismiss"
                      onClick={() => setOps((prev) => { const n = { ...prev }; delete n[site.id]; return n; })}
                    >
                      {op.error ?? "Failed"}
                    </span>
                  ) : site.ssl_enabled ? (
                    <span className={styles.dotActive}>
                      <Circle size={7} fill="currentColor" /> Active
                    </span>
                  ) : (
                    <span className={styles.dotInactive}>
                      <Circle size={7} fill="currentColor" /> Off
                    </span>
                  )}
                </div>

                <div className={styles.action}>
                  {!busy && op?.state !== "error" && (
                    site.ssl_enabled ? (
                      <button className={styles.btnDisable} onClick={() => handleDisable(site.id)}>
                        <ShieldOff size={12} /> Disable
                      </button>
                    ) : (
                      <button className={styles.btnEnable} onClick={() => handleEnable(site.id)}>
                        <ShieldCheck size={12} /> Enable
                      </button>
                    )
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}

      <p className={styles.hint}>
        Certificates are issued by a local mkcert CA. On first use the CA is installed into the
        system trust store (requires admin/sudo).
      </p>
    </div>
  );
}
