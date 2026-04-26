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

  useEffect(() => {
    api.sites.list()
      .then(setSites)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));

    api.config.get().then(setConfig).catch(() => {});
  }, []);

  async function scan() {
    setScanning(true);
    try {
      const found = await api.sites.scan();
      if (found.length > 0) {
        // Bulk add only the differences.
        await api.sites.bulk(found);
      }
      // Re-fetch list to get the cleanup and new sites.
      const updated = await api.sites.list();
      setSites(updated);
      if (found.length === 0) {
        alert("No new sites found, but we updated the existing ones.");
      }
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
