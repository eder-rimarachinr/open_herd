import { useCallback, useEffect, useRef, useState } from "react";
import { api, CatalogEntry, InstallProgress } from "../api/client";
import styles from "./PHP.module.css";

export default function PHP() {
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [rescanning, setRescanning] = useState(false);
  const [installs, setInstalls] = useState<Record<string, InstallProgress>>({});
  const [customDir, setCustomDir] = useState("");
  const pollRefs = useRef<Record<string, ReturnType<typeof setInterval>>>({});

  const fetchCatalog = useCallback(async () => {
    try {
      const data = await api.php.catalog();
      setCatalog(data);
    } catch {
      // daemon unreachable — keep showing last state
    }
  }, []);

  useEffect(() => {
    fetchCatalog().finally(() => setLoading(false));
    const iv = setInterval(fetchCatalog, 5000);
    return () => {
      clearInterval(iv);
      Object.values(pollRefs.current).forEach(clearInterval);
    };
  }, [fetchCatalog]);

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
    console.log("Config updated", updated);
    setCustomDir("");
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

                {/* Action buttons — only Install / Update */}
                <div className={styles.actionCell}>
                  {prog?.state === "error" && (
                    <span className={styles.errorText} title={prog.error}>Error</span>
                  )}
                  {!installing && !entry.installed && (
                    <button className={styles.btnInstall} onClick={() => handleInstall(entry.major)}>
                      Install
                    </button>
                  )}
                  {!installing && entry.installed && entry.has_update && (
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
