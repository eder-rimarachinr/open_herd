import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  RefreshCw, FolderOpen, Lock, Unlock, Trash2,
  ExternalLink, RotateCcw, FolderPlus, X, ShieldCheck,
  Plus, Globe,
} from "lucide-react";
import { api, Site, PHPVersion, SiteInfo } from "../api/client";
import { useToast } from "../context/ToastContext";
import s from "./Sites.module.css";

const sortSites = (arr: Site[]) => [...arr].sort((a, b) => a.domain.localeCompare(b.domain));

const TYPE_LABELS: Record<string, string> = {
  laravel:      "Laravel",
  wordpress:    "WordPress",
  codeigniter4: "CI4",
  codeigniter3: "CI3",
  spa:          "SPA",
  static:       "Static",
  generic:      "Generic",
};

// Avatar color per project type — rgba so they work in both themes
const TYPE_AVATAR: Record<string, { bg: string; color: string }> = {
  laravel:      { bg: "rgba(220,38,38,0.12)",   color: "#DC2626" },
  wordpress:    { bg: "rgba(37,99,235,0.12)",   color: "#3B82F6" },
  codeigniter4: { bg: "rgba(234,88,12,0.12)",   color: "#EA580C" },
  codeigniter3: { bg: "rgba(180,65,0,0.12)",    color: "#C2410C" },
  spa:          { bg: "rgba(124,58,237,0.12)",  color: "#8B5CF6" },
  static:       { bg: "rgba(100,116,139,0.12)", color: "#64748B" },
  generic:      { bg: "rgba(5,150,105,0.12)",   color: "#059669" },
};

type Tab    = "general" | "info";
type View   = "detail" | "new";

export default function Sites() {
  const { toast } = useToast();
  const [sites,          setSites]          = useState<Site[]>([]);
  const [loading,        setLoading]        = useState(true);
  const [scanning,       setScanning]       = useState(false);
  const [error,          setError]          = useState<string | null>(null);
  const [selectedId,     setSelectedId]     = useState<string | null>(null);
  const [view,           setView]           = useState<View>("detail");
  const [tab,            setTab]            = useState<Tab>("general");
  const [phpVersions,    setPhpVersions]    = useState<PHPVersion[]>([]);
  const [siteInfo,       setSiteInfo]       = useState<SiteInfo | null>(null);
  const [infoLoading,    setInfoLoading]    = useState(false);
  const [sslLoading,     setSslLoading]     = useState(false);
  const [refreshLoading, setRefreshLoading] = useState(false);
  const [folderLoading,  setFolderLoading]  = useState(false);
  const [changingPhp,    setChangingPhp]    = useState(false);
  const [deleteLoading,  setDeleteLoading]  = useState(false);
  const [confirmDelete,  setConfirmDelete]  = useState(false);
  const [config,         setConfig]         = useState<any>(null);
  const [newDir,         setNewDir]         = useState("");
  const [search,         setSearch]         = useState("");

  // New site form
  const [newDomain,     setNewDomain]     = useState("");
  const [newPath,       setNewPath]       = useState("");
  const [newPhp,        setNewPhp]        = useState("");
  const [addingLoading, setAddingLoading] = useState(false);

  const selected = sites.find((s) => s.id === selectedId) ?? null;
  const siteUrl  = selected
    ? `http${selected.ssl_enabled ? "s" : ""}://${selected.domain}`
    : "";
  const filtered = sites.filter((s) =>
    s.domain.toLowerCase().includes(search.toLowerCase())
  );

  useEffect(() => {
    Promise.all([api.sites.list(), api.php.versions(), api.config.get()])
      .then(([s, php, cfg]) => {
        setSites(sortSites(s));
        setPhpVersions(php);
        setConfig(cfg);
        setNewPhp(cfg.default_php ?? "");
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
    setView("detail");
    setTab("general");
    setSiteInfo(null);
    setError(null);
    setConfirmDelete(false);
  }

  function showNewForm() {
    setSelectedId(null);
    setView("new");
    setError(null);
    setNewDomain("");
    setNewPath("");
    setNewPhp(config?.default_php ?? "");
  }

  async function scan() {
    setScanning(true); setError(null);
    try {
      const result = sortSites(await api.sites.scan());
      setSites(result);
      toast(`Found ${result.length} site(s)`);
    }
    catch (e: any) { setError(e.message); }
    finally { setScanning(false); }
  }

  async function handleAddSite() {
    let domain = newDomain.trim();
    const path = newPath.trim();
    if (!domain || !path) { setError("Domain and path are required."); return; }
    if (!domain.endsWith(".test")) domain = `${domain}.test`;
    setAddingLoading(true); setError(null);
    try {
      const site = await api.sites.create({ domain, path, php_version: newPhp || config?.default_php });
      setSites((prev) => sortSites([...prev, site]));
      selectSite(site.id);
      toast(`${site.domain} added`);
    }
    catch (e: any) { setError(e.message); }
    finally { setAddingLoading(false); }
  }

  async function toggleSSL() {
    if (!selected) return;
    setSslLoading(true); setError(null);
    try {
      if (selected.ssl_enabled) {
        const updated = await api.sites.disableSSL(selected.id);
        setSites((prev) => prev.map((s) => s.id === updated.id ? updated : s));
        toast("HTTPS disabled");
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
        toast("HTTPS enabled");
      }
    }
    catch (e: any) { setError(e.message); toast(e.message, "error"); }
    finally { setSslLoading(false); }
  }

  async function refreshConfig() {
    if (!selected) return;
    setRefreshLoading(true); setError(null);
    try {
      const updated = await api.sites.refreshConfig(selected.id);
      setSites((prev) => prev.map((s) => s.id === updated.id ? updated : s));
      toast("Config refreshed");
    }
    catch (e: any) { setError(e.message); toast(e.message, "error"); }
    finally { setRefreshLoading(false); }
  }

  async function deleteSite() {
    if (!selected) return;
    setDeleteLoading(true); setError(null);
    try {
      await api.sites.delete(selected.id);
      const domain = selected.domain;
      setSites((prev) => prev.filter((s) => s.id !== selected.id));
      setSelectedId(null);
      toast(`${domain} removed`);
    }
    catch (e: any) { setError(e.message); toast(e.message, "error"); }
    finally { setDeleteLoading(false); setConfirmDelete(false); }
  }

  async function openFolder() {
    if (!selected) return;
    setFolderLoading(true);
    try { await api.sites.openFolder(selected.id); }
    catch (e: any) { setError(e.message); }
    finally { setFolderLoading(false); }
  }

  async function changePhp(version: string) {
    if (!selected) return;
    setChangingPhp(true);
    try {
      const updated = await api.sites.update(selected.id, { php_version: version });
      setSites((prev) => prev.map((s) => s.id === updated.id ? updated : s));
      await api.sites.refreshConfig(updated.id);
      toast(`Switched to PHP ${version}`);
    }
    catch (e: any) { setError(e.message); }
    finally { setChangingPhp(false); }
  }

  async function pickPath(setter: (p: string) => void) {
    try {
      const picked = await open({ directory: true, multiple: false });
      if (!picked) return;
      setter(typeof picked === "string" ? picked : picked[0]);
    } catch {}
  }

  async function pickAndAddDir() {
    try {
      const picked = await open({ directory: true, multiple: false, title: "Select projects folder" });
      if (!picked || !config) return;
      const dir = typeof picked === "string" ? picked : picked[0];
      if (!dir || (config.scanned_dirs ?? []).includes(dir)) return;
      const updated = await api.config.update({ scanned_dirs: [...(config.scanned_dirs ?? []), dir] });
      setConfig(updated);
      toast("Directory added");
    } catch (e: any) { setError(e.message); }
  }

  async function addScannedDir() {
    const dir = newDir.trim();
    if (!dir || !config) return;
    try {
      const updated = await api.config.update({ scanned_dirs: [...(config.scanned_dirs ?? []), dir] });
      setConfig(updated);
      setNewDir("");
    } catch (e: any) { setError(e.message); }
  }

  async function removeScannedDir(path: string) {
    if (!config) return;
    try {
      const updated = await api.config.update({ scanned_dirs: config.scanned_dirs.filter((d: string) => d !== path) });
      setConfig(updated);
    } catch (e: any) { setError(e.message); }
  }

  if (loading) return <div className={s.empty} style={{ height: "100%" }}>Loading…</div>;

  return (
    <div className={s.root}>
      {/* ── Left: site list ──────────────────────────────────────────── */}
      <div className={s.listPanel}>
        <div className={s.listHeader}>
          <span className={s.listTitle}>
            Sites <span style={{ color: "var(--text-3)", fontWeight: 400 }}>({sites.length})</span>
          </span>
          <div style={{ display: "flex", gap: 5 }}>
            <button className={s.scanBtn} onClick={showNewForm} title="Add site">
              <Plus size={11} />
            </button>
            <button className={s.scanBtn} onClick={scan} disabled={scanning}>
              <RefreshCw size={11} style={{ animation: scanning ? "spin 0.6s linear infinite" : "none" }} />
              {scanning ? "…" : "Scan"}
            </button>
          </div>
        </div>

        <div className={s.listSearch}>
          <input
            className={s.searchInput}
            placeholder="Filter sites…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>

        <div className={s.listScroll}>
          {filtered.length === 0 ? (
            <div className={s.emptyList}>
              {sites.length === 0
                ? "No sites yet.\nClick + to add one or Scan a directory."
                : "No matches."}
            </div>
          ) : filtered.map((site) => {
            const av = TYPE_AVATAR[site.project_type] ?? TYPE_AVATAR.generic;
            return (
              <button
                key={site.id}
                className={[s.siteRow, selectedId === site.id && view === "detail" ? s.siteRowActive : ""].join(" ")}
                onClick={() => selectSite(site.id)}
              >
                <div
                  className={s.siteFavicon}
                  style={{ background: av.bg, color: av.color, borderColor: "transparent" }}
                >
                  {site.domain.slice(0, 2).toUpperCase()}
                </div>
                <div className={s.siteMeta}>
                  <span className={s.siteDomain}>{site.domain}</span>
                  <span className={s.siteType}>{TYPE_LABELS[site.project_type] ?? site.project_type}</span>
                </div>
                <div className={s.siteIcons}>
                  {site.ssl_enabled && <ShieldCheck size={12} className={s.siteSSL} />}
                </div>
              </button>
            );
          })}
        </div>

        {/* Scanned directories */}
        <div className={s.dirsSection}>
          <div className={s.dirsLabel}>Scanned Dirs</div>
          {(config?.scanned_dirs ?? []).map((path: string) => (
            <div key={path} className={s.dirRow}>
              <span className={s.dirPath}>{path}</span>
              <button className={s.dirRemove} onClick={() => removeScannedDir(path)}>
                <X size={10} />
              </button>
            </div>
          ))}
          <div className={s.dirAddRow}>
            <input
              className={s.dirInput}
              placeholder="C:\Projects"
              value={newDir}
              onChange={(e) => setNewDir(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && addScannedDir()}
            />
            <button className={s.dirBrowse} onClick={pickAndAddDir} title="Browse">
              <FolderPlus size={13} />
            </button>
          </div>
        </div>
      </div>

      {/* ── Right: detail / new-site panel ──────────────────────────── */}
      <div className={s.detailPanel}>
        {/* ── New site form ─────────────────────────────────────────── */}
        {view === "new" && (
          <div className="fade-in">
            <div className={s.detailHeader}>
              <div>
                <h1 className={s.detailTitle}>New site</h1>
                <span style={{ fontSize: 13, color: "var(--text-3)" }}>
                  Register a local project as a .test domain
                </span>
              </div>
              <button className={s.actionBtn} onClick={() => setView("detail")} style={{ padding: "5px 10px" }}>
                <X size={13} /> Cancel
              </button>
            </div>

            {error && <div className={s.error}>{error}</div>}

            <div className={s.propGrid} style={{ marginBottom: 20 }}>
              <PropRow label="Domain">
                <div style={{ display: "flex", alignItems: "center", gap: 6, width: "100%" }}>
                  <input
                    style={{ flex: 1, padding: "6px 10px", borderRadius: 5, border: "1px solid var(--border)", background: "var(--bg)", color: "var(--text)", fontSize: 13, outline: "none" }}
                    placeholder="myapp  →  myapp.test"
                    value={newDomain}
                    onChange={(e) => setNewDomain(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleAddSite()}
                    autoFocus
                  />
                  <span style={{ fontSize: 11, color: "var(--text-3)", whiteSpace: "nowrap" }}>
                    {newDomain && !newDomain.endsWith(".test") ? `→ ${newDomain}.test` : ""}
                  </span>
                </div>
              </PropRow>
              <PropRow label="Path">
                <div style={{ display: "flex", gap: 6, flex: 1 }}>
                  <input
                    style={{ flex: 1, padding: "6px 10px", borderRadius: 5, border: "1px solid var(--border)", background: "var(--bg)", color: "var(--text)", fontSize: 13, fontFamily: "monospace", outline: "none" }}
                    placeholder="C:\Projects\myapp"
                    value={newPath}
                    onChange={(e) => setNewPath(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleAddSite()}
                  />
                  <button className={s.actionBtn} style={{ padding: "5px 10px" }} onClick={() => pickPath(setNewPath)}>
                    <FolderOpen size={13} />
                  </button>
                </div>
              </PropRow>
              {phpVersions.length > 0 && (
                <PropRow label="PHP">
                  <select
                    className={s.phpSelect}
                    value={newPhp}
                    onChange={(e) => setNewPhp(e.target.value)}
                  >
                    {phpVersions.map((v) => (
                      <option key={v.major} value={v.major}>{v.version}</option>
                    ))}
                  </select>
                </PropRow>
              )}
            </div>

            <button
              className={[s.actionBtn, s.actionBtnPrimary].join(" ")}
              onClick={handleAddSite}
              disabled={addingLoading}
            >
              {addingLoading ? <span className="spinner" /> : <Globe size={13} />}
              {addingLoading ? "Adding…" : "Add site"}
            </button>
          </div>
        )}

        {/* ── Site detail ───────────────────────────────────────────── */}
        {view === "detail" && !selected && (
          <div className={s.empty}>
            <ShieldCheck size={32} style={{ color: "var(--border-2)", marginBottom: 8 }} />
            Select a site or <button
              onClick={showNewForm}
              style={{ background: "none", border: "none", color: "var(--accent)", cursor: "pointer", fontSize: "inherit", padding: 0, fontWeight: 600 }}
            >add a new one</button>
          </div>
        )}

        {view === "detail" && selected && (
          <div className="fade-in">
            {error && <div className={s.error}>{error}</div>}

            <div className={s.detailHeader}>
              <div>
                <h1 className={s.detailTitle}>{selected.name || selected.domain}</h1>
                <button
                  onClick={() => openUrl(siteUrl)}
                  style={{ background: "none", border: "none", cursor: "pointer", padding: 0, fontSize: 13, color: "var(--text-3)", fontFamily: "monospace", display: "flex", alignItems: "center", gap: 4 }}
                >
                  {siteUrl}
                  <ExternalLink size={11} />
                </button>
              </div>
              <div className={s.detailBadges}>
                {selected.ssl_enabled && (
                  <span className="badge badge-green"><ShieldCheck size={10} /> HTTPS</span>
                )}
                <span className="badge badge-gray">
                  {TYPE_LABELS[selected.project_type] ?? selected.project_type}
                </span>
                <span className="badge badge-accent">PHP {selected.php_version}</span>
              </div>
            </div>

            <div className={s.tabs}>
              {(["general", "info"] as Tab[]).map((t) => (
                <button
                  key={t}
                  className={[s.tab, tab === t ? s.tabActive : ""].join(" ")}
                  onClick={() => setTab(t)}
                >
                  {t === "general" ? "General" : "Information"}
                </button>
              ))}
            </div>

            {tab === "general" && (
              <>
                <div className={s.propGrid}>
                  <PropRow label="Path">
                    <code style={{ fontFamily: "monospace", fontSize: 12, wordBreak: "break-all" }}>{selected.path}</code>
                  </PropRow>
                  <PropRow label="URL">
                    <button
                      onClick={() => openUrl(siteUrl)}
                      style={{ background: "none", border: "none", color: "var(--accent)", cursor: "pointer", padding: 0, fontSize: 13, display: "flex", alignItems: "center", gap: 5 }}
                    >
                      {siteUrl} <ExternalLink size={11} />
                    </button>
                  </PropRow>
                  <PropRow label="PHP Version">
                    <select
                      className={s.phpSelect}
                      value={selected.php_version}
                      onChange={(e) => changePhp(e.target.value)}
                      disabled={changingPhp || phpVersions.length === 0}
                    >
                      {phpVersions.length === 0 && (
                        <option value={selected.php_version}>{selected.php_version || "default"}</option>
                      )}
                      {phpVersions.map((v) => (
                        <option key={v.major} value={v.major}>{v.version}</option>
                      ))}
                    </select>
                    {changingPhp && <span style={{ fontSize: 11, color: "var(--text-3)", marginLeft: 8 }}>Updating…</span>}
                  </PropRow>
                  <PropRow label="Type">
                    <span>{TYPE_LABELS[selected.project_type] ?? selected.project_type}</span>
                  </PropRow>
                </div>

                <div className={s.actionBar}>
                  <button className={[s.actionBtn, s.actionBtnPrimary].join(" ")} onClick={() => openUrl(siteUrl)}>
                    <ExternalLink size={13} /> Open site
                  </button>
                  <button className={s.actionBtn} onClick={openFolder} disabled={folderLoading}>
                    {folderLoading ? <span className="spinner" /> : <FolderOpen size={13} />}
                    Folder
                  </button>
                  <button className={s.actionBtn} onClick={toggleSSL} disabled={sslLoading}>
                    {sslLoading ? <span className="spinner" /> : selected.ssl_enabled ? <Unlock size={13} /> : <Lock size={13} />}
                    {selected.ssl_enabled ? "Disable HTTPS" : "Enable HTTPS"}
                  </button>
                  <button className={s.actionBtn} onClick={refreshConfig} disabled={refreshLoading}>
                    {refreshLoading ? <span className="spinner" /> : <RotateCcw size={13} />}
                    Refresh config
                  </button>
                  {!confirmDelete ? (
                    <button className={[s.actionBtn, s.actionBtnDanger].join(" ")} onClick={() => setConfirmDelete(true)}>
                      <Trash2 size={13} /> Remove
                    </button>
                  ) : (
                    <div className={s.deleteConfirm}>
                      <span className={s.deleteConfirmText}>Remove {selected.domain}?</span>
                      <button className={[s.actionBtn, s.actionBtnDanger].join(" ")} onClick={deleteSite} disabled={deleteLoading} style={{ padding: "4px 10px" }}>
                        {deleteLoading ? <span className="spinner" /> : "Yes"}
                      </button>
                      <button className={s.actionBtn} onClick={() => setConfirmDelete(false)} style={{ padding: "4px 10px" }}>
                        Cancel
                      </button>
                    </div>
                  )}
                </div>
              </>
            )}

            {tab === "info" && (
              infoLoading ? (
                <div style={{ color: "var(--text-3)", padding: "32px 0" }}>Loading…</div>
              ) : !siteInfo || (!siteInfo.app_name && !siteInfo.framework_name) ? (
                <div style={{ color: "var(--text-3)", fontSize: 13 }}>
                  No application metadata available for this project type.
                </div>
              ) : (
                <div className={s.infoGrid}>
                  {siteInfo.app_name      && <PropRow label="App Name"><span>{siteInfo.app_name}</span></PropRow>}
                  {siteInfo.framework_name && <PropRow label={siteInfo.framework_name}><span>{siteInfo.framework_version}</span></PropRow>}
                  {siteInfo.app_env       && <PropRow label="Environment"><span className="badge badge-gray">{siteInfo.app_env}</span></PropRow>}
                  {siteInfo.app_env       && (
                    <PropRow label="Debug Mode">
                      <span className={siteInfo.app_debug ? "badge badge-amber" : "badge badge-green"}>
                        {siteInfo.app_debug ? "Enabled" : "Disabled"}
                      </span>
                    </PropRow>
                  )}
                  {siteInfo.app_url && <PropRow label="App URL"><code style={{ fontFamily: "monospace", fontSize: 12 }}>{siteInfo.app_url}</code></PropRow>}
                  <PropRow label="Maintenance">
                    <span className={siteInfo.maintenance_mode ? "badge badge-red" : "badge badge-green"}>
                      {siteInfo.maintenance_mode ? "On" : "Off"}
                    </span>
                  </PropRow>
                  {siteInfo.app_timezone && <PropRow label="Timezone"><span>{siteInfo.app_timezone}</span></PropRow>}
                  {siteInfo.app_locale   && <PropRow label="Locale"><span>{siteInfo.app_locale}</span></PropRow>}
                </div>
              )
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function PropRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <>
      <div style={{
        padding: "11px 16px",
        borderBottom: "1px solid var(--border)",
        borderRight: "1px solid var(--border)",
        background: "var(--surface-2)",
        fontSize: 11, fontWeight: 700, textTransform: "uppercase" as const,
        letterSpacing: "0.05em", color: "var(--text-3)",
        display: "flex", alignItems: "center",
      }}>
        {label}
      </div>
      <div style={{
        padding: "11px 16px",
        borderBottom: "1px solid var(--border)",
        fontSize: 13, color: "var(--text)",
        display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" as const,
      }}>
        {children}
      </div>
    </>
  );
}
