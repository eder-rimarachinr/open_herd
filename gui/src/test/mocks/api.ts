import { vi } from "vitest";
import type { Site, PHPVersion, AppConfig } from "../../api/client";

export const mockSite = (overrides: Partial<Site> = {}): Site => ({
  id: "site-1",
  name: "myapp.test",
  domain: "myapp.test",
  path: "D:\\projects\\myapp",
  php_version: "8.2",
  project_type: "laravel",
  ssl_enabled: false,
  active: false,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  ...overrides,
});

export const mockPhpVersion = (overrides: Partial<PHPVersion> = {}): PHPVersion => ({
  version: "8.2.0",
  major: "8.2",
  binary_path: "C:\\php\\php.exe",
  fpm_binary: "C:\\php\\php.exe",
  fastcgi_addr: "127.0.0.1:9082",
  installed: true,
  running: false,
  ...overrides,
});

export const mockConfig = (overrides: Partial<AppConfig> = {}): AppConfig => ({
  base_dir: "C:\\Users\\user\\.phpenv",
  nginx_dir: "C:\\Users\\user\\.phpenv\\nginx",
  php_dir: "C:\\Users\\user\\.phpenv\\php",
  certs_dir: "C:\\Users\\user\\.phpenv\\certs",
  logs_dir: "C:\\Users\\user\\.phpenv\\logs",
  api_addr: "127.0.0.1:7878",
  http_port: 80,
  https_port: 443,
  scanned_dirs: [],
  default_php: "8.2",
  custom_php_dirs: [],
  os: "windows",
  ...overrides,
});

export const createApiMock = (overrides: Record<string, unknown> = {}) => ({
  sites: {
    list: vi.fn().mockResolvedValue([mockSite()]),
    scan: vi.fn().mockResolvedValue([mockSite()]),
    get: vi.fn().mockResolvedValue(mockSite()),
    create: vi.fn().mockResolvedValue(mockSite()),
    update: vi.fn().mockImplementation((_id, body) =>
      Promise.resolve(mockSite(body as Partial<Site>))
    ),
    delete: vi.fn().mockResolvedValue(undefined),
    enableSSL: vi.fn().mockResolvedValue({ state: "done", message: "SSL enabled" }),
    disableSSL: vi.fn().mockResolvedValue(mockSite({ ssl_enabled: false })),
    sslProgress: vi.fn().mockResolvedValue({ state: "done", message: "SSL active" }),
    refreshConfig: vi.fn().mockResolvedValue(mockSite()),
    info: vi.fn().mockResolvedValue({
      app_name: "MyApp", app_env: "local", app_debug: true,
      app_url: "http://myapp.test", app_timezone: "UTC",
      app_locale: "en", framework_name: "Laravel",
      framework_version: "11.0", maintenance_mode: false,
    }),
    openFolder: vi.fn().mockResolvedValue({ ok: true }),
    invalidate: vi.fn(),
    bulk: vi.fn().mockResolvedValue([]),
  },
  php: {
    versions: vi.fn().mockResolvedValue([mockPhpVersion()]),
    catalog: vi.fn().mockResolvedValue([]),
    detect: vi.fn().mockResolvedValue([]),
    install: vi.fn().mockResolvedValue({}),
    installProgress: vi.fn().mockResolvedValue({ state: "done", percent: 100 }),
    start: vi.fn().mockResolvedValue({}),
    stop: vi.fn().mockResolvedValue({}),
  },
  config: {
    get: vi.fn().mockResolvedValue(mockConfig()),
    update: vi.fn().mockImplementation((body) =>
      Promise.resolve(mockConfig(body as Partial<AppConfig>))
    ),
  },
  nginx: {
    status: vi.fn().mockResolvedValue({ running: false }),
    info: vi.fn().mockResolvedValue({}),
    download: vi.fn().mockResolvedValue({}),
    downloadProgress: vi.fn().mockResolvedValue({ state: "done" }),
    start: vi.fn().mockResolvedValue({}),
    stop: vi.fn().mockResolvedValue({}),
    reload: vi.fn().mockResolvedValue({}),
  },
  services: {
    status: vi.fn().mockResolvedValue({ nginx: false, php_versions: [], all_running: false }),
    start: vi.fn().mockResolvedValue({}),
    stop: vi.fn().mockResolvedValue({}),
  },
  daemon: {
    quit: vi.fn().mockResolvedValue({}),
    logs: vi.fn().mockResolvedValue({ logs: "" }),
  },
  status: vi.fn().mockResolvedValue({ status: "ok" }),
  ...overrides,
});
