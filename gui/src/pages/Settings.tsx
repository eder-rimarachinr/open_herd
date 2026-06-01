import { useEffect, useState } from "react";
import { Save, FolderPlus, X } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, AppConfig } from "../api/client";
import { useToast } from "../context/ToastContext";

export default function Settings() {
  const { toast } = useToast();
  const [config,     setConfig]     = useState<AppConfig | null>(null);
  const [loading,    setLoading]    = useState(true);
  const [offline,    setOffline]    = useState(false);
  const [saving,     setSaving]     = useState(false);
  const [saved,      setSaved]      = useState(false);
  const [httpPort,   setHttpPort]   = useState("80");
  const [httpsPort,  setHttpsPort]  = useState("443");
  const [defaultPhp, setDefaultPhp] = useState("8.2");
  const [newDir,     setNewDir]     = useState("");

  useEffect(() => {
    api.config.get()
      .then((cfg) => {
        setConfig(cfg);
        setHttpPort(String(cfg.http_port));
        setHttpsPort(String(cfg.https_port));
        setDefaultPhp(cfg.default_php);
        setOffline(false);
      })
      .catch(() => setOffline(true))
      .finally(() => setLoading(false));
  }, []);

  async function handleSave() {
    setSaving(true);
    try {
      const updated = await api.config.update({
        http_port:   Number(httpPort),
        https_port:  Number(httpsPort),
        default_php: defaultPhp,
      });
      setConfig(updated);
      setSaved(true);
      toast("Settings saved");
      setTimeout(() => setSaved(false), 2000);
    } catch (e: any) {
      toast(e.message, "error");
    } finally {
      setSaving(false);
    }
  }

  async function addScannedDir() {
    const dir = newDir.trim();
    if (!dir || !config) return;
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
      if (!picked || !config) return;
      const dir = typeof picked === "string" ? picked : picked[0];
      if (!dir || config.scanned_dirs.includes(dir)) return;
      const updated = await api.config.update({ scanned_dirs: [...config.scanned_dirs, dir] });
      setConfig(updated);
      toast("Directory added");
    } catch {}
  }

  const card: React.CSSProperties = {
    background: "var(--surface)",
    border: "1px solid var(--border)",
    borderRadius: 12,
    overflow: "hidden",
    boxShadow: "var(--shadow-sm)",
    marginBottom: 20,
  };

  const labelCell: React.CSSProperties = {
    padding: "13px 16px",
    background: "var(--surface-2)",
    borderRight: "1px solid var(--border)",
    borderBottom: "1px solid var(--border)",
    display: "flex",
    flexDirection: "column",
    justifyContent: "center",
    gap: 2,
  };

  const valueCell: React.CSSProperties = {
    padding: "10px 16px",
    borderBottom: "1px solid var(--border)",
    display: "flex",
    alignItems: "center",
    gap: 8,
  };

  const inputStyle: React.CSSProperties = {
    background: "var(--bg)", border: "1px solid var(--border)",
    borderRadius: 5, padding: "6px 10px", fontSize: 13,
    color: "var(--text)", width: "100%", outline: "none",
    fontFamily: "inherit",
  };

  if (loading) return <div style={{ color: "var(--text-3)", padding: "48px 0", textAlign: "center" }}>Loading…</div>;

  if (offline) return (
    <div style={{ maxWidth: 480 }}>
      <div className="page-header"><h1 className="page-title">Settings</h1></div>
      <div style={{ background: "var(--amber-surface)", border: "1px solid var(--amber-border)", borderRadius: 8, padding: "14px 18px", color: "var(--amber)", fontSize: 13, lineHeight: 1.6 }}>
        Daemon is offline — start it to load and save settings.
      </div>
    </div>
  );

  return (
    <div style={{ maxWidth: 560 }}>
      <div className="page-header"><h1 className="page-title">Settings</h1></div>

      {/* General config */}
      <div style={card}>
        {[
          { label: "HTTP Port",   hint: "Default: 80",  value: httpPort,   set: setHttpPort,   type: "number" },
          { label: "HTTPS Port",  hint: "Default: 443", value: httpsPort,  set: setHttpsPort,  type: "number" },
          { label: "Default PHP", hint: "e.g. 8.2",     value: defaultPhp, set: setDefaultPhp, type: "text" },
        ].map(({ label, hint, value, set, type }, i, arr) => (
          <div key={label} style={{ display: "grid", gridTemplateColumns: "160px 1fr" }}>
            <div style={{ ...labelCell, borderBottom: i < arr.length - 1 ? "1px solid var(--border)" : "none" }}>
              <span style={{ fontSize: 11, fontWeight: 700, textTransform: "uppercase", letterSpacing: "0.05em", color: "var(--text-3)" }}>{label}</span>
              <span style={{ fontSize: 10, color: "var(--text-3)" }}>{hint}</span>
            </div>
            <div style={{ ...valueCell, borderBottom: i < arr.length - 1 ? "1px solid var(--border)" : "none" }}>
              <input
                type={type} value={value}
                onChange={(e) => set(e.target.value)}
                style={inputStyle}
                onFocus={(e) => { e.target.style.borderColor = "var(--accent)"; e.target.style.boxShadow = "0 0 0 3px var(--accent-surface)"; }}
                onBlur={(e)  => { e.target.style.borderColor = "var(--border)"; e.target.style.boxShadow = "none"; }}
              />
            </div>
          </div>
        ))}
      </div>

      {/* Scanned directories */}
      <div style={{ marginBottom: 8, fontSize: 13, fontWeight: 700, color: "var(--text)" }}>Scanned directories</div>
      <div style={{ ...card }}>
        {(config?.scanned_dirs ?? []).length === 0 && (
          <div style={{ padding: "16px", fontSize: 12, color: "var(--text-3)", textAlign: "center" }}>
            No directories configured. Add one below.
          </div>
        )}
        {(config?.scanned_dirs ?? []).map((dir, i, arr) => (
          <div key={dir} style={{ display: "flex", alignItems: "center", gap: 10, padding: "10px 16px", borderBottom: i < arr.length - 1 ? "1px solid var(--border)" : "none" }}>
            <code style={{ flex: 1, fontSize: 12, fontFamily: "monospace", color: "var(--text-2)", wordBreak: "break-all" }}>{dir}</code>
            <button
              onClick={() => removeScannedDir(dir)}
              style={{ background: "none", border: "none", color: "var(--text-3)", cursor: "pointer", padding: "2px 4px", borderRadius: 3, display: "flex", transition: "color 0.15s" }}
              onMouseEnter={(e) => (e.currentTarget.style.color = "var(--red)")}
              onMouseLeave={(e) => (e.currentTarget.style.color = "var(--text-3)")}
            >
              <X size={14} />
            </button>
          </div>
        ))}
        <div style={{ display: "flex", gap: 6, padding: "10px 12px", borderTop: (config?.scanned_dirs ?? []).length > 0 ? "1px solid var(--border)" : "none" }}>
          <input
            value={newDir}
            onChange={(e) => setNewDir(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && addScannedDir()}
            placeholder="C:\Projects"
            style={{ ...inputStyle, flex: 1, fontSize: 12, fontFamily: "monospace" }}
          />
          <button
            onClick={browseScanDir}
            style={{ background: "var(--surface-2)", border: "1px solid var(--border)", borderRadius: 5, padding: "5px 10px", color: "var(--text-2)", cursor: "pointer", display: "flex", alignItems: "center" }}
          >
            <FolderPlus size={14} />
          </button>
        </div>
      </div>

      {/* Data paths (read-only) */}
      {config && (
        <>
          <div style={{ marginBottom: 8, fontSize: 13, fontWeight: 700, color: "var(--text)" }}>Data paths</div>
          <div style={{ ...card, marginBottom: 24 }}>
            {[
              { label: "Base",   value: config.base_dir },
              { label: "Nginx",  value: config.nginx_dir },
              { label: "PHP",    value: config.php_dir },
              { label: "Certs",  value: config.certs_dir },
              { label: "Logs",   value: config.logs_dir },
            ].map(({ label, value }, i, arr) => (
              <div key={label} style={{ display: "grid", gridTemplateColumns: "80px 1fr" }}>
                <div style={{ ...labelCell, borderBottom: i < arr.length - 1 ? "1px solid var(--border)" : "none" }}>
                  <span style={{ fontSize: 11, fontWeight: 700, textTransform: "uppercase", letterSpacing: "0.05em", color: "var(--text-3)" }}>{label}</span>
                </div>
                <div style={{ ...valueCell, borderBottom: i < arr.length - 1 ? "1px solid var(--border)" : "none" }}>
                  <code style={{ fontSize: 11, fontFamily: "monospace", color: "var(--text-2)", wordBreak: "break-all" }}>{value}</code>
                </div>
              </div>
            ))}
          </div>
        </>
      )}

      <button
        onClick={handleSave}
        disabled={saving}
        style={{
          display: "inline-flex", alignItems: "center", gap: 6,
          background: saved ? "var(--green-surface)" : "var(--accent)",
          color: saved ? "var(--green)" : "#fff",
          border: saved ? "1px solid var(--green-border)" : "none",
          borderRadius: 6, padding: "9px 22px",
          fontSize: 13, fontWeight: 600, cursor: saving ? "not-allowed" : "pointer",
          opacity: saving ? 0.6 : 1, transition: "all 0.2s",
        }}
      >
        <Save size={14} />
        {saved ? "Saved!" : saving ? "Saving…" : "Save changes"}
      </button>
    </div>
  );
}
