const BASE = "http://127.0.0.1:7878/api/v1";

const TIMEOUT_DEFAULT = 15_000; // ms — standard API calls

// ── Cache ─────────────────────────────────────────────────────────────────────
// Module-level TTL cache for stable GET endpoints. Mutations call invalidate()
// so the next read always hits the network after any write operation.

const CACHE_TTL = 5 * 60 * 1000; // 5 min — safety net; mutations invalidate eagerly

interface CacheEntry<T> { data: T; expiresAt: number }
const _cache = new Map<string, CacheEntry<unknown>>();

async function cached<T>(key: string, fetcher: () => Promise<T>): Promise<T> {
  const entry = _cache.get(key) as CacheEntry<T> | undefined;
  if (entry && Date.now() < entry.expiresAt) return entry.data;
  const data = await fetcher();
  _cache.set(key, { data, expiresAt: Date.now() + CACHE_TTL });
  return data;
}

function invalidate(...keys: string[]) {
  keys.forEach(k => _cache.delete(k));
}

async function request<T>(path: string, init?: RequestInit & { timeoutMs?: number }): Promise<T> {
  const { timeoutMs, ...fetchInit } = init ?? {};
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs ?? TIMEOUT_DEFAULT);
  const res = await fetch(BASE + path, {
    headers: { "Content-Type": "application/json" },
    signal: controller.signal,
    ...fetchInit,
  }).finally(() => clearTimeout(timer)).catch((err: Error) => {
    if (err.name === "AbortError") {
      throw new Error(`Request timed out after ${(timeoutMs ?? 15_000) / 1000}s`);
    }
    throw err;
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(body.error ?? res.statusText);
  }
  return res.json() as Promise<T>;
}

// ── Types ─────────────────────────────────────────────────────────────────────

export interface DaemonStatus {
  status: string;
  version: string;
  uptime: string;
  os: string;
  php_versions: string[];
  nginx: { running: boolean };
}

export interface Site {
  id: string;
  name: string;
  domain: string;
  path: string;
  php_version: string;
  project_type: "laravel" | "wordpress" | "codeigniter4" | "codeigniter3" | "spa" | "static" | "generic";
  ssl_enabled: boolean;
  active: boolean;
  created_at: string;
  updated_at: string;
}

export interface PHPVersion {
  version: string;
  major: string;
  binary_path: string;
  fpm_binary: string;
  fastcgi_addr: string;
  installed: boolean;
  running: boolean;
}

export interface CatalogEntry {
  major: string;
  latest_patch: string;
  installed_patch?: string;
  installed: boolean;
  running: boolean;
  has_update: boolean;
  security_only: boolean;
  end_of_life: boolean;
}

export interface InstallProgress {
  major: string;
  state: "pending" | "downloading" | "extracting" | "configuring" | "done" | "error";
  message: string;
  percent: number;
  error?: string;
}

export interface AsyncTask {
  state: "pending" | "running" | "done" | "error";
  message: string;
  error?: string;
}

export interface NginxInfo {
  installed: boolean;
  running: boolean;
  version: string;
  binary_path: string;
  config_valid: boolean | null;
  config_error: string;
  error_log: string;
  downloadable: boolean; // false on Linux — must use system package manager
  os: string;
}

export type DBType = "mysql" | "mariadb" | "postgres";

export interface DBInstance {
  id: string;
  name: string;
  type: DBType;
  host: string;
  port: number;
  user: string;
  password?: string;
  managed: boolean;
  service_name?: string;
  created_at: string;
  running: boolean; // enriched by the list/get endpoints
}

export interface AppConfig {
  base_dir: string;
  nginx_dir: string;
  php_dir: string;
  certs_dir: string;
  logs_dir: string;
  api_addr: string;
  http_port: number;
  https_port: number;
  scanned_dirs: string[];
  default_php: string;
  custom_php_dirs: string[];
  os: string;
}

// ── API calls ─────────────────────────────────────────────────────────────────

export const api = {
  status: () => request<DaemonStatus>("/status"),

  sites: {
    // Cached — shared between Sites and SSL pages, invalidated by any write below.
    list: () => cached("/sites", () => request<Site[]>("/sites")),
    // scan returns the full updated list so we pre-populate the cache instead of invalidating.
    scan: () => request<Site[]>("/sites/scan", { method: "POST" }).then(data => {
      _cache.set("/sites", { data, expiresAt: Date.now() + CACHE_TTL });
      return data;
    }),
    bulk: (sites: Partial<Site>[]) =>
      request<Site[]>("/sites/bulk", { method: "POST", body: JSON.stringify(sites) })
        .then(r => { invalidate("/sites"); return r; }),
    get: (id: string) => request<Site>(`/sites/${id}`),
    create: (body: Partial<Site>) =>
      request<Site>("/sites", { method: "POST", body: JSON.stringify(body) })
        .then(r => { invalidate("/sites"); return r; }),
    update: (id: string, body: Partial<Site>) =>
      request<Site>(`/sites/${id}`, { method: "PUT", body: JSON.stringify(body) })
        .then(r => { invalidate("/sites"); return r; }),
    delete: (id: string) =>
      request<void>(`/sites/${id}`, { method: "DELETE" })
        .then(r => { invalidate("/sites"); return r; }),
    // SSL issuance is async: POST returns 202, then poll sslProgress until done.
    // Invalidate immediately so the list re-fetches fresh data once polling shows "done".
    enableSSL: (id: string) =>
      request<AsyncTask>(`/sites/${id}/ssl`, { method: "POST" })
        .then(r => { invalidate("/sites"); return r; }),
    sslProgress: (id: string) => request<AsyncTask>(`/sites/${id}/ssl/progress`),
    disableSSL: (id: string) =>
      request<Site>(`/sites/${id}/ssl`, { method: "DELETE" })
        .then(r => { invalidate("/sites"); return r; }),
    refreshConfig: (id: string) => request<Site>(`/sites/${id}/refresh-config`, { method: "POST" }),
    // Call after an async operation (SSL, etc.) completes to force a fresh list fetch.
    invalidate: () => invalidate("/sites"),
  },

  php: {
    versions: () => request<PHPVersion[]>("/php/versions"),
    catalog: () => request<CatalogEntry[]>("/php/catalog"),
    detect: () => request<CatalogEntry[]>("/php/detect", { method: "POST" }),
    install: (major: string) =>
      request<unknown>("/php/install", { method: "POST", body: JSON.stringify({ major }) }),
    installProgress: (major: string) =>
      request<InstallProgress>(`/php/install/${major}/progress`),
    start: (v: string) => request<unknown>(`/php/versions/${v}/start`, { method: "POST" }),
    stop: (v: string) => request<unknown>(`/php/versions/${v}/stop`, { method: "POST" }),
  },

  config: {
    get: () => cached("/config", () => request<AppConfig>("/config")),
    update: (body: Partial<AppConfig>) =>
      request<AppConfig>("/config", { method: "PUT", body: JSON.stringify(body) })
        .then(r => { invalidate("/config"); return r; }),
  },

  nginx: {
    status: () => request<{ running: boolean }>("/nginx/status"),
    info: () => request<NginxInfo>("/nginx/info"),
    // Nginx download is async: POST returns 202, then poll downloadProgress until done.
    download: () => request<AsyncTask>("/nginx/download", { method: "POST" }),
    downloadProgress: () => request<AsyncTask>("/nginx/download/progress"),
    start: () => request<unknown>("/nginx/start", { method: "POST" }),
    stop: () => request<unknown>("/nginx/stop", { method: "POST" }),
    reload: () => request<unknown>("/nginx/reload", { method: "POST" }),
  },

  services: {
    status: () => request<ServiceStatus>("/services/status"),
    start: () => request<ServiceStatus>("/services/start", { method: "POST" }),
    stop: () => request<ServiceStatus>("/services/stop", { method: "POST" }),
  },

  databases: {
    list: () => request<DBInstance[]>("/databases"),
    detect: () => request<DBInstance[]>("/databases/detect", { method: "POST" }),
    add: (body: Partial<DBInstance>) =>
      request<DBInstance>("/databases", { method: "POST", body: JSON.stringify(body) }),
    get: (id: string) => request<DBInstance>(`/databases/${id}`),
    delete: (id: string) => request<void>(`/databases/${id}`, { method: "DELETE" }),
    start: (id: string) => request<unknown>(`/databases/${id}/start`, { method: "POST" }),
    stop: (id: string) => request<unknown>(`/databases/${id}/stop`, { method: "POST" }),
  },

  daemon: {
    quit: () => request<unknown>("/daemon/quit", { method: "POST" }),
    logs: () => request<{ logs: string }>("/daemon/logs"),
  },
};

export interface ServiceStatus {
  nginx: boolean;
  php_versions: { major: string; version: string; running: boolean }[];
  all_running: boolean;
}
