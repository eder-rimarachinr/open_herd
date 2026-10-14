import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  RefreshCw, FolderOpen, Lock, Unlock, Trash2,
  ExternalLink, RotateCcw, FolderPlus, X, ShieldCheck,
  Plus, Globe, Search, ChevronLeft, ChevronRight, FolderInput,
} from "lucide-react";
import { api, Site, PHPVersion, SiteInfo } from "../api/client";
import { useToast } from "../context/ToastContext";
import { TYPE_LABELS, avatarFor } from "../components/siteTypes";
import s from "./Sites.module.css";

const sortSites = (arr: Site[]) => [...arr].sort((a, b) => a.domain.localeCompare(b.domain));

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
  const [dirsOpen,       setDirsOpen]       = useState(true);

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
    catch (e: any) { setError(e.message); toast(e.message, "error"); }
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

  async function openFolderOf(id: string) {
    try { await api.sites.openFolder(id); }
    catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg); toast(msg, "error");
    }
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

  if (loading) return <div className={s.loading}>Loading…</div>;

  return (
    <div className={s.root}>
      {/* ── Left: site list ──────────────────────────────────────────── */}
      <div className={s.listPanel}>
        <div className={s.listHeader}>
          <span className={s.listTitle}>
            Sites <span className={s.countPill}>{sites.length}</span>
          </span>
          <div className={s.headActions}>
            <button className={s.headBtn} onClick={scan} disabled={scanning}>
              <RefreshCw size={13} style={{ animation: scanning ? "spin 0.6s linear infinite" : "none" }} />
              {scanning ? "…" : "Scan"}
            </button>
            <button className={[s.headBtn, s.headBtnSquare, s.headBtnAccent].join(" ")} onClick={showNewForm} title="Add site" aria-label="Add site">
              <Plus size={15} />
            </button>
          </div>
        </div>

        <label className={s.listSearch}>
          <Search size={15} className={s.searchIcon} />
          <input
            className={s.searchInput}
            placeholder="Filter sites…"
            aria-label="Filter sites"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </label>

        <div className={s.listScroll}>
          {filtered.length === 0 ? (
            <div className={s.emptyList}>
              {sites.length === 0
                ? "No sites yet.\nClick + to add one or Scan a directory."
                : "No matches."}
            </div>
          ) : filtered.map((site) => {
            const av = avatarFor(site.project_type);
            const active = selectedId === site.id && view === "detail";
            return (
              <div key={site.id} className={[s.siteCard, active ? s.siteCardActive : ""].join(" ")}>
                <button className={s.siteRow} onClick={() => selectSite(site.id)}>
                  <span className={s.avatar} style={{ background: av.bg }}>{av.short}</span>
                  <span className={s.siteMeta}>
                    <span className={s.siteDomain}>{site.domain}</span>
                    <span className={s.siteTags}>
                      <span className={s.siteType}>{TYPE_LABELS[site.project_type] ?? site.project_type}</span>
                      <span className={s.sitePhp}>PHP {site.php_version}</span>
                    </span>
                  </span>
                  {site.ssl_enabled && (
                    <span className={s.sslChip}>
                      <ShieldCheck size={13} role="img" aria-label="HTTPS enabled" />
                    </span>
                  )}
                </button>
                <span className={s.quick}>
                  <button
                    className={s.quickBtn}
                    onClick={() => openUrl(`http${site.ssl_enabled ? "s" : ""}://${site.domain}`)}
                    title="Open in browser" aria-label="Open in browser"
                  >
                    <ExternalLink size={14} />
                  </button>
                  <button
                    className={s.quickBtn}
                    onClick={() => openFolderOf(site.id)}
                    title="Open folder" aria-label="Open folder"
                  >
                    <FolderOpen size={14} />
                  </button>
                </span>
              </div>
            );
          })}
        </div>

        {(config?.scanned_dirs ?? []).length > 0 && (
          <div className={s.listFoot}>
            <FolderOpen size={13} />
            <span>Watching {config.scanned_dirs.length} {config.scanned_dirs.length === 1 ? "folder" : "folders"}</span>
          </div>
        )}
      </div>

      {/* ── Right: detail / new-site panel ──────────────────────────── */}
      <div className={s.detailPanel}>
        {/* Shown in every view: a failed Scan with no site selected used to be invisible. */}
        {error && <div className={s.error} role="alert">{error}</div>}

        {/* ── New site form ─────────────────────────────────────────── */}
        {view === "new" && (
          <div className={["fade-in", s.detail].join(" ")}>
            <div className={s.detailHeader}>
              <div>
                <h1 className={s.detailTitle}>New site</h1>
                <span className="page-sub">Register a local project as a .test domain</span>
              </div>
              <button className={s.actionBtn} onClick={() => setView("detail")}>
                <X size={14} /> Cancel
              </button>
            </div>

            <div className={s.propGrid}>
              <PropRow label="Domain">
                <div className={s.inputRow}>
                  <input
                    className={s.textInput}
                    placeholder="myapp  →  myapp.test"
                    value={newDomain}
                    onChange={(e) => setNewDomain(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleAddSite()}
                    autoFocus
                  />
                  <span className={s.inputHint}>
                    {newDomain && !newDomain.endsWith(".test") ? `→ ${newDomain}.test` : ""}
                  </span>
                </div>
              </PropRow>
              <PropRow label="Path">
                <div className={s.inputRow}>
                  <input
                    className={[s.textInput, s.mono].join(" ")}
                    placeholder="C:\Projects\myapp"
                    value={newPath}
                    onChange={(e) => setNewPath(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleAddSite()}
                  />
                  <button className={s.actionBtn} onClick={() => pickPath(setNewPath)} aria-label="Browse for the project folder">
                    <FolderOpen size={14} />
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

            <div className={s.actionBar}>
              <button
                className={[s.actionBtn, s.actionBtnPrimary].join(" ")}
                onClick={handleAddSite}
                disabled={addingLoading}
              >
                {addingLoading ? <span className="spinner" /> : <Globe size={14} />}
                {addingLoading ? "Adding…" : "Add site"}
              </button>
            </div>
          </div>
        )}

        {/* ── Nothing selected ──────────────────────────────────────── */}
        {view === "detail" && !selected && (
          <div className={s.hero}>
            <EmptyIllustration />
            <h2 className={s.heroTitle}>Select a site or add a new one</h2>
            <p className={s.heroText}>
              Each project gets a <code>.test</code> domain, its own PHP version and a trusted HTTPS certificate.
            </p>
            <button className={[s.actionBtn, s.actionBtnPrimary, s.heroCta].join(" ")} onClick={showNewForm}>
              <Plus size={15} /> Add new site
            </button>

            <div className={s.quickGrid}>
              <button className={s.quickCard} onClick={showNewForm}>
                <span className={s.quickIcon}><FolderInput size={16} /></span>
                <b>Import project</b>
                <span>Link an existing folder; its framework is detected.</span>
              </button>
              <button className={s.quickCard} onClick={scan} disabled={scanning}>
                <span className={s.quickIcon}><RefreshCw size={16} /></span>
                <b>Scan folders</b>
                <span>Register every project found in the folders below.</span>
              </button>
              <button className={s.quickCard} onClick={pickAndAddDir}>
                <span className={s.quickIcon}><FolderPlus size={16} /></span>
                <b>Watch a folder</b>
                <span>Add a folder where you keep your projects.</span>
              </button>
            </div>

            <details
              className={s.dirs}
              open={dirsOpen}
              onToggle={(e) => setDirsOpen((e.currentTarget as HTMLDetailsElement).open)}
            >
              <summary className={s.dirsSummary}>
                <FolderOpen size={15} className={s.dirsIcon} />
                Scanned directories
                <span className={s.dirsSub}>· Scan looks for projects here</span>
                <ChevronRight size={14} className={s.dirsChevron} />
              </summary>
              {(config?.scanned_dirs ?? []).map((path: string) => (
                <div key={path} className={s.dirRow}>
                  <FolderOpen size={14} className={s.dirIcon} />
                  <span className={s.dirPath}>{path}</span>
                  <button
                    className={s.dirRemove}
                    onClick={() => removeScannedDir(path)}
                    title="Remove directory"
                    aria-label={`Remove ${path}`}
                  >
                    <X size={12} />
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
                <button className={s.dirBrowse} onClick={pickAndAddDir} title="Browse" aria-label="Browse for a directory">
                  <FolderPlus size={14} />
                </button>
              </div>
            </details>
          </div>
        )}

        {/* ── Site detail ───────────────────────────────────────────── */}
        {view === "detail" && selected && (
          <div className={["fade-in", s.detail].join(" ")}>
            <button className={[s.actionBtn, s.backBtn].join(" ")} onClick={() => setSelectedId(null)}>
              <ChevronLeft size={14} /> All sites
            </button>

            <div className={s.detailHeader}>
              <span className={[s.avatar, s.avatarLg].join(" ")} style={{ background: avatarFor(selected.project_type).bg }}>
                {avatarFor(selected.project_type).short}
              </span>
              <div className={s.detailHeading}>
                <h1 className={s.detailTitle}>{selected.name || selected.domain}</h1>
                <button className={s.detailUrl} onClick={() => openUrl(siteUrl)}>
                  {siteUrl}
                  <ExternalLink size={12} />
                </button>
              </div>
              <div className={s.detailBadges}>
                {selected.ssl_enabled && (
                  <span className="badge badge-green"><ShieldCheck size={12} /> HTTPS</span>
                )}
                <span className="badge badge-gray">
                  {TYPE_LABELS[selected.project_type] ?? selected.project_type}
                </span>
                <span className="badge badge-gray">PHP {selected.php_version}</span>
              </div>
            </div>

            <div className={s.tabs} role="tablist">
              {(["general", "info"] as Tab[]).map((t) => (
                <button
                  key={t}
                  role="tab"
                  aria-selected={tab === t}
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
                    <code className={s.mono}>{selected.path}</code>
                  </PropRow>
                  <PropRow label="URL">
                    <button className={s.linkBtn} onClick={() => openUrl(siteUrl)}>
                      {siteUrl} <ExternalLink size={12} />
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
                    {changingPhp && <span className={s.inputHint}>Updating…</span>}
                  </PropRow>
                  <PropRow label="Type">
                    <span>{TYPE_LABELS[selected.project_type] ?? selected.project_type}</span>
                  </PropRow>
                </div>

                {selected.ssl_enabled && (
                  <div className={s.certCard}>
                    <span className={s.certIcon}><Lock size={16} /></span>
                    <div>
                      <b>Trusted certificate</b>
                      <span>Issued by the local mkcert authority; browsers show a padlock.</span>
                    </div>
                  </div>
                )}

                <div className={s.actionBar}>
                  <button className={[s.actionBtn, s.actionBtnPrimary].join(" ")} onClick={() => openUrl(siteUrl)}>
                    <ExternalLink size={14} /> Open site
                  </button>
                  <button className={s.actionBtn} onClick={openFolder} disabled={folderLoading} title="Open folder in the file manager">
                    {folderLoading ? <span className="spinner" /> : <FolderOpen size={14} />}
                    Folder
                  </button>
                  <button className={s.actionBtn} onClick={toggleSSL} disabled={sslLoading}>
                    {sslLoading ? <span className="spinner" /> : selected.ssl_enabled ? <Unlock size={14} /> : <Lock size={14} />}
                    {selected.ssl_enabled ? "Disable HTTPS" : "Enable HTTPS"}
                  </button>
                  <button className={s.actionBtn} onClick={refreshConfig} disabled={refreshLoading}>
                    {refreshLoading ? <span className="spinner" /> : <RotateCcw size={14} />}
                    Refresh config
                  </button>
                  {!confirmDelete ? (
                    <button className={[s.actionBtn, s.actionBtnDanger, s.pushRight].join(" ")} onClick={() => setConfirmDelete(true)}>
                      <Trash2 size={14} /> Remove
                    </button>
                  ) : (
                    <div className={[s.deleteConfirm, s.pushRight].join(" ")}>
                      <span className={s.deleteConfirmText}>Remove {selected.domain}?</span>
                      <button className={[s.actionBtn, s.actionBtnDanger, s.actionBtnSm].join(" ")} onClick={deleteSite} disabled={deleteLoading}>
                        {deleteLoading ? <span className="spinner" /> : "Yes"}
                      </button>
                      <button className={[s.actionBtn, s.actionBtnSm].join(" ")} onClick={() => setConfirmDelete(false)}>
                        Cancel
                      </button>
                    </div>
                  )}
                </div>
              </>
            )}

            {tab === "info" && (
              infoLoading ? (
                <div className={s.muted}>Loading…</div>
              ) : !siteInfo || (!siteInfo.app_name && !siteInfo.framework_name) ? (
                <div className={s.muted}>
                  No application metadata available for this project type.
                </div>
              ) : (
                <div className={s.propGrid}>
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
                  {siteInfo.app_url && <PropRow label="App URL"><code className={s.mono}>{siteInfo.app_url}</code></PropRow>}
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
    <div className={s.propRow}>
      <div className={s.propLabel}>{label}</div>
      <div className={s.propValue}>{children}</div>
    </div>
  );
}

/** Shield over a .test chip and a :443 chip — the three things a site gets. */
function EmptyIllustration() {
  return (
    <svg className={s.heroArt} width="190" height="150" viewBox="0 0 190 150" aria-hidden="true">
      <defs>
        <linearGradient id="oh-shield" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#34D399" stopOpacity=".6" />
          <stop offset="1" stopColor="#3B82F6" stopOpacity=".28" />
        </linearGradient>
        <linearGradient id="oh-glass" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#FFFFFF" stopOpacity=".24" />
          <stop offset="1" stopColor="#FFFFFF" stopOpacity=".03" />
        </linearGradient>
        <radialGradient id="oh-glow">
          <stop offset="0" stopColor="#10B981" stopOpacity=".3" />
          <stop offset="1" stopColor="#10B981" stopOpacity="0" />
        </radialGradient>
      </defs>
      <circle cx="95" cy="78" r="70" fill="url(#oh-glow)" />
      <circle cx="95" cy="78" r="56" fill="none" stroke="#10B981" strokeOpacity=".22" strokeDasharray="2 6" />
      <circle cx="95" cy="78" r="40" fill="none" stroke="#3B82F6" strokeOpacity=".2" />
      <rect x="18" y="40" width="44" height="22" rx="6" className={s.artChip} />
      <text x="40" y="55" textAnchor="middle" className={s.artText}>.test</text>
      <rect x="132" y="94" width="42" height="22" rx="6" className={s.artChip} />
      <text x="153" y="109" textAnchor="middle" className={s.artTextAccent}>:443</text>
      <path d="M95 26l34 13v24c0 21-14.5 37.5-34 43-19.5-5.5-34-22-34-43V39z" fill="url(#oh-shield)" />
      <path d="M95 26l34 13v24c0 21-14.5 37.5-34 43-19.5-5.5-34-22-34-43V39z" fill="url(#oh-glass)" stroke="#FFFFFF" strokeOpacity=".35" />
      <path d="M80 66l10 10 20-20" fill="none" stroke="#FFFFFF" strokeWidth="5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
