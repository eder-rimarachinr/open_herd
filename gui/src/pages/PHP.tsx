import { useCallback, useEffect, useRef, useState } from "react";
import { api, CatalogEntry, InstallProgress } from "../api/client";
import styles from "./PHP.module.css";

export default function PHP() {
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [rescanning, setRescanning] = useState(false);
  const [installs, setInstalls] = useState<Record<string, InstallProgress>>({});
  const [customDir, setCustomDir] = useState("");
  const [config, setConfig] = useState<any>(null);
  const pollRefs = useRef<Record<string, ReturnType<typeof setInterval>>>({});

  const fetchCatalog = useCallback(async () => {
    try {
      const data = await api.php.catalog();
      setCatalog(data);
    } catch {
      // daemon unreachable — keep showing last state
    }
  }, []);

  const fetchConfig = useCallback(async () => {
    try {
      const cfg = await api.config.get();
      setConfig(cfg);
    } catch {}
  }, []);

  useEffect(() => {
    fetchCatalog().finally(() => setLoading(false));
    fetchConfig();
    const iv = setInterval(fetchCatalog, 5000);
    return () => {
      clearInterval(iv);
      Object.values(pollRefs.current).forEach(clearInterval);
    };
  }, [fetchCatalog, fetchConfig]);

  async function handleRescan() {
    setRescanning(true);
    try {
      const data = await api.php.detect();
      setCatalog(data);
    } finally {
      setRescanning(false);
    }
  }

  async function handleAddCustomDir() {
    const dir = customDir.trim();
    if (!dir) return;
    const cfg = await api.config.get();
    const updated = await api.config.update({
      custom_php_dirs: [...(cfg.custom_php_dirs ?? []), dir],
    });
    setConfig(updated);
    setCustomDir("");
    handleRescan();
  }

  async function handleRemoveCustomDir(path: string) {
    if (!config) return;
    const updated = await api.config.update({
      custom_php_dirs: config.custom_php_dirs.filter((p: string) => p !== path),
    });
    setConfig(updated);
    handleRescan();
  }

  function startPollingInstall(major: string) {
    if (pollRefs.current[major]) return;
    pollRefs.current[major] = setInterval(async () => {
      try {
        const prog = await api.php.installProgress(major);
        setInstalls((prev) => ({ ...prev, [major]: prog }));
        if (prog.state === "done" || prog.state === "error") {
          clearInterval(pollRefs.current[major]);
          delete pollRefs.current[major];
          fetchCatalog();
        }
      } catch {
        clearInterval(pollRefs.current[major]);
        delete pollRefs.current[major];
      }
    }, 800);
  }

  async function handleInstall(major: string) {
    setInstalls((prev) => ({
      ...prev,
      [major]: { major, state: "pending", message: "Starting…", percent: 0 },
    }));
    try {
      await api.php.install(major);
      startPollingInstall(major);
    } catch (e: any) {
      setInstalls((prev) => ({
        ...prev,
        [major]: { major, state: "error", message: "", percent: 0, error: e.message },
      }));
    }
  }

  async function handleSetActive(major: string) {
    if (!config || config.default_php === major) return;
    try {
      const updated = await api.config.update({ default_php: major });
      setConfig(updated);
      await api.services.start(); // restart services to apply new default
    } catch (e) {
      console.error("Failed to set active PHP", e);
    }
  }

  if (loading) return <div className={styles.loading}>Loading…</div>;

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <h1 className={styles.title}>PHP</h1>
        <button className={styles.btnRescan} onClick={handleRescan} disabled={rescanning}>
          {rescanning ? "Scanning…" : "↺ Rescan"}
        </button>
      </div>

      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Versions</h2>

        <div className={styles.table}>
          <div className={styles.thead}>
            <span>Version</span>
            <span>Installed</span>
            <span>Active</span>
            <span></span>
          </div>

          {catalog.map((entry) => {
            const prog = installs[entry.major];
            const installing = prog && prog.state !== "done" && prog.state !== "error";

            return (
              <div
                key={entry.major}
                className={[
                  styles.row,
                  entry.end_of_life ? styles.eol : "",
                  installing ? styles.rowInstalling : "",
                ].join(" ")}
              >
                {/* Progress bar overlay during install */}
                {installing && prog && (
                  <div className={styles.progressOverlay}>
                    <div
                      className={styles.progressFill}
                      style={{ width: `${prog.percent}%` }}
                    />
                    <span className={styles.progressLabel}>{prog.message}</span>
                  </div>
                )}

                {/* Version label */}
                <div className={styles.versionCell}>
                  <span className={styles.major}>
                    {entry.major}
                    {entry.installed_patch ? ` (${entry.installed_patch})` : ""}
                  </span>
                  {entry.security_only && (
                    <span className={styles.tag + " " + styles.tagSecurity}>security</span>
                  )}
                  {entry.end_of_life && (
                    <span className={styles.tag + " " + styles.tagEol}>EOL</span>
                  )}
                </div>

                {/* Installed checkmark */}
                <div className={styles.statusCell}>
                  {entry.installed && (
                    <span className={styles.checkmark}>✓</span>
                  )}
                </div>

                {/* Active Radio Button */}
                <div className={styles.activeCell}>
                  {entry.installed && (
                    <input
                      type="radio"
                      name="active_php"
                      checked={config?.default_php === entry.major}
                      onChange={() => handleSetActive(entry.major)}
                      style={{ cursor: "pointer" }}
                    />
                  )}
                </div>

                {/* Action buttons — only Install / Update */}
                <div className={styles.actionCell}>
                  {prog?.state === "error" && (
                    <button
                      className={styles.btnError}
                      title="Click to dismiss"
                      onClick={() => setInstalls((prev) => {
                        const next = { ...prev };
                        delete next[entry.major];
                        return next;
                      })}
                    >
                      ✕ {prog.error ?? "Install failed"}
                    </button>
                  )}
                  {!installing && prog?.state !== "error" && !entry.installed && (
                    <button className={styles.btnInstall} onClick={() => handleInstall(entry.major)}>
                      Install
                    </button>
                  )}
                  {!installing && prog?.state !== "error" && entry.installed && entry.has_update && (
                    <button className={styles.btnUpdate} onClick={() => handleInstall(entry.major)}>
                      Update
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </section>

      {/* Custom PHP paths */}
      <section className={styles.section}>
        <h2 className={styles.sectionTitle}>Custom PHP paths</h2>
        <p className={styles.hint}>
          phpenv auto-detects XAMPP, WAMP and Laragon. Add a directory here if your PHP is
          installed elsewhere (e.g. <code>C:\my-tools\php83</code>).
        </p>

        {/* List of existing custom paths */}
        <div className={styles.pathList}>
          {config?.custom_php_dirs?.map((path) => (
            <div key={path} className={styles.pathItem}>
              <code className={styles.pathLabel}>{path}</code>
              <button
                className={styles.btnRemovePath}
                onClick={() => handleRemoveCustomDir(path)}
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
            placeholder="C:\path\to\php-dir"
            value={customDir}
            onChange={(e) => setCustomDir(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAddCustomDir()}
          />
          <button className={styles.btnAdd} onClick={handleAddCustomDir}>
            Add &amp; Rescan
          </button>
        </div>
      </section>
    </div>
  );
}
