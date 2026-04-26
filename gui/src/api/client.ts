const BASE = "http://127.0.0.1:7878/api/v1";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(BASE + path, {
    headers: { "Content-Type": "application/json" },
    ...init,
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
  project_type: "laravel" | "wordpress" | "generic";
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
    get: (id: string) => request<Site>(`/sites/${id}`),
    create: (body: Partial<Site>) =>
      request<Site>("/sites", { method: "POST", body: JSON.stringify(body) }),
    update: (id: string, body: Partial<Site>) =>
      request<Site>(`/sites/${id}`, { method: "PUT", body: JSON.stringify(body) }),
    delete: (id: string) => request<void>(`/sites/${id}`, { method: "DELETE" }),
    enableSSL: (id: string) => request<Site>(`/sites/${id}/ssl`, { method: "POST" }),
    disableSSL: (id: string) => request<Site>(`/sites/${id}/ssl`, { method: "DELETE" }),
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
    start: () => request<unknown>("/nginx/start", { method: "POST" }),
    stop: () => request<unknown>("/nginx/stop", { method: "POST" }),
    reload: () => request<unknown>("/nginx/reload", { method: "POST" }),
  },

  services: {
    status: () => request<ServiceStatus>("/services/status"),
    start: () => request<ServiceStatus>("/services/start", { method: "POST" }),
    stop: () => request<ServiceStatus>("/services/stop", { method: "POST" }),
  },

  daemon: {
    quit: () => request<unknown>("/daemon/quit", { method: "POST" }),
  },
};

export interface ServiceStatus {
  nginx: boolean;
  php_versions: { major: string; version: string; running: boolean }[];
  all_running: boolean;
}
