import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, Site, PHPVersion, SiteInfo } from "../api/client";
import styles from "./Page.module.css";

const sortSites = (s: Site[]) => [...s].sort((a, b) => a.domain.localeCompare(b.domain));

const TYPE_LABELS: Record<string, string> = {
  laravel: "Laravel",
  wordpress: "WordPress",
  codeigniter4: "CodeIgniter 4",
  codeigniter3: "CodeIgniter 3",
  spa: "SPA",
  static: "Static",
  generic: "Generic",
};

type Tab = "general" | "info";

export default function Sites() {
  const [sites, setSites] = useState<Site[]>([]);
  const [loading, setLoading] = useState(true);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("general");
  const [phpVersions, setPhpVersions] = useState<PHPVersion[]>([]);
  const [siteInfo, setSiteInfo] = useState<SiteInfo | null>(null);
  const [infoLoading, setInfoLoading] = useState(false);
  const [sslLoading, setSslLoading] = useState(false);
  const [refreshLoading, setRefreshLoading] = useState(false);
  const [folderLoading, setFolderLoading] = useState(false);
  const [changingPhp, setChangingPhp] = useState(false);
  const [deleteLoading, setDeleteLoading] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [config, setConfig] = useState<any>(null);
  const [newDir, setNewDir] = useState("");

  const selected = sites.find((s) => s.id === selectedId) ?? null;
  const siteUrl = selected
    ? `http${selected.ssl_enabled ? "s" : ""}://${selected.domain}`
    : "";

  useEffect(() => {
    Promise.all([api.sites.list(), api.php.versions(), api.config.get()])
      .then(([s, php, cfg]) => {
        setSites(sortSites(s));
        setPhpVersions(php);
        setConfig(cfg);
      })
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (tab !== "info" || !selectedId) return;
    setInfoLoading(true);
    setSiteInfo(null);
    api.sites.info(selectedId)
      .then(setSiteInfo)
      .catch(() => setSiteInfo(null))
      .finally(() => setInfoLoading(false));
  }, [tab, selectedId]);

  function selectSite(id: string) {
    setSelectedId(id);
    setTab("general");
    setSiteInfo(null);
    setError(null);
    setConfirmDelete(false);
  }

  async function scan() {
    setScanning(true);
    setError(null);
    try {
      setSites(sortSites(await api.sites.scan()));
    } catch (e: any) {
      setError(e.message);
    } finally {
      setScanning(false);
    }
  }

  async function toggleSSL() {
    if (!selected) return;
    setSslLoading(true);
    setError(null);
    try {
      if (selected.ssl_enabled) {
        const updated = await api.sites.disableSSL(selected.id);
        setSites((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
      } else {
        await api.sites.enableSSL(selected.id);
        let task;
        do {
          await new Promise<void>((r) => setTimeout(r, 1000));
          task = await api.sites.sslProgress(selected.id);
        } while (task.state === "pending" || task.state === "running");
        if (task.state === "error") throw new Error(task.error ?? task.message);
        api.sites.invalidate();
        setSites(sortSites(await api.sites.list()));
      }
    } catch (e: any) {
      setError(e.message);
    } finally {
      setSslLoading(false);
    }
  }

  async function refreshConfig() {
    if (!selected) return;
    setRefreshLoading(true);
    setError(null);
    try {
      const updated = await api.sites.refreshConfig(selected.id);
      setSites((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
    } catch (e: any) {
      setError(e.message);
    } finally {
      setRefreshLoading(false);
    }
  }

  async function deleteSite() {
    if (!selected) return;
    setDeleteLoading(true);
    setError(null);
    try {
      await api.sites.delete(selected.id);
      setSites((prev) => prev.filter((s) => s.id !== selected.id));
      setSelectedId(null);
    } catch (e: any) {
      setError(e.message);
    } finally {
      setDeleteLoading(false);
      setConfirmDelete(false);
    }
  }

  async function openFolder() {
    if (!selected) return;
    setFolderLoading(true);
    try {
      await api.sites.openFolder(selected.id);
    } catch (e: any) {
      setError(e.message);
    } finally {
      setFolderLoading(false);
    }
  }

  async function changePhp(version: string) {
    if (!selected) return;
    setChangingPhp(true);
    try {
      const updated = await api.sites.update(selected.id, { php_version: version });
      setSites((prev) => prev.map((s) => (s.id === updated.id ? updated : s)));
      await api.sites.refreshConfig(updated.id);
    } catch (e: any) {
      setError(e.message);
    } finally {
      setChangingPhp(false);
    }
  }

  async function pickAndAddDir() {
    try {
      const selected = await open({ directory: true, multiple: false, title: "Select projects folder" });
      if (!selected || !config) return;
      const dir = typeof selected === "string" ? selected : selected[0];
      if (!dir) return;
      const already = (config.scanned_dirs ?? []).includes(dir);
      if (already) return;
      const updated = await api.config.update({
        scanned_dirs: [...(config.scanned_dirs ?? []), dir],
      });
      setConfig(updated);
    } catch (e: any) {
      setError(e.message);
    }
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
    <div style={{ display: "flex", height: "100%", overflow: "hidden" }}>

      {/* ── Left: sites list ─────────────────────────────────────── */}
      <div style={{
        width: "260px", minWidth: "260px",
        borderRight: "1px solid var(--border)",
        display: "flex", flexDirection: "column",
        height: "100%",
      }}>
        <div style={{
          padding: "14px 16px",
          borderBottom: "1px solid var(--border)",
          display: "flex", alignItems: "center", justifyContent: "space-between",
        }}>
          <span style={{ fontWeight: 600, fontSize: "14px" }}>Sites</span>
          <button className="btn-ghost" onClick={scan} disabled={scanning}
            style={{ fontSize: "12px", padding: "3px 8px" }}>
            {scanning ? "Scanning…" : "Scan"}
          </button>
        </div>

        {error && (
          <div className={styles.error} style={{ margin: "8px", fontSize: "12px" }}>{error}</div>
        )}

        <div style={{ flex: 1, overflowY: "auto" }}>
          {sites.length === 0 ? (
            <div style={{ padding: "16px", fontSize: "13px", color: "var(--text-muted)" }}>
              No sites yet. Add a directory below and scan.
            </div>
          ) : (
            sites.map((site) => (
              <button key={site.id} onClick={() => selectSite(site.id)} style={{
                width: "100%", textAlign: "left",
                padding: "10px 16px",
                background: selectedId === site.id ? "var(--surface)" : "transparent",
                border: "none",
                borderLeft: selectedId === site.id ? "2px solid #6366f1" : "2px solid transparent",
                cursor: "pointer",
                display: "flex", alignItems: "center", justifyContent: "space-between",
                fontSize: "13px", color: "var(--text)",
              }}>
                <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {site.domain}
                </span>
                {site.ssl_enabled && (
                  <span style={{ color: "var(--text-muted)", fontSize: "11px", marginLeft: "6px" }}>🔒</span>
                )}
              </button>
            ))
          )}
        </div>

        {/* Scanned directories */}
        <div style={{ borderTop: "1px solid var(--border)", padding: "12px" }}>
          <div style={{
            fontSize: "11px", fontWeight: 600, color: "var(--text-muted)",
            textTransform: "uppercase", letterSpacing: "0.5px", marginBottom: "8px",
          }}>
            Scanned Dirs
          </div>
          {config?.scanned_dirs?.map((path: string) => (
            <div key={path} style={{ display: "flex", alignItems: "center", gap: "4px", marginBottom: "4px" }}>
              <code style={{
                fontSize: "11px", flex: 1,
                overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                color: "var(--text-muted)",
              }}>
                {path}
              </code>
              <button onClick={() => removeScannedDir(path)} style={{
                background: "none", border: "none", cursor: "pointer",
                color: "var(--text-muted)", padding: "0 2px", fontSize: "11px",
              }}>✕</button>
            </div>
          ))}
          <div style={{ display: "flex", gap: "4px", marginTop: "6px" }}>
            <input
              className={styles.pathInput}
              style={{ flex: 1, fontSize: "11px", padding: "4px 8px" }}
              placeholder="C:\Projects"
              value={newDir}
              onChange={(e) => setNewDir(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && addScannedDir()}
            />
            <button className="btn-primary" onClick={pickAndAddDir}
              title="Browse for folder"
              style={{ fontSize: "13px", padding: "4px 10px" }}>
              📁
            </button>
          </div>
        </div>
      </div>

      {/* ── Right: detail panel ──────────────────────────────────── */}
      <div style={{ flex: 1, overflowY: "auto", padding: "28px 32px" }}>
        {!selected ? (
          <div className={styles.empty} style={{ marginTop: "80px" }}>
            Select a site from the list to view details.
          </div>
        ) : (
          <>
            {/* Header */}
            <div style={{
              display: "flex", alignItems: "flex-start",
              justifyContent: "space-between", marginBottom: "24px",
            }}>
              <div>
                <h1 style={{ margin: 0, fontSize: "22px", fontWeight: 600 }}>
                  {selected.name || selected.domain}
                </h1>
                <a href={siteUrl} target="_blank" rel="noreferrer"
                  style={{ fontSize: "13px", color: "var(--text-muted)", textDecoration: "none" }}>
                  {siteUrl}
                </a>
              </div>
              <div style={{ display: "flex", gap: "6px", alignItems: "center" }}>
                {selected.ssl_enabled && (
                  <span style={{
                    fontSize: "12px", padding: "3px 8px",
                    background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "4px",
                  }}>🔒 HTTPS</span>
                )}
                <span className="badge badge-gray">
                  {TYPE_LABELS[selected.project_type] ?? selected.project_type}
                </span>
              </div>
            </div>

            {/* Tabs */}
            <div style={{ display: "flex", borderBottom: "1px solid var(--border)", marginBottom: "24px" }}>
              {(["general", "info"] as Tab[]).map((t) => (
                <button key={t} onClick={() => setTab(t)} style={{
                  background: "none", border: "none",
                  borderBottom: tab === t ? "2px solid #6366f1" : "2px solid transparent",
                  padding: "8px 18px", marginBottom: "-1px",
                  fontSize: "13px",
                  fontWeight: tab === t ? 600 : 400,
                  color: tab === t ? "#6366f1" : "var(--text-muted)",
                  cursor: "pointer",
                }}>
                  {t === "info" ? "Information" : "General"}
                </button>
              ))}
            </div>

            {/* ── General tab ──────────────────────────────────────── */}
            {tab === "general" && (
              <div>
                <div style={{
                  display: "grid", gridTemplateColumns: "130px 1fr",
                  rowGap: "16px", marginBottom: "28px",
                }}>
                  <span style={{ fontSize: "13px", color: "var(--text-muted)", alignSelf: "center" }}>Path</span>
                  <code style={{ fontSize: "12px", wordBreak: "break-all" }}>{selected.path}</code>

                  <span style={{ fontSize: "13px", color: "var(--text-muted)", alignSelf: "center" }}>URL</span>
                  <a href={siteUrl} target="_blank" rel="noreferrer"
                    style={{ fontSize: "13px", color: "#6366f1", textDecoration: "none" }}>
                    {siteUrl} ↗
                  </a>

                  <span style={{ fontSize: "13px", color: "var(--text-muted)", alignSelf: "center" }}>PHP Version</span>
                  <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                    <select
                      value={selected.php_version}
                      onChange={(e) => changePhp(e.target.value)}
                      disabled={changingPhp || phpVersions.length === 0}
                      style={{
                        fontSize: "13px", padding: "5px 10px", borderRadius: "6px",
                        border: "1px solid var(--border)",
                        background: "var(--bg)", color: "var(--text)",
                      }}
                    >
                      {phpVersions.length === 0 && (
                        <option value={selected.php_version}>{selected.php_version || "default"}</option>
                      )}
                      {phpVersions.map((v) => (
                        <option key={v.major} value={v.major}>{v.version}</option>
                      ))}
                    </select>
                    {changingPhp && (
                      <span style={{ fontSize: "12px", color: "var(--text-muted)" }}>Updating…</span>
                    )}
                  </div>
                </div>

                <div style={{ display: "flex", gap: "8px", flexWrap: "wrap", alignItems: "center" }}>
                  <button className="btn-primary" onClick={() => window.open(siteUrl, "_blank")}>
                    Open ↗
                  </button>

                  <button className="btn-ghost" onClick={openFolder} disabled={folderLoading}
                    style={{ minWidth: "100px" }}>
                    {folderLoading ? <Spinner /> : "Open folder"}
                  </button>

                  <button className="btn-ghost" onClick={toggleSSL} disabled={sslLoading}
                    style={{ minWidth: "120px" }}>
                    {sslLoading
                      ? <Spinner />
                      : selected.ssl_enabled ? "Disable HTTPS" : "Enable HTTPS"}
                  </button>

                  <button className="btn-ghost" onClick={refreshConfig} disabled={refreshLoading}
                    style={{ minWidth: "120px" }}>
                    {refreshLoading ? <Spinner /> : "↺ Refresh config"}
                  </button>

                  {!confirmDelete ? (
                    <button className="btn-danger" onClick={() => setConfirmDelete(true)}>
                      Remove
                    </button>
                  ) : (
                    <div style={{ display: "flex", gap: "6px", alignItems: "center" }}>
                      <span style={{ fontSize: "12px", color: "var(--text-muted)" }}>Remove {selected.domain}?</span>
                      <button className="btn-danger" onClick={deleteSite} disabled={deleteLoading}
                        style={{ minWidth: "64px" }}>
                        {deleteLoading ? <Spinner /> : "Yes"}
                      </button>
                      <button className="btn-ghost" onClick={() => setConfirmDelete(false)} disabled={deleteLoading}>
                        Cancel
                      </button>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* ── Information tab ──────────────────────────────────── */}
            {tab === "info" && (
              <div>
                {infoLoading ? (
                  <div className={styles.empty}>Loading…</div>
                ) : !siteInfo || (!siteInfo.app_name && !siteInfo.framework_name) ? (
                  <div className={styles.empty}>
                    No application info available for this project.
                  </div>
                ) : (
                  <div style={{ display: "grid", gridTemplateColumns: "160px 1fr", rowGap: "16px" }}>
                    {siteInfo.app_name && (
                      <InfoRow label="Application Name" value={siteInfo.app_name} />
                    )}
                    {siteInfo.framework_name && (
                      <InfoRow
                        label={`${siteInfo.framework_name} Version`}
                        value={siteInfo.framework_version}
                      />
                    )}
                    {siteInfo.app_env && (
                      <InfoRow label="Environment" value={siteInfo.app_env} />
                    )}
                    {siteInfo.app_env && (
                      <InfoRow
                        label="Debug Mode"
                        value={siteInfo.app_debug ? "✓" : "✗"}
                        color={siteInfo.app_debug ? "#22c55e" : "#ef4444"}
                      />
                    )}
                    {siteInfo.app_url && (
                      <InfoRow label="App URL" value={siteInfo.app_url} />
                    )}
                    <InfoRow
                      label="Maintenance Mode"
                      value={siteInfo.maintenance_mode ? "✓" : "✗"}
                      color={siteInfo.maintenance_mode ? "#ef4444" : "#22c55e"}
                    />
                    {siteInfo.app_timezone && (
                      <InfoRow label="Timezone" value={siteInfo.app_timezone} />
                    )}
                    {siteInfo.app_locale && (
                      <InfoRow label="Locale" value={siteInfo.app_locale} />
                    )}
                  </div>
                )}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function Spinner() {
  return (
    <span style={{
      display: "inline-block",
      width: "12px", height: "12px",
      border: "2px solid currentColor",
      borderTopColor: "transparent",
      borderRadius: "50%",
      animation: "spin 0.6s linear infinite",
    }} />
  );
}

function InfoRow({ label, value, color }: { label: string; value: string; color?: string }) {
  return (
    <>
      <span style={{ fontSize: "13px", color: "var(--text-muted)", alignSelf: "center" }}>{label}</span>
      <span style={{ fontSize: "13px", color: color ?? "var(--text)" }}>{value}</span>
    </>
  );
}
