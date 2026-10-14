import { useCallback, useEffect, useRef, useState } from "react";
import { ShieldAlert, Lock } from "lucide-react";
import { api, AsyncTask, Site } from "../api/client";
import { useToast } from "../context/ToastContext";
import { TYPE_LABELS, avatarFor } from "../components/siteTypes";
import styles from "./SSL.module.css";

type SslOp = { state: AsyncTask["state"]; message: string; error?: string };

export default function SSL() {
  const { toast } = useToast();
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
            toast("SSL certificate enabled");
            await fetchSites();
            setOps((prev) => { const n = { ...prev }; delete n[id]; return n; });
          } else if (task.state === "error") {
            toast(task.error ?? "SSL failed", "error");
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
      toast("SSL disabled");
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
      <div>
        <h1 className="page-title">SSL Certificates</h1>
        <p className="page-sub">Certificates come from a local authority that only this computer trusts.</p>
      </div>

      <div className={styles.ca}>
        <span className={styles.caIcon}><Lock size={20} /></span>
        <div className={styles.caText}>
          <h2>Local certificate authority (mkcert)</h2>
          <p>
            Installed into the system trust store the first time you enable HTTPS (asks for
            admin rights once). After that, browsers show a padlock for every <code>.test</code> site.
          </p>
        </div>
        <span className={styles.caCount}>
          <b>{active}</b> of {sites.length} {sites.length === 1 ? "site" : "sites"} on HTTPS
        </span>
      </div>

      {sites.length === 0 ? (
        <div className={styles.empty}>
          <ShieldAlert size={32} className={styles.emptyIcon} />
          No sites registered yet.<br />
          Add a site from the Sites page first.
        </div>
      ) : (
        <div className="panel">
          <table className={styles.table}>
            <thead>
              <tr>
                <th>Site</th>
                <th>Type</th>
                <th>Status</th>
                <th>HTTPS</th>
              </tr>
            </thead>
            <tbody>
              {sites.map((site) => {
                const op   = ops[site.id];
                const busy = op && op.state !== "done" && op.state !== "error";
                const av   = avatarFor(site.project_type);

                return (
                  <tr key={site.id}>
                    <td>
                      <span className={styles.siteCell}>
                        <span className={styles.avatar} style={{ background: av.bg }}>{av.short}</span>
                        <span className={styles.domain}>{site.domain}</span>
                      </span>
                    </td>
                    <td><span className="badge badge-gray">{TYPE_LABELS[site.project_type] ?? site.project_type}</span></td>
                    <td>
                      {busy ? (
                        <span className={styles.progress}><span className="spinner" />{op.message || "Working…"}</span>
                      ) : op?.state === "error" ? (
                        <button
                          className={styles.errorBadge}
                          title="Click to dismiss"
                          onClick={() => setOps((prev) => { const n = { ...prev }; delete n[site.id]; return n; })}
                        >
                          {op.error ?? "Failed"}
                        </button>
                      ) : site.ssl_enabled ? (
                        <span className="badge badge-green">Active</span>
                      ) : (
                        <span className="badge badge-gray">Off</span>
                      )}
                    </td>
                    <td>
                      <button
                        className="switch"
                        role="switch"
                        aria-checked={site.ssl_enabled}
                        aria-label={`HTTPS for ${site.domain}`}
                        disabled={busy || op?.state === "error"}
                        onClick={() => site.ssl_enabled ? handleDisable(site.id) : handleEnable(site.id)}
                      />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
