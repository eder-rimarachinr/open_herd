import { useEffect, useState } from "react";
import { Save } from "lucide-react";
import { api, AppConfig } from "../api/client";

export default function Settings() {
  const [config,  setConfig]  = useState<AppConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving,  setSaving]  = useState(false);
  const [saved,   setSaved]   = useState(false);
  const [httpPort,  setHttpPort]  = useState("");
  const [httpsPort, setHttpsPort] = useState("");
  const [defaultPhp, setDefaultPhp] = useState("");

  useEffect(() => {
    api.config.get().then((cfg) => {
      setConfig(cfg);
      setHttpPort(String(cfg.http_port));
      setHttpsPort(String(cfg.https_port));
      setDefaultPhp(cfg.default_php);
    }).finally(() => setLoading(false));
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
      setTimeout(() => setSaved(false), 2000);
    } finally {
      setSaving(false);
    }
  }

  if (loading) return <div style={{ color: "var(--text-3)", padding: "48px 0", textAlign: "center" }}>Loading…</div>;

  return (
    <div style={{ maxWidth: 560 }}>
      <div className="page-header">
        <h1 className="page-title">Settings</h1>
      </div>

      <div style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: 12, overflow: "hidden", boxShadow: "var(--shadow-sm)", marginBottom: 20 }}>
        {[
          { label: "HTTP Port",   value: httpPort,   set: setHttpPort,   type: "number", hint: "Default: 80" },
          { label: "HTTPS Port",  value: httpsPort,  set: setHttpsPort,  type: "number", hint: "Default: 443" },
          { label: "Default PHP", value: defaultPhp, set: setDefaultPhp, type: "text",   hint: "e.g. 8.2" },
        ].map(({ label, value, set, type, hint }, i, arr) => (
          <div key={label} style={{
            display: "grid", gridTemplateColumns: "160px 1fr",
            borderBottom: i < arr.length - 1 ? "1px solid var(--border)" : "none",
          }}>
            <div style={{ padding: "14px 16px", background: "var(--surface-2)", borderRight: "1px solid var(--border)", display: "flex", flexDirection: "column", justifyContent: "center" }}>
              <span style={{ fontSize: 11, fontWeight: 700, textTransform: "uppercase", letterSpacing: "0.05em", color: "var(--text-3)" }}>{label}</span>
              <span style={{ fontSize: 10, color: "var(--text-3)", marginTop: 2 }}>{hint}</span>
            </div>
            <div style={{ padding: "12px 16px", display: "flex", alignItems: "center" }}>
              <input
                type={type}
                value={value}
                onChange={(e) => set(e.target.value)}
                style={{ background: "var(--bg)", border: "1px solid var(--border)", borderRadius: 5, padding: "6px 10px", fontSize: 13, color: "var(--text)", width: "100%", outline: "none" }}
                onFocus={(e) => e.target.style.borderColor = "var(--accent)"}
                onBlur={(e) => e.target.style.borderColor = "var(--border)"}
              />
            </div>
          </div>
        ))}
      </div>

      {config && (
        <div style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: 12, overflow: "hidden", boxShadow: "var(--shadow-sm)", marginBottom: 20 }}>
          {[
            { label: "Data directory", value: config.base_dir },
            { label: "Nginx directory", value: config.nginx_dir },
            { label: "PHP directory",  value: config.php_dir },
            { label: "Certs directory", value: config.certs_dir },
          ].map(({ label, value }, i, arr) => (
            <div key={label} style={{ display: "grid", gridTemplateColumns: "160px 1fr", borderBottom: i < arr.length - 1 ? "1px solid var(--border)" : "none" }}>
              <div style={{ padding: "12px 16px", background: "var(--surface-2)", borderRight: "1px solid var(--border)", display: "flex", alignItems: "center" }}>
                <span style={{ fontSize: 11, fontWeight: 700, textTransform: "uppercase", letterSpacing: "0.05em", color: "var(--text-3)" }}>{label}</span>
              </div>
              <div style={{ padding: "12px 16px", display: "flex", alignItems: "center" }}>
                <code style={{ fontSize: 11, fontFamily: "monospace", color: "var(--text-2)", wordBreak: "break-all" }}>{value}</code>
              </div>
            </div>
          ))}
        </div>
      )}

      <button
        onClick={handleSave}
        disabled={saving}
        style={{
          display: "inline-flex", alignItems: "center", gap: 6,
          background: saved ? "var(--green-surface)" : "var(--accent)",
          color: saved ? "var(--green)" : "#fff",
          border: saved ? "1px solid var(--green-border)" : "none",
          borderRadius: 6, padding: "8px 20px", fontSize: 13, fontWeight: 600,
          cursor: saving ? "not-allowed" : "pointer",
          opacity: saving ? 0.6 : 1,
          transition: "all 0.2s",
        }}
      >
        <Save size={14} />
        {saved ? "Saved!" : saving ? "Saving…" : "Save changes"}
      </button>
    </div>
  );
}
