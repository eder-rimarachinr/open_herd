import { useEffect, useRef, useState } from "react";
import { api, AsyncTask, DBInstance, DBType } from "../api/client";
import styles from "./Page.module.css";

const DB_LABELS: Record<DBType, string> = {
  mysql: "MySQL",
  mariadb: "MariaDB",
  postgres: "PostgreSQL",
};

const DB_PORTS: Record<DBType, number> = {
  mysql: 3306,
  mariadb: 3306,
  postgres: 5432,
};

const emptyForm = {
  name: "",
  type: "mysql" as DBType,
  host: "127.0.0.1",
  port: 3306,
  user: "root",
  password: "",
  managed: false,
  service_name: "",
};

const emptyInstallForm = { name: "MariaDB Local", port: 3306 };

export default function Database() {
  const [instances, setInstances] = useState<DBInstance[]>([]);
  const [loading, setLoading] = useState(true);
  const [detecting, setDetecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [actionLoading, setActionLoading] = useState<Record<string, boolean>>({});
  const [showForm, setShowForm] = useState(false);
  const [showInstall, setShowInstall] = useState(false);
  const [form, setForm] = useState(emptyForm);
  const [installForm, setInstallForm] = useState(emptyInstallForm);
  const [submitting, setSubmitting] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [installTask, setInstallTask] = useState<AsyncTask | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    api.databases.list()
      .then(setInstances)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false));
    return () => { if (pollRef.current) clearInterval(pollRef.current); };
  }, []);

  async function detect() {
    setDetecting(true);
    setError(null);
    try {
      setInstances(await api.databases.detect());
    } catch (e: any) {
      setError(e.message);
    } finally {
      setDetecting(false);
    }
  }

  async function startDB(inst: DBInstance) {
    setActionLoading((p) => ({ ...p, [inst.id]: true }));
    setError(null);
    try {
      await api.databases.start(inst.id);
      setInstances((prev) => prev.map((i) => (i.id === inst.id ? { ...i, running: true } : i)));
    } catch (e: any) {
      setError(e.message);
    } finally {
      setActionLoading((p) => ({ ...p, [inst.id]: false }));
    }
  }

  async function stopDB(inst: DBInstance) {
    setActionLoading((p) => ({ ...p, [inst.id]: true }));
    setError(null);
    try {
      await api.databases.stop(inst.id);
      setInstances((prev) => prev.map((i) => (i.id === inst.id ? { ...i, running: false } : i)));
    } catch (e: any) {
      setError(e.message);
    } finally {
      setActionLoading((p) => ({ ...p, [inst.id]: false }));
    }
  }

  async function removeDB(id: string) {
    if (!confirm("Remove this database connection?")) return;
    await api.databases.delete(id);
    setInstances((prev) => prev.filter((i) => i.id !== id));
  }

  async function submitForm(e: React.FormEvent) {
    e.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      const added = await api.databases.add({
        ...form,
        service_name: form.service_name || undefined,
        password: form.password || undefined,
      });
      setInstances((prev) => [...prev, added]);
      setShowForm(false);
      setForm(emptyForm);
    } catch (e: any) {
      setError(e.message);
    } finally {
      setSubmitting(false);
    }
  }

  async function startInstall(e: React.FormEvent) {
    e.preventDefault();
    setInstalling(true);
    setError(null);
    try {
      const { id } = await api.databases.install({
        name: installForm.name,
        type: "mariadb",
        port: installForm.port,
      });
      // Poll progress every 1.5s until done/error.
      pollRef.current = setInterval(async () => {
        try {
          const task = await api.databases.installProgress(id);
          setInstallTask(task);
          if (task.state === "done" || task.state === "error") {
            clearInterval(pollRef.current!);
            setInstalling(false);
            if (task.state === "done") {
              setInstances(await api.databases.list());
              setShowInstall(false);
              setInstallForm(emptyInstallForm);
            }
          }
        } catch {
          // keep polling
        }
      }, 1500);
    } catch (e: any) {
      setError(e.message);
      setInstalling(false);
    }
  }

  function onTypeChange(type: DBType) {
    setForm((f) => ({ ...f, type, port: DB_PORTS[type] }));
  }

  if (loading) return <div className={styles.empty}>Loading…</div>;

  return (
    <div>
      <div className={styles.header}>
        <h1 className={styles.title}>Database</h1>
        <div style={{ display: "flex", gap: "8px" }}>
          <button className="btn-ghost" onClick={detect} disabled={detecting}>
            {detecting ? "Detecting…" : "Detect installed"}
          </button>
          <button className="btn-ghost" onClick={() => { setShowInstall((v) => !v); setShowForm(false); }}>
            {showInstall ? "Cancel" : "Install MariaDB"}
          </button>
          <button className="btn-primary" onClick={() => { setShowForm((v) => !v); setShowInstall(false); }}>
            {showForm ? "Cancel" : "+ Add connection"}
          </button>
        </div>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      {/* ── Install local MariaDB ─────────────────────────────────────── */}
      {showInstall && (
        <form onSubmit={startInstall} style={{ marginBottom: "24px", background: "var(--surface)", borderRadius: "8px", padding: "20px" }}>
          <h3 style={{ marginBottom: "4px" }}>Install MariaDB 11.4 LTS locally</h3>
          <p style={{ fontSize: "13px", color: "var(--text-muted)", marginBottom: "16px" }}>
            phpenv will download the portable MariaDB ZIP (~200 MB), extract it to <code>~/.phpenv/databases/</code>, and initialize a fresh database with an empty root password.
          </p>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "12px", marginBottom: "16px" }}>
            <label>
              <span style={{ display: "block", marginBottom: "4px", fontSize: "13px" }}>Name</span>
              <input className={styles.pathInput} value={installForm.name}
                onChange={(e) => setInstallForm((f) => ({ ...f, name: e.target.value }))} required />
            </label>
            <label>
              <span style={{ display: "block", marginBottom: "4px", fontSize: "13px" }}>Port</span>
              <input className={styles.pathInput} type="number" value={installForm.port}
                onChange={(e) => setInstallForm((f) => ({ ...f, port: +e.target.value }))} required />
            </label>
          </div>

          {installTask && (
            <div style={{ marginBottom: "16px", padding: "12px", background: "var(--bg)", borderRadius: "6px" }}>
              <div style={{ fontSize: "13px", marginBottom: "8px", display: "flex", alignItems: "center", gap: "8px" }}>
                {installTask.state === "error"
                  ? <span style={{ color: "#ef4444" }}>✗ {installTask.error || installTask.message}</span>
                  : installTask.state === "done"
                    ? <span style={{ color: "#22c55e" }}>✓ {installTask.message}</span>
                    : <span>{installTask.message}</span>
                }
              </div>
              {(installTask.state === "pending" || installTask.state === "running") && (
                <div style={{ height: "4px", background: "var(--border)", borderRadius: "2px", overflow: "hidden" }}>
                  <div style={{ height: "100%", background: "#6366f1", width: "100%",
                    animation: "indeterminate 1.5s ease-in-out infinite",
                    transformOrigin: "left" }} />
                </div>
              )}
            </div>
          )}

          <div style={{ display: "flex", gap: "8px" }}>
            <button className="btn-primary" type="submit" disabled={installing}>
              {installing ? "Installing…" : "Install"}
            </button>
            <button className="btn-ghost" type="button"
              onClick={() => { setShowInstall(false); setInstallTask(null); setInstallForm(emptyInstallForm); }}>
              Cancel
            </button>
          </div>
        </form>
      )}

      {/* ── Add remote/service connection ────────────────────────────── */}
      {showForm && (
        <form onSubmit={submitForm} style={{ marginBottom: "24px", background: "var(--surface)", borderRadius: "8px", padding: "20px" }}>
          <h3 style={{ marginBottom: "16px" }}>Add database connection</h3>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "12px" }}>
            <label>
              <span style={{ display: "block", marginBottom: "4px", fontSize: "13px" }}>Name *</span>
              <input className={styles.pathInput} value={form.name}
                onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))} placeholder="My MySQL" required />
            </label>
            <label>
              <span style={{ display: "block", marginBottom: "4px", fontSize: "13px" }}>Type *</span>
              <select className={styles.pathInput} value={form.type}
                onChange={(e) => onTypeChange(e.target.value as DBType)}>
                <option value="mysql">MySQL</option>
                <option value="mariadb">MariaDB</option>
                <option value="postgres">PostgreSQL</option>
              </select>
            </label>
            <label>
              <span style={{ display: "block", marginBottom: "4px", fontSize: "13px" }}>Host *</span>
              <input className={styles.pathInput} value={form.host}
                onChange={(e) => setForm((f) => ({ ...f, host: e.target.value }))} required />
            </label>
            <label>
              <span style={{ display: "block", marginBottom: "4px", fontSize: "13px" }}>Port *</span>
              <input className={styles.pathInput} type="number" value={form.port}
                onChange={(e) => setForm((f) => ({ ...f, port: +e.target.value }))} required />
            </label>
            <label>
              <span style={{ display: "block", marginBottom: "4px", fontSize: "13px" }}>User</span>
              <input className={styles.pathInput} value={form.user}
                onChange={(e) => setForm((f) => ({ ...f, user: e.target.value }))} />
            </label>
            <label>
              <span style={{ display: "block", marginBottom: "4px", fontSize: "13px" }}>Password</span>
              <input className={styles.pathInput} type="password" value={form.password}
                onChange={(e) => setForm((f) => ({ ...f, password: e.target.value }))} />
            </label>
          </div>

          <div style={{ marginTop: "12px", display: "flex", alignItems: "center", gap: "8px" }}>
            <input id="managed" type="checkbox" checked={form.managed}
              onChange={(e) => setForm((f) => ({ ...f, managed: e.target.checked }))} />
            <label htmlFor="managed" style={{ fontSize: "13px" }}>
              Managed — phpenv can start/stop this instance
            </label>
          </div>

          {form.managed && (
            <div style={{ marginTop: "12px" }}>
              <span style={{ display: "block", marginBottom: "4px", fontSize: "13px" }}>
                Service name <span style={{ color: "var(--text-muted)" }}>(Windows: MySQL80 / Linux: mysql)</span>
              </span>
              <input className={styles.pathInput} value={form.service_name}
                onChange={(e) => setForm((f) => ({ ...f, service_name: e.target.value }))}
                placeholder="MySQL80 / mysql / postgresql-x64-16" />
            </div>
          )}

          <div style={{ marginTop: "16px", display: "flex", gap: "8px" }}>
            <button className="btn-primary" type="submit" disabled={submitting}>
              {submitting ? "Saving…" : "Save"}
            </button>
            <button className="btn-ghost" type="button"
              onClick={() => { setShowForm(false); setForm(emptyForm); }}>
              Cancel
            </button>
          </div>
        </form>
      )}

      {/* ── Instances table ───────────────────────────────────────────── */}
      {instances.length === 0 ? (
        <div className={styles.empty}>
          No database connections yet. Click "Detect installed" to find local instances, "Install MariaDB" to install one, or "+ Add connection" to add manually.
        </div>
      ) : (
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Name</th>
              <th>Type</th>
              <th>Host</th>
              <th>Port</th>
              <th>User</th>
              <th>Status</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {instances.map((inst) => (
              <tr key={inst.id}>
                <td className={styles.bold}>{inst.name}</td>
                <td><span className="badge badge-blue">{DB_LABELS[inst.type]}</span></td>
                <td>{inst.host}</td>
                <td>{inst.port}</td>
                <td>{inst.user}</td>
                <td>
                  <span style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                    <span style={{
                      width: "8px", height: "8px", borderRadius: "50%",
                      background: inst.running ? "#22c55e" : "#6b7280",
                      display: "inline-block",
                    }} />
                    {inst.running ? "Running" : "Stopped"}
                  </span>
                </td>
                <td style={{ display: "flex", gap: "6px", alignItems: "center" }}>
                  {inst.managed && (inst.service_name || inst.binary_dir) && (
                    inst.running
                      ? <button className="btn-danger" onClick={() => stopDB(inst)} disabled={actionLoading[inst.id]}>
                          {actionLoading[inst.id] ? "…" : "Stop"}
                        </button>
                      : <button className="btn-primary" onClick={() => startDB(inst)} disabled={actionLoading[inst.id]}>
                          {actionLoading[inst.id] ? "…" : "Start"}
                        </button>
                  )}
                  {inst.managed && !inst.service_name && !inst.binary_dir && (
                    <span style={{ fontSize: "12px", color: "var(--text-muted)" }}>Install pending</span>
                  )}
                  <button className="btn-ghost" onClick={() => removeDB(inst.id)}>Remove</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
