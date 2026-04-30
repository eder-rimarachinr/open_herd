import { useEffect, useState } from "react";
import { api, Site } from "../api/client";
import styles from "./Page.module.css";

export default function Sites() {
  const [sites, setSites] = useState<Site[]>([]);
  const [loading, setLoading] = useState(true);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [config, setConfig] = useState<any>(null);
  const [newDir, setNewDir] = useState("");
  const [sslLoading, setSslLoading] = useState<Record<string, boolean>>({});
  const [refreshLoading, setRefreshLoading] = useState<Record<string, boolean>>({});

  useEffect(() => {
    api.sites.list()
      .then(setSites)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));

    api.config.get().then(setConfig).catch(() => { });
  }, []);

  async function scan() {
    setScanning(true);
    setError(null);
    try {
      // Scan auto-persists new sites and prunes deleted ones; returns full list.
      const all = await api.sites.scan();
      setSites(all);
    } catch (e: any) {
      setError(e.message);
    } finally {
      setScanning(false);
    }
  }

  async function toggleSSL(site: Site) {
    setSslLoading((prev) => ({ ...prev, [site.id]: true }));
    try {
      if (site.ssl_enabled) {
        const updated = await api.sites.disableSSL(site.id);
        setSites((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
      } else {
        await api.sites.enableSSL(site.id);
        let task;
        do {
          await new Promise<void>((r) => setTimeout(r, 1000));
          task = await api.sites.sslProgress(site.id);
        } while (task.state === "pending" || task.state === "running");
        if (task.state === "error") throw new Error(task.error ?? task.message);
        api.sites.invalidate();
        setSites(await api.sites.list());
      }
    } catch (e: any) {
      setError(e.message);
    } finally {
      setSslLoading((prev) => ({ ...prev, [site.id]: false }));
    }
  }

  async function refreshConfig(site: Site) {
    setRefreshLoading((prev) => ({ ...prev, [site.id]: true }));
    try {
      const updated = await api.sites.refreshConfig(site.id);
      setSites((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
    } catch (e: any) {
      setError(e.message);
    } finally {
      setRefreshLoading((prev) => ({ ...prev, [site.id]: false }));
    }
  }

  async function deleteSite(id: string) {
    if (!confirm("Remove this site?")) return;
    await api.sites.delete(id);
    setSites((prev) => prev.filter((s) => s.id !== id));
  }

  async function addScannedDir() {
    const dir = newDir.trim();
    if (!dir || !config) return;
    try {
      const updated = await api.config.update({
        scanned_dirs: [...(config.scanned_dirs ?? []), dir],
      });
      setConfig(updated);
      setNewDir("");
    } catch (e: any) {
      setError(e.message);
    }
  }

  async function removeScannedDir(path: string) {
    if (!config) return;
    try {
      const updated = await api.config.update({
        scanned_dirs: config.scanned_dirs.filter((d: string) => d !== path),
      });
      setConfig(updated);
    } catch (e: any) {
      setError(e.message);
    }
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
          No sites yet. Add a scanned directory below and click "Scan for sites".
        </div>
      ) : (
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Name</th>
              <th>Domain</th>
              <th>PHP</th>
              <th>Type</th>
              <th>HTTPS</th>
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
                  <button 
                    className={site.ssl_enabled ? "btn-primary" : "btn-ghost"} 
                    onClick={() => toggleSSL(site)}
                    disabled={sslLoading[site.id]}
                    title={site.ssl_enabled ? "Site is secure (HTTPS)" : "Site is HTTP only"}
                  >
                    {sslLoading[site.id] ? "Processing…" : (site.ssl_enabled ? "Disable HTTPS" : "Enable HTTPS")}
                  </button>
                </td>
                <td style={{ display: "flex", gap: "6px" }}>
                  <button
                    className="btn-ghost"
                    onClick={() => refreshConfig(site)}
                    disabled={refreshLoading[site.id]}
                    title="Regenerate nginx config and reload"
                  >
                    {refreshLoading[site.id] ? "…" : "↺ Refresh"}
                  </button>
                  <button className="btn-danger" onClick={() => deleteSite(site.id)}>
                    Remove
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {/* Scanned Directories Management */}
      <section className={styles.section} style={{ marginTop: "40px" }}>
        <h2 className={styles.sectionTitle}>Scanned Directories</h2>
        <p className={styles.hint}>
          phpenv will scan these folders for projects (Laravel, WordPress, etc).
        </p>

        <div className={styles.pathList}>
          {config?.scanned_dirs?.map((path: string) => (
            <div key={path} className={styles.pathItem}>
              <code className={styles.pathLabel}>{path}</code>
              <button
                className={styles.btnRemovePath}
                onClick={() => removeScannedDir(path)}
                title="Remove path"
              >
                ✕
              </button>
            </div>
          ))}
        </div>

        <div className={styles.addRow}>
          <input
            className={styles.pathInput}
            type="text"
            placeholder="C:\Users\name\Projects"
            value={newDir}
            onChange={(e) => setNewDir(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addScannedDir()}
          />
          <button className={styles.btnAdd} onClick={addScannedDir}>
            Add Directory
          </button>
        </div>
      </section>
    </div>
  );
}
