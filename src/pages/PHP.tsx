import { useCallback, useEffect, useRef, useState } from "react";
import { RefreshCw, Check, Download, AlertCircle, X, Plus, Settings2, Play, Square, FileCog, FolderOpen } from "lucide-react";
import { api, AppConfig, CatalogEntry, InstallProgress, PhpExtension, PhpIniConfig, PhpSetting } from "../api/client";
import { phpPort } from "../api/ports";
import { useToast } from "../context/ToastContext";
import styles from "./PHP.module.css";

// ── Extension categories with display labels ──────────────────────────────────
const CATEGORY_LABELS: Record<string, string> = {
  database: "Database",
  string:   "String / Encoding / i18n",
  image:    "Image",
  network:  "Network / Mail",
  files:    "Files / Compression",
  security: "Security",
  math:     "Math",
  misc:     "XML / Misc",
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
  const [toggling, setToggling] = useState<Record<string, boolean>>({});
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

  async function handleStart(major: string) {
    setToggling(prev => ({ ...prev, [major]: true }));
    try {
      await api.php.start(major);
      await fetchCatalog();
    } catch (e: any) {
      toast(e.message, "error");
    } finally {
      setToggling(prev => ({ ...prev, [major]: false }));
    }
  }

  async function handleStop(major: string) {
    setToggling(prev => ({ ...prev, [major]: true }));
    try {
      await api.php.stop(major);
      await fetchCatalog();
    } catch (e: any) {
      toast(e.message, "error");
    } finally {
      setToggling(prev => ({ ...prev, [major]: false }));
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
    <div className={styles.page}>
      <div className={styles.header}>
        <div>
          <h1 className="page-title">PHP</h1>
          <p className="page-sub">
            Each installed version runs its own php-cgi; sites pick one in their detail panel.
          </p>
        </div>
        <button className={styles.btnGhost} onClick={handleRescan} disabled={rescanning}>
          <RefreshCw size={14} style={{ animation: rescanning ? "spin 0.6s linear infinite" : "none" }} />
          {rescanning ? "Scanning…" : "Rescan"}
        </button>
      </div>

      {/* Version catalog */}
      <div className="section-label">PHP Versions</div>

      {catalog.length === 0 && (
        <div className={styles.emptyTable}>
          No PHP versions detected.<br />
          Start the daemon and click Rescan to detect installed versions.
        </div>
      )}

      <div className={styles.grid}>
        {catalog.map((entry) => {
          const prog = installs[entry.major];
          const installing = prog && prog.state !== "done" && prog.state !== "error";
          const isOpen = iniPanel === entry.major;
          const port = phpPort(entry.major);

          return (
            <div
              key={entry.major}
              className={[
                styles.card,
                entry.end_of_life ? styles.eol : "",
                isOpen ? styles.cardSelected : "",
              ].join(" ")}
            >
              <div className={styles.cardTop}>
                <span className={styles.major}>{entry.major}</span>
                {entry.installed && <span className={styles.checkmark} title="Installed"><Check size={14} strokeWidth={2.5} /></span>}
                <span className={styles.chips}>
                  {entry.security_only && <span className={`${styles.tag} ${styles.tagSecurity}`}>security</span>}
                  {entry.end_of_life   && <span className={`${styles.tag} ${styles.tagEol}`}>EOL</span>}
                  {!entry.security_only && !entry.end_of_life && <span className={`${styles.tag} ${styles.tagActive}`}>supported</span>}
                </span>
              </div>

              <div className={styles.patch}>
                {entry.installed
                  ? entry.installed_patch || "installed"
                  : `latest ${entry.latest_patch}`}
                {entry.installed && entry.has_update && <span className={styles.updateNote}> · {entry.latest_patch} available</span>}
              </div>

              {installing && prog ? (
                <div className={styles.progress}>
                  <div className={styles.progressTrack}>
                    <div className={styles.progressFill} style={{ width: `${prog.percent}%` }} />
                  </div>
                  <span className={styles.progressLabel}>{prog.message}</span>
                </div>
              ) : (
                <div className={styles.cardFoot}>
                  {entry.installed ? (
                    <label className={styles.defaultLabel}>
                      <input
                        type="radio"
                        name="active_php"
                        checked={config?.default_php === entry.major}
                        onChange={() => handleSetActive(entry.major)}
                      />
                      Default
                    </label>
                  ) : (
                    <span className={styles.notInstalled}>Not installed</span>
                  )}

                  {entry.running ? (
                    <span className={styles.running}>
                      <span className="status-dot on" title="Running" />
                      {port ? `:${port}` : "on"}
                    </span>
                  ) : null}

                  <span className={styles.actions}>
                    {prog?.state === "error" && (
                      <button
                        className={styles.btnError}
                        title="Click to dismiss"
                        onClick={() => setInstalls((prev) => { const n = { ...prev }; delete n[entry.major]; return n; })}
                      >
                        <AlertCircle size={12} /> {prog.error ?? "Install failed"}
                      </button>
                    )}
                    {prog?.state !== "error" && !entry.installed && (
                      <button className={styles.btnInstall} onClick={() => handleInstall(entry.major)}>
                        <Download size={13} /> Install
                      </button>
                    )}
                    {prog?.state !== "error" && entry.installed && entry.has_update && (
                      <button className={styles.btnUpdate} onClick={() => handleInstall(entry.major)}>
                        Update
                      </button>
                    )}
                    {entry.installed && !entry.running && (
                      <button
                        className={styles.btnIcon}
                        onClick={() => handleStart(entry.major)}
                        disabled={toggling[entry.major]}
                        title={`Start PHP ${entry.major}`}
                      >
                        <Play size={13} />
                      </button>
                    )}
                    {entry.installed && entry.running && (
                      <button
                        className={[styles.btnIcon, styles.btnIconStop].join(" ")}
                        onClick={() => handleStop(entry.major)}
                        disabled={toggling[entry.major]}
                        title={`Stop PHP ${entry.major}`}
                      >
                        <Square size={12} />
                      </button>
                    )}
                    {entry.installed && (
                      <button
                        className={[styles.btnIcon, isOpen ? styles.btnIconActive : ""].join(" ")}
                        onClick={() => isOpen ? setIniPanel(null) : openIniPanel(entry.major)}
                        title="Configure php.ini extensions"
                        aria-expanded={isOpen}
                      >
                        <Settings2 size={14} />
                      </button>
                    )}
                  </span>
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* ── php.ini panel ──────────────────────────────────────────── */}
      {iniPanel && (
        <div className={["panel", styles.iniPanel].join(" ")}>
          <div className="panel-head">
            <FileCog size={15} className={styles.accentIcon} />
            php.ini · PHP {iniPanel}
            <span className={["sub", styles.iniPath].join(" ")}>{iniConfig?.ini_path}</span>
            <div className="right">
              <button className={styles.iniClose} onClick={() => setIniPanel(null)} aria-label="Close php.ini"><X size={14} /></button>
            </div>
          </div>

          {iniLoading ? (
            <div className={styles.iniLoading}>Loading php.ini…</div>
          ) : (
            <>
              <div className={styles.iniBody}>
                {/* ── Extensions ───────────────────────────────────── */}
                <div className={styles.iniExtensions}>
                  {grouped.map(({ cat, label: catLabel, exts }) => (
                    <div key={cat} className={styles.iniGroup}>
                      <div className={styles.iniGroupLabel}>{catLabel}</div>
                      {exts.map(ext => {
                        const on = iniEdits[ext.name] ?? ext.enabled;
                        return (
                          <div key={ext.name} className={styles.iniRow}>
                            <span className={styles.iniExtName}>{ext.name}</span>
                            <button
                              className="switch"
                              role="switch"
                              aria-checked={on}
                              aria-label={ext.name}
                              onClick={() => setIniEdits(prev => ({ ...prev, [ext.name]: !(prev[ext.name] ?? ext.enabled) }))}
                            />
                          </div>
                        );
                      })}
                    </div>
                  ))}
                </div>

                {/* ── Settings ─────────────────────────────────────── */}
                {iniConfig && iniConfig.settings.length > 0 && (
                  <div className={styles.iniSettings}>
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
                          aria-label={setting.label}
                          spellCheck={false}
                        />
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div className={styles.iniFooter}>
                <button className="btn-primary" onClick={saveIni} disabled={iniSaving}>
                  {iniSaving ? <span className="spinner" /> : null}
                  {iniSaving ? "Saving & restarting…" : "Save & restart services"}
                </button>
                <button className={styles.btnGhost} onClick={() => setIniPanel(null)}>
                  Cancel
                </button>
                <span className={styles.iniNote}>Changes restart php-cgi {iniPanel} and reload nginx.</span>
              </div>
            </>
          )}
        </div>
      )}

      {/* Custom paths */}
      <div className="section-label">Custom PHP paths</div>
      <div className="panel">
        <p className={styles.hint}>
          Open Herd auto-detects XAMPP, WAMP and Laragon. Add a directory if your PHP is
          installed elsewhere (e.g. <code>C:\my-tools\php83</code>).
        </p>
        {config?.custom_php_dirs?.map((path: string) => (
          <div key={path} className={styles.pathItem}>
            <FolderOpen size={14} className={styles.pathIcon} />
            <code className={styles.pathLabel}>{path}</code>
            <button className={styles.btnRemovePath} title="Remove path" onClick={() => handleRemoveCustomDir(path)}>
              <X size={13} />
            </button>
          </div>
        ))}
        <div className={styles.addRow}>
          <input
            className={styles.pathInput}
            type="text"
            placeholder="C:\path\to\php-dir"
            value={customDir}
            onChange={(e) => setCustomDir(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAddCustomDir()}
          />
          <button className={styles.btnGhost} onClick={handleAddCustomDir}>
            <Plus size={14} /> Add &amp; Rescan
          </button>
        </div>
      </div>
    </div>
  );
}
