const BASE = "http://127.0.0.1:7878/api/v1";

const TIMEOUT_DEFAULT  =  15_000; // ms — standard API calls
const TIMEOUT_SSL      =  90_000; // ms — first run generates mkcert CA + cert
const TIMEOUT_DOWNLOAD = 120_000; // ms — nginx download (~1.5 MB, nginx.org can be slow)

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
    list: () => request<Site[]>("/sites"),
    scan: () => request<Site[]>("/sites/scan", { method: "POST" }),
    bulk: (sites: Partial<Site>[]) =>
      request<Site[]>("/sites/bulk", { method: "POST", body: JSON.stringify(sites) }),
    get: (id: string) => request<Site>(`/sites/${id}`),
    create: (body: Partial<Site>) =>
      request<Site>("/sites", { method: "POST", body: JSON.stringify(body) }),
    update: (id: string, body: Partial<Site>) =>
      request<Site>(`/sites/${id}`, { method: "PUT", body: JSON.stringify(body) }),
    delete: (id: string) => request<void>(`/sites/${id}`, { method: "DELETE" }),
    // Allow 90s — first run generates the mkcert CA key + domain cert.
    enableSSL: (id: string) => request<Site>(`/sites/${id}/ssl`, { method: "POST", timeoutMs: TIMEOUT_SSL }),
    disableSSL: (id: string) => request<Site>(`/sites/${id}/ssl`, { method: "DELETE" }),
    refreshConfig: (id: string) => request<Site>(`/sites/${id}/refresh-config`, { method: "POST" }),
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
    get: () => request<AppConfig>("/config"),
    update: (body: Partial<AppConfig>) =>
      request<AppConfig>("/config", { method: "PUT", body: JSON.stringify(body) }),
  },

  nginx: {
    status: () => request<{ running: boolean }>("/nginx/status"),
    info: () => request<NginxInfo>("/nginx/info"),
    // Allow 120s — download is ~1.5 MB but nginx.org can be slow.
    download: () => request<unknown>("/nginx/download", { method: "POST", timeoutMs: TIMEOUT_DOWNLOAD }),
    start: () => request<unknown>("/nginx/start", { method: "POST" }),
    stop: () => request<unknown>("/nginx/stop", { method: "POST" }),
    reload: () => request<unknown>("/nginx/reload", { method: "POST" }),
  },

  services: {
    status: () => request<ServiceStatus>("/services/status"),
    // Allow 120s — first start may download nginx (~15 MB).
    start: () => request<ServiceStatus>("/services/start", { method: "POST", timeoutMs: TIMEOUT_DOWNLOAD }),
    stop: () => request<ServiceStatus>("/services/stop", { method: "POST" }),
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
