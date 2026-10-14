import { useEffect, useState } from "react";
import { Save, FolderPlus, FolderOpen, X, Plus, Check } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, AppConfig, PHPVersion } from "../api/client";
import { useToast } from "../context/ToastContext";
import styles from "./Settings.module.css";

export default function Settings() {
  const { toast } = useToast();
  const [config,     setConfig]     = useState<AppConfig | null>(null);
  const [phpVersions, setPhpVersions] = useState<PHPVersion[]>([]);
  const [loading,    setLoading]    = useState(true);
  const [offline,    setOffline]    = useState(false);
  const [saving,     setSaving]     = useState(false);
  const [saved,      setSaved]      = useState(false);
  const [httpPort,   setHttpPort]   = useState("80");
  const [httpsPort,  setHttpsPort]  = useState("443");
  const [defaultPhp, setDefaultPhp] = useState("8.2");
  const [newDir,     setNewDir]     = useState("");

  function showConfig(cfg: AppConfig) {
    setConfig(cfg);
    setHttpPort(String(cfg.http_port));
    setHttpsPort(String(cfg.https_port));
    setDefaultPhp(cfg.default_php);
  }

  useEffect(() => {
    api.config.get()
      .then((cfg) => { showConfig(cfg); setOffline(false); })
      .catch(() => setOffline(true))
      .finally(() => setLoading(false));
    api.php.versions().then(setPhpVersions).catch(() => {});
  }, []);

  const dirty = config !== null && (
    httpPort !== String(config.http_port) ||
    httpsPort !== String(config.https_port) ||
    defaultPhp !== config.default_php
  );

  async function handleSave() {
    setSaving(true);
    try {
      const updated = await api.config.update({
        http_port:   Number(httpPort),
        https_port:  Number(httpsPort),
        default_php: defaultPhp,
      });
      showConfig(updated);
      setSaved(true);
      toast("Settings saved");
      setTimeout(() => setSaved(false), 2000);
    } catch (e: any) {
      toast(e.message, "error");
      // Show what the server really has now (a busy port is rolled back).
      api.config.get().then(showConfig).catch(() => {});
    } finally {
      setSaving(false);
    }
  }

  async function addScannedDir(dir: string) {
    if (!dir || !config || config.scanned_dirs.includes(dir)) return;
    try {
      const updated = await api.config.update({ scanned_dirs: [...config.scanned_dirs, dir] });
      setConfig(updated);
      setNewDir("");
      toast("Directory added");
    } catch (e: any) { toast(e.message, "error"); }
  }

  async function removeScannedDir(dir: string) {
    if (!config) return;
    try {
      const updated = await api.config.update({ scanned_dirs: config.scanned_dirs.filter((d) => d !== dir) });
      setConfig(updated);
    } catch (e: any) { toast(e.message, "error"); }
  }

  async function browseScanDir() {
    try {
      const picked = await open({ directory: true, multiple: false });
      if (!picked) return;
      await addScannedDir(typeof picked === "string" ? picked : picked[0]);
    } catch {}
  }

  if (loading) return <div className={styles.loading}>Loading…</div>;

  if (offline) return (
    <div className={styles.page}>
      <h1 className="page-title">Settings</h1>
      <div className={styles.offline}>Daemon is offline — start it to load and save settings.</div>
    </div>
  );

  const phpOptions = phpVersions.some((v) => v.major === defaultPhp)
    ? phpVersions
    : [...phpVersions, { major: defaultPhp, version: defaultPhp } as PHPVersion];

  return (
    <div className={styles.page}>
      <div>
        <h1 className="page-title">Settings</h1>
        <p className="page-sub">
          Saved to <code>config.json</code>. Port changes restart nginx right away.
        </p>
      </div>

      {/* Network */}
      <div className="section-label">Network</div>
      <div className="panel">
        <Row label="HTTP port" hint="Default: 80" htmlFor="set-http">
          <input id="set-http" className={styles.field} type="number" value={httpPort} onChange={(e) => setHttpPort(e.target.value)} />
          <span className={styles.note}>Sites with HTTPS redirect from here</span>
        </Row>
        <Row label="HTTPS port" hint="Default: 443" htmlFor="set-https">
          <input id="set-https" className={styles.field} type="number" value={httpsPort} onChange={(e) => setHttpsPort(e.target.value)} />
          <span className={styles.note}>If another program holds a port, the previous ones are kept</span>
        </Row>
      </div>

      {/* PHP */}
      <div className="section-label">PHP</div>
      <div className="panel">
        <Row label="Default PHP" hint="Used for new sites" htmlFor="set-php">
          {phpVersions.length > 0 ? (
            <select id="set-php" className={styles.field} value={defaultPhp} onChange={(e) => setDefaultPhp(e.target.value)}>
              {phpOptions.map((v) => <option key={v.major} value={v.major}>{v.version}</option>)}
            </select>
          ) : (
            <input id="set-php" className={styles.field} type="text" value={defaultPhp} onChange={(e) => setDefaultPhp(e.target.value)} placeholder="8.2" />
          )}
        </Row>
      </div>

      <div className={styles.saveBar}>
        <button className="btn-primary" onClick={handleSave} disabled={saving || !dirty}>
          {saved ? <Check size={14} /> : <Save size={14} />}
          {saved ? "Saved" : saving ? "Saving…" : "Save changes"}
        </button>
        {dirty && <span className={styles.dirty}>Unsaved changes</span>}
      </div>

      {/* Scanned directories */}
      <div className="section-label">Scanned directories</div>
      <div className="panel">
        {(config?.scanned_dirs ?? []).length === 0 && (
          <div className={styles.emptyDirs}>No directories yet. Scan looks for projects in the folders you add here.</div>
        )}
        {(config?.scanned_dirs ?? []).map((dir) => (
          <div key={dir} className={styles.dirRow}>
            <FolderOpen size={14} className={styles.dirIcon} />
            <code className={styles.dirPath}>{dir}</code>
            <button className={styles.iconBtn} onClick={() => removeScannedDir(dir)} aria-label={`Remove ${dir}`} title="Remove directory">
              <X size={13} />
            </button>
          </div>
        ))}
        <div className={styles.dirAdd}>
          <input
            className={styles.dirInput}
            value={newDir}
            onChange={(e) => setNewDir(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addScannedDir(newDir.trim())}
            placeholder="C:\Projects"
            aria-label="New scanned directory"
          />
          <button className={styles.iconBtnBox} onClick={() => addScannedDir(newDir.trim())} aria-label="Add directory" title="Add directory">
            <Plus size={14} />
          </button>
          <button className={styles.iconBtnBox} onClick={browseScanDir} aria-label="Browse for a directory" title="Browse">
            <FolderPlus size={14} />
          </button>
        </div>
      </div>

      {/* Data paths (read-only) */}
      {config && (
        <>
          <div className="section-label">Data paths</div>
          <div className="panel">
            {[
              { label: "Base",  value: config.base_dir },
              { label: "Nginx", value: config.nginx_dir },
              { label: "PHP",   value: config.php_dir },
              { label: "Certs", value: config.certs_dir },
              { label: "Logs",  value: config.logs_dir },
            ].map(({ label, value }) => (
              <div key={label} className={styles.pathRow}>
                <span className={styles.pathLabel}>{label}</span>
                <code className={styles.pathValue}>{value}</code>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function Row({ label, hint, htmlFor, children }: { label: string; hint: string; htmlFor: string; children: React.ReactNode }) {
  return (
    <div className={styles.row}>
      <label className={styles.rowLabel} htmlFor={htmlFor}>
        <b>{label}</b>
        <span>{hint}</span>
      </label>
      <div className={styles.rowControl}>{children}</div>
    </div>
  );
}
