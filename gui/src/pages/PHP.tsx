import { useCallback, useEffect, useRef, useState } from "react";
import { api, CatalogEntry, InstallProgress } from "../api/client";
import styles from "./PHP.module.css";

export default function PHP() {
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [installs, setInstalls] = useState<Record<string, InstallProgress>>({});
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

  async function handleToggleFPM(entry: CatalogEntry) {
    try {
      if (entry.running) {
        await api.php.stop(entry.major);
      } else {
        await api.php.start(entry.major);
      }
      fetchCatalog();
    } catch (e: any) {
      alert(e.message);
    }
  }

  if (loading) return <div className={styles.loading}>Loading…</div>;

  return (
    <div className={styles.page}>
      <h1 className={styles.title}>PHP</h1>

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
                ].join(" ")}
              >
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

                {/* Install status column */}
                <div className={styles.statusCell}>
                  {entry.installed && (
                    <span className={styles.checkmark} title="Installed">✓</span>
                  )}
                  {entry.installed && entry.running && (
                    <span className={styles.dotRunning} title="FPM running" />
                  )}
                </div>

                {/* Action column */}
                <div className={styles.actionCell}>
                  {/* Install progress bar */}
                  {installing && prog && (
                    <div className={styles.progressWrap}>
                      <div
                        className={styles.progressBar}
                        style={{ width: `${prog.percent}%` }}
                      />
                      <span className={styles.progressMsg}>{prog.message}</span>
                    </div>
                  )}

                  {/* Error */}
                  {prog?.state === "error" && (
                    <span className={styles.errorText}>{prog.error}</span>
                  )}

                  {/* Buttons */}
                  {!installing && (
                    <>
                      {!entry.installed && (
                        <button
                          className={styles.btnInstall}
                          onClick={() => handleInstall(entry.major)}
                        >
                          Install
                        </button>
                      )}
                      {entry.installed && entry.has_update && (
                        <button
                          className={styles.btnUpdate}
                          onClick={() => handleInstall(entry.major)}
                        >
                          Update
                        </button>
                      )}
                      {entry.installed && (
                        <button
                          className={entry.running ? styles.btnStop : styles.btnStart}
                          onClick={() => handleToggleFPM(entry)}
                        >
                          {entry.running ? "Stop FPM" : "Start FPM"}
                        </button>
                      )}
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </section>
    </div>
  );
}
