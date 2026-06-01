import { useCallback, useEffect, useRef, useState } from "react";
import { RefreshCw, Check, Download, AlertCircle, X, Plus, Settings2, ChevronRight } from "lucide-react";
import { api, AppConfig, CatalogEntry, InstallProgress, PhpExtension, PhpIniConfig, PhpSetting } from "../api/client";
import { useToast } from "../context/ToastContext";
import styles from "./PHP.module.css";

// ── Extension categories with display labels ──────────────────────────────────
const CATEGORY_LABELS: Record<string, string> = {
  database: "Database",
  string:   "String / Encoding",
  image:    "Image",
  network:  "Network / Mail",
  files:    "Files / Compression",
  security: "Security",
  math:     "Math",
  misc:     "Miscellaneous",
};

export default function PHP() {
  const { toast } = useToast();
  const [catalog,    setCatalog]    = useState<CatalogEntry[]>([]);
  const [loading,    setLoading]    = useState(true);
  const [rescanning, setRescanning] = useState(false);
  const [installs,   setInstalls]   = useState<Record<string, InstallProgress>>({});
  const [customDir,  setCustomDir]  = useState("");
  const [config,     setConfig]     = useState<AppConfig | null>(null);
  const [iniPanel,   setIniPanel]   = useState<string | null>(null); // major version with open panel
  const [iniConfig,  setIniConfig]  = useState<PhpIniConfig | null>(null);
  const [iniLoading, setIniLoading] = useState(false);
  const [iniSaving,  setIniSaving]  = useState(false);
  const [iniEdits,    setIniEdits]    = useState<Record<string, boolean>>({}); // extension toggles
  const [settingEdits, setSettingEdits] = useState<Record<string, string>>({}); // setting values
  const pollRefs = useRef<Record<string, ReturnType<typeof setInterval>>>({});

  const fetchCatalog = useCallback(async () => {
    try { setCatalog(await api.php.catalog()); } catch {}
  }, []);

  const fetchConfig = useCallback(async () => {
    try { setConfig(await api.config.get()); } catch {}
  }, []);

  useEffect(() => {
    fetchCatalog().finally(() => setLoading(false));
    fetchConfig();
    const iv = setInterval(fetchCatalog, 5000);
    return () => { clearInterval(iv); Object.values(pollRefs.current).forEach(clearInterval); };
  }, [fetchCatalog, fetchConfig]);

  // ── php.ini panel ─────────────────────────────────────────────────────────
  async function openIniPanel(major: string) {
    setIniPanel(major);
    setIniConfig(null);
    setIniEdits({});
    setIniLoading(true);
    try {
      const data = await api.php.getIni(major);
      setIniConfig(data);
      // Seed extension toggles
      const extInit: Record<string, boolean> = {};
      data.extensions.forEach(e => { extInit[e.name] = e.enabled; });
      setIniEdits(extInit);
      // Seed setting values
      const setInit: Record<string, string> = {};
      data.settings.forEach(s => { setInit[s.key] = s.value; });
      setSettingEdits(setInit);
    } catch (e: any) {
      toast(e.message, "error");
      setIniPanel(null);
    } finally {
      setIniLoading(false);
    }
  }

  async function saveIni() {
    if (!iniPanel || !iniConfig) return;
    setIniSaving(true);
    try {
      const extensions: PhpExtension[] = iniConfig.extensions.map(e => ({
        ...e,
        enabled: iniEdits[e.name] ?? e.enabled,
      }));
      const settings: PhpSetting[] = iniConfig.settings.map(s => ({
        ...s,
        value: settingEdits[s.key] ?? s.value,
      }));
      await api.php.updateIni(iniPanel, extensions, settings);
      toast(`PHP ${iniPanel} restarted + Nginx reloaded — settings active`);
      setIniPanel(null);
      fetchCatalog();
    } catch (e: any) {
      toast(e.message, "error");
    } finally {
      setIniSaving(false);
    }
  }

  // ── Install polling ───────────────────────────────────────────────────────
  async function handleRescan() {
    setRescanning(true);
    try { setCatalog(await api.php.detect()); }
    finally { setRescanning(false); }
  }

  async function handleAddCustomDir() {
    const dir = customDir.trim();
    if (!dir) return;
    const cfg = await api.config.get();
    const updated = await api.config.update({ custom_php_dirs: [...(cfg.custom_php_dirs ?? []), dir] });
    setConfig(updated);
    setCustomDir("");
    handleRescan();
  }

  async function handleRemoveCustomDir(path: string) {
    if (!config) return;
    const updated = await api.config.update({ custom_php_dirs: config.custom_php_dirs.filter((p: string) => p !== path) });
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
          if (prog.state === "done") toast(`PHP ${major} installed successfully`);
          if (prog.state === "error") toast(prog.error ?? `PHP ${major} install failed`, "error");
        }
      } catch {
        clearInterval(pollRefs.current[major]);
        delete pollRefs.current[major];
      }
    }, 800);
  }

  async function handleInstall(major: string) {
    setInstalls((prev) => ({ ...prev, [major]: { major, state: "pending", message: "Starting…", percent: 0 } }));
    try {
      await api.php.install(major);
      startPollingInstall(major);
    } catch (e: any) {
      setInstalls((prev) => ({ ...prev, [major]: { major, state: "error", message: "", percent: 0, error: e.message } }));
    }
  }

  async function handleSetActive(major: string) {
    if (!config || config.default_php === major) return;
    try {
      const updated = await api.config.update({ default_php: major });
      setConfig(updated);
      await api.services.start();
    } catch (e) { console.error(e); }
  }

  if (loading) return <div className={styles.loading}>Loading…</div>;

  // Group extensions by category for the panel
  const grouped = iniConfig
    ? Object.entries(CATEGORY_LABELS).map(([cat, label]) => ({
        cat, label,
        exts: iniConfig.extensions.filter(e => e.category === cat),
      })).filter(g => g.exts.length > 0)
    : [];

  return (
    <div style={{ display: "flex", gap: 24, alignItems: "flex-start" }}>
      {/* ── Main content ───────────────────────────────────────────── */}
      <div className={styles.page} style={{ flex: 1, minWidth: 0 }}>
        <div className={styles.header}>
          <h1 className="page-title">PHP</h1>
          <button
            className="btn-ghost"
            onClick={handleRescan}
            disabled={rescanning}
            style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12 }}
          >
            <RefreshCw size={13} style={{ animation: rescanning ? "spin 0.6s linear infinite" : "none" }} />
            {rescanning ? "Scanning…" : "Rescan"}
          </button>
        </div>

        {/* Version catalog */}
        <section className={styles.section}>
          <div className={styles.sectionTitle}>PHP Versions</div>
          <div className={styles.table}>
            <div className={styles.thead}>
              <div className={styles.theadCell}>Version</div>
              <div className={styles.theadCell}>Installed</div>
              <div className={styles.theadCell}>Active</div>
              <div className={styles.theadCell} style={{ textAlign: "right" }}>Action</div>
            </div>

            {catalog.length === 0 && (
              <div className={styles.emptyTable}>
                No PHP versions detected.<br />
                Start the daemon and click Rescan to detect installed versions.
              </div>
            )}

            {catalog.map((entry) => {
              const prog = installs[entry.major];
              const installing = prog && prog.state !== "done" && prog.state !== "error";
              const isOpen = iniPanel === entry.major;

              return (
                <div
                  key={entry.major}
                  className={[
                    styles.row,
                    entry.end_of_life ? styles.eol : "",
                    installing ? styles.rowInstalling : "",
                    isOpen ? styles.rowSelected : "",
                  ].join(" ")}
                >
                  {installing && prog && (
                    <div className={styles.progressOverlay}>
                      <div className={styles.progressFill} style={{ width: `${prog.percent}%` }} />
                      <span className={styles.progressLabel}>{prog.message}</span>
                    </div>
                  )}

                  <div className={styles.versionCell}>
                    <span className={styles.major}>
                      PHP {entry.major}
                      {entry.installed_patch ? ` (${entry.installed_patch})` : ""}
                    </span>
                    {entry.security_only && <span className={`${styles.tag} ${styles.tagSecurity}`}>security</span>}
                    {entry.end_of_life   && <span className={`${styles.tag} ${styles.tagEol}`}>EOL</span>}
                  </div>

                  <div className={styles.statusCell}>
                    {entry.installed && <span className={styles.checkmark}><Check size={15} strokeWidth={2.5} /></span>}
                  </div>

                  <div className={styles.activeCell}>
                    {entry.installed && (
                      <input
                        type="radio"
                        name="active_php"
                        checked={config?.default_php === entry.major}
                        onChange={() => handleSetActive(entry.major)}
                        style={{ cursor: "pointer", accentColor: "var(--accent)" }}
                      />
                    )}
                  </div>

                  <div className={styles.actionCell}>
                    {prog?.state === "error" && (
                      <button
                        className={styles.btnError}
                        title="Click to dismiss"
                        onClick={() => setInstalls((prev) => { const n = { ...prev }; delete n[entry.major]; return n; })}
                      >
                        <AlertCircle size={11} /> {prog.error ?? "Install failed"}
                      </button>
                    )}
                    {!installing && prog?.state !== "error" && !entry.installed && (
                      <button className={styles.btnInstall} onClick={() => handleInstall(entry.major)}>
                        <Download size={12} /> Install
                      </button>
                    )}
                    {!installing && prog?.state !== "error" && entry.installed && entry.has_update && (
                      <button className={styles.btnUpdate} onClick={() => handleInstall(entry.major)}>
                        Update
                      </button>
                    )}
                    {!installing && entry.installed && (
                      <button
                        className={[styles.btnConfigure, isOpen ? styles.btnConfigureActive : ""].join(" ")}
                        onClick={() => isOpen ? setIniPanel(null) : openIniPanel(entry.major)}
                        title="Configure php.ini extensions"
                      >
                        <Settings2 size={12} />
                        {isOpen ? <ChevronRight size={11} style={{ transform: "rotate(90deg)" }} /> : <ChevronRight size={11} />}
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </section>

        {/* Custom paths */}
        <section className={styles.section}>
          <div className={styles.sectionTitle}>Custom PHP paths</div>
          <p className={styles.hint}>
            Open Herd auto-detects XAMPP, WAMP, and Laragon. Add a directory if your PHP is
            installed elsewhere (e.g. <code>C:\my-tools\php83</code>).
          </p>
          <div className={styles.pathList}>
            {config?.custom_php_dirs?.map((path: string) => (
              <div key={path} className={styles.pathItem}>
                <code className={styles.pathLabel}>{path}</code>
                <button className={styles.btnRemovePath} onClick={() => handleRemoveCustomDir(path)}>
                  <X size={13} />
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
              <Plus size={13} /> Add &amp; Rescan
            </button>
          </div>
        </section>
      </div>

      {/* ── php.ini panel ──────────────────────────────────────────── */}
      {iniPanel && (
        <div className={styles.iniPanel}>
          <div className={styles.iniPanelHeader}>
            <div>
              <span className={styles.iniPanelTitle}>PHP {iniPanel} — Extensions</span>
              <span className={styles.iniPanelPath}>{iniConfig?.ini_path}</span>
            </div>
            <button className={styles.iniClose} onClick={() => setIniPanel(null)}><X size={14} /></button>
          </div>

          {iniLoading ? (
            <div className={styles.iniLoading}>Loading php.ini…</div>
          ) : (
            <>
              <div className={styles.iniScroll}>
                {/* ── Settings section ─────────────────────────────── */}
                {iniConfig && iniConfig.settings.length > 0 && (
                  <div className={styles.iniGroup}>
                    <div className={styles.iniGroupLabel}>PHP Settings</div>
                    {iniConfig.settings.map(setting => (
                      <div key={setting.key} className={styles.iniSettingRow}>
                        <div className={styles.iniSettingMeta}>
                          <span className={styles.iniSettingLabel}>{setting.label}</span>
                          <span className={styles.iniSettingHint}>{setting.hint}</span>
                        </div>
                        <input
                          className={styles.iniSettingInput}
                          value={settingEdits[setting.key] ?? setting.value}
                          onChange={e => setSettingEdits(prev => ({ ...prev, [setting.key]: e.target.value }))}
                          placeholder={setting.value || "default"}
                          spellCheck={false}
                        />
                      </div>
                    ))}
                  </div>
                )}

                <div className={styles.iniDivider} />

                {/* ── Extensions section ───────────────────────────── */}
                <div className={styles.iniGroupLabel} style={{ paddingTop: 12 }}>Extensions</div>
                {grouped.map(({ cat, label: catLabel, exts }) => (
                  <div key={cat} className={styles.iniGroup}>
                    <div className={styles.iniGroupLabel}>{catLabel}</div>
                    {exts.map(ext => (
                      <label key={ext.name} className={styles.iniRow}>
                        <span className={styles.iniExtName}>{ext.name}</span>
                        <div
                          className={[styles.toggle, (iniEdits[ext.name] ?? ext.enabled) ? styles.toggleOn : ""].join(" ")}
                          onClick={() => setIniEdits(prev => ({ ...prev, [ext.name]: !(prev[ext.name] ?? ext.enabled) }))}
                        >
                          <div className={styles.toggleThumb} />
                        </div>
                      </label>
                    ))}
                  </div>
                ))}
              </div>

              <div className={styles.iniFooter}>
                <button
                  className={styles.iniSave}
                  onClick={saveIni}
                  disabled={iniSaving}
                >
                  {iniSaving ? <span className="spinner" /> : null}
                  {iniSaving ? "Saving & restarting…" : "Save & restart services"}
                </button>
                <button className={styles.iniCancel} onClick={() => setIniPanel(null)}>
                  Cancel
                </button>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
