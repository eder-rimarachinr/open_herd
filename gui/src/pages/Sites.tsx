import { useEffect, useState } from "react";
import { api, Site } from "../api/client";
import styles from "./Page.module.css";

export default function Sites() {
  const [sites, setSites] = useState<Site[]>([]);
  const [loading, setLoading] = useState(true);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.sites.list()
      .then(setSites)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  async function scan() {
    setScanning(true);
    try {
      const found = await api.sites.scan();
      if (found.length === 0) {
        alert("No new sites found.");
        return;
      }
      // Auto-add discovered sites.
      const added = await Promise.all(found.map((s) => api.sites.create(s)));
      setSites((prev) => [...prev, ...added]);
    } catch (e: any) {
      setError(e.message);
    } finally {
      setScanning(false);
    }
  }

  async function toggleSSL(site: Site) {
    try {
      const updated = site.ssl_enabled
        ? await api.sites.disableSSL(site.id)
        : await api.sites.enableSSL(site.id);
      setSites((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
    } catch (e: any) {
      setError(e.message);
    }
  }

  async function deleteSite(id: string) {
    if (!confirm("Remove this site?")) return;
    await api.sites.delete(id);
    setSites((prev) => prev.filter((s) => s.id !== id));
  }

  if (loading) return <div className={styles.empty}>Loading…</div>;

  return (
    <div>
      <div className={styles.header}>
        <h1 className={styles.title}>Sites</h1>
        <button className="btn-primary" onClick={scan} disabled={scanning}>
          {scanning ? "Scanning…" : "Scan for sites"}
        </button>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      {sites.length === 0 ? (
        <div className={styles.empty}>
          No sites yet. Add a scanned directory in Settings and click "Scan for sites".
        </div>
      ) : (
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Name</th>
              <th>Domain</th>
              <th>PHP</th>
              <th>Type</th>
              <th>SSL</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {sites.map((site) => (
              <tr key={site.id}>
                <td className={styles.bold}>{site.name}</td>
                <td>
                  <a href={`http${site.ssl_enabled ? "s" : ""}://${site.domain}`} target="_blank" rel="noreferrer">
                    {site.domain}
                  </a>
                </td>
                <td>
                  <span className="badge badge-blue">{site.php_version || "default"}</span>
                </td>
                <td>
                  <span className="badge badge-gray">{site.project_type}</span>
                </td>
                <td>
                  <button className="btn-ghost" onClick={() => toggleSSL(site)}>
                    {site.ssl_enabled ? "Disable SSL" : "Enable SSL"}
                  </button>
                </td>
                <td>
                  <button className="btn-danger" onClick={() => deleteSite(site.id)}>
                    Remove
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
