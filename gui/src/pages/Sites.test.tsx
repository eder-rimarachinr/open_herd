import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import Sites from "./Sites";
import { mockSite, mockPhpVersion, mockConfig } from "../test/mocks/api";

// ── Module mocks (hoisted by Vitest) ─────────────────────────────────────────

vi.mock("../api/client", () => ({
    api: {
        sites: {
            list: vi.fn(),
            scan: vi.fn(),
            get: vi.fn(),
            create: vi.fn(),
            update: vi.fn(),
            delete: vi.fn(),
            enableSSL: vi.fn(),
            disableSSL: vi.fn(),
            sslProgress: vi.fn(),
            refreshConfig: vi.fn(),
            info: vi.fn(),
            openFolder: vi.fn(),
            invalidate: vi.fn(),
            bulk: vi.fn(),
        },
        php: { versions: vi.fn(), catalog: vi.fn(), detect: vi.fn(), install: vi.fn(), installProgress: vi.fn(), start: vi.fn(), stop: vi.fn() },
        config: { get: vi.fn(), update: vi.fn() },
        nginx: { status: vi.fn(), info: vi.fn(), download: vi.fn(), downloadProgress: vi.fn(), start: vi.fn(), stop: vi.fn(), reload: vi.fn() },
        services: { status: vi.fn(), start: vi.fn(), stop: vi.fn() },
        daemon: { quit: vi.fn(), logs: vi.fn() },
        status: vi.fn(),
    },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
    open: vi.fn().mockResolvedValue("/mocked/path"),
}));

// Get reference to the mocked api after hoisting
import { api } from "../api/client";
const m = api as Record<string, any>;

// ── Helpers ───────────────────────────────────────────────────────────────────

const renderSites = () => render(<MemoryRouter><Sites /></MemoryRouter>);

const EMPTY_INFO = { app_name: "", framework_name: "", framework_version: "", app_env: "", app_debug: false, app_url: "", app_timezone: "", app_locale: "", maintenance_mode: false };

function setupDefaults(overrides: { sites?: any[]; phpVersions?: any[]; config?: any } = {}) {
    m.sites.list.mockResolvedValue(overrides.sites ?? [mockSite()]);
    m.php.versions.mockResolvedValue(overrides.phpVersions ?? [mockPhpVersion()]);
    m.config.get.mockResolvedValue(overrides.config ?? mockConfig());
}

async function renderAndSelect(site = mockSite()) {
    setupDefaults({ sites: [site] });
    renderSites();
    await waitFor(() => screen.getByText(site.domain));
    await userEvent.click(screen.getByText(site.domain));
    await waitFor(() => screen.getByText("Open folder"));
}

beforeEach(() => {
    vi.clearAllMocks();
    m.sites.info.mockResolvedValue(EMPTY_INFO);
    m.sites.refreshConfig.mockResolvedValue(mockSite());
    m.sites.delete.mockResolvedValue(undefined);
    m.sites.openFolder.mockResolvedValue({ ok: true });
    m.sites.enableSSL.mockResolvedValue({ state: "done", message: "SSL enabled" });
    m.sites.disableSSL.mockResolvedValue(mockSite({ ssl_enabled: false }));
    m.sites.sslProgress.mockResolvedValue({ state: "done", message: "done" });
    m.sites.update.mockImplementation((_id: string, body: any) => Promise.resolve(mockSite(body)));
    m.config.update.mockImplementation((body: any) => Promise.resolve(mockConfig(body)));
});

// ── Loading ───────────────────────────────────────────────────────────────────

describe("Loading", () => {
    it("shows loading state on mount", () => {
        m.sites.list.mockReturnValue(new Promise(() => { }));
        m.php.versions.mockReturnValue(new Promise(() => { }));
        m.config.get.mockReturnValue(new Promise(() => { }));
        renderSites();
        expect(document.body).toBeTruthy();
    });
});

// ── Site list ─────────────────────────────────────────────────────────────────

describe("Site list", () => {
    it("renders a site after loading", async () => {
        setupDefaults();
        renderSites();
        await waitFor(() => expect(screen.getByText("myapp.test")).toBeInTheDocument());
    });

    it("shows empty state when no sites", async () => {
        setupDefaults({ sites: [] });
        renderSites();
        await waitFor(() => expect(screen.getByText(/No sites yet/i)).toBeInTheDocument());
    });

    it("sorts sites alphabetically", async () => {
        setupDefaults({
            sites: [
                mockSite({ id: "2", domain: "zebra.test", name: "zebra.test" }),
                mockSite({ id: "1", domain: "alpha.test", name: "alpha.test" }),
            ],
        });
        renderSites();
        await waitFor(() => {
            const buttons = screen.getAllByRole("button", { name: /\.test/ });
            expect(buttons[0].textContent).toContain("alpha.test");
            expect(buttons[1].textContent).toContain("zebra.test");
        });
    });

    it("shows 🔒 for SSL-enabled sites", async () => {
        setupDefaults({ sites: [mockSite({ ssl_enabled: true })] });
        renderSites();
        await waitFor(() => expect(screen.getByText("🔒")).toBeInTheDocument());
    });
});

// ── Detail panel ──────────────────────────────────────────────────────────────

describe("Site detail panel", () => {
    it("shows placeholder when no site selected", async () => {
        setupDefaults();
        renderSites();
        await waitFor(() =>
            expect(screen.getByText(/Select a site from the list/i)).toBeInTheDocument()
        );
    });

    it("shows site details after selecting", async () => {
        await renderAndSelect();
        expect(screen.getByText("D:\\projects\\myapp")).toBeInTheDocument();
        expect(screen.getByText("http://myapp.test ↗")).toBeInTheDocument();
    });

    it("shows HTTPS badge for SSL sites", async () => {
        await renderAndSelect(mockSite({ ssl_enabled: true }));
        expect(screen.getByText("🔒 HTTPS")).toBeInTheDocument();
    });

    it("shows project type badge", async () => {
        await renderAndSelect(mockSite({ project_type: "laravel" }));
        expect(screen.getByText("Laravel")).toBeInTheDocument();
    });

    it("resets confirm delete when switching sites", async () => {
        setupDefaults({
            sites: [
                mockSite({ id: "1", domain: "alpha.test", name: "alpha.test" }),
                mockSite({ id: "2", domain: "beta.test", name: "beta.test" }),
            ],
        });
        renderSites();
        await waitFor(() => screen.getByText("alpha.test"));
        await userEvent.click(screen.getByText("alpha.test"));
        await waitFor(() => screen.getByText("Remove"));
        await userEvent.click(screen.getByText("Remove"));
        expect(screen.getByText("Yes")).toBeInTheDocument();

        await userEvent.click(screen.getByText("beta.test"));
        await waitFor(() => screen.getByText("Remove"));
        expect(screen.queryByText("Yes")).not.toBeInTheDocument();
    });
});

// ── Actions ───────────────────────────────────────────────────────────────────

describe("Actions", () => {
    it("calls openFolder when Open folder clicked", async () => {
        await renderAndSelect();
        await userEvent.click(screen.getByText("Open folder"));
        expect(m.sites.openFolder).toHaveBeenCalledWith("site-1");
    });

    it("calls refreshConfig when Refresh config clicked", async () => {
        await renderAndSelect();
        await userEvent.click(screen.getByText("↺ Refresh config"));
        await waitFor(() => expect(m.sites.refreshConfig).toHaveBeenCalledWith("site-1"));
    });

    it("shows Enable HTTPS when SSL disabled", async () => {
        await renderAndSelect(mockSite({ ssl_enabled: false }));
        expect(screen.getByText("Enable HTTPS")).toBeInTheDocument();
    });

    it("shows Disable HTTPS when SSL enabled", async () => {
        await renderAndSelect(mockSite({ ssl_enabled: true }));
        expect(screen.getByText("Disable HTTPS")).toBeInTheDocument();
    });

    it("calls enableSSL on Enable HTTPS click", async () => {
        await renderAndSelect(mockSite({ ssl_enabled: false }));
        await userEvent.click(screen.getByText("Enable HTTPS"));
        await waitFor(() => expect(m.sites.enableSSL).toHaveBeenCalledWith("site-1"));
    });

    it("calls disableSSL on Disable HTTPS click", async () => {
        await renderAndSelect(mockSite({ ssl_enabled: true }));
        await userEvent.click(screen.getByText("Disable HTTPS"));
        await waitFor(() => expect(m.sites.disableSSL).toHaveBeenCalledWith("site-1"));
    });

    it("shows inline confirm on Remove click", async () => {
        await renderAndSelect();
        await userEvent.click(screen.getByText("Remove"));
        expect(screen.getByText(/Remove myapp\.test\?/i)).toBeInTheDocument();
        expect(screen.getByText("Yes")).toBeInTheDocument();
        expect(screen.getByText("Cancel")).toBeInTheDocument();
    });

    it("does not delete on Cancel", async () => {
        await renderAndSelect();
        await userEvent.click(screen.getByText("Remove"));
        await userEvent.click(screen.getByText("Cancel"));
        expect(screen.queryByText("Yes")).not.toBeInTheDocument();
        expect(m.sites.delete).not.toHaveBeenCalled();
    });

    it("deletes site and removes from list on Yes", async () => {
        await renderAndSelect();
        await userEvent.click(screen.getByText("Remove"));
        await userEvent.click(screen.getByText("Yes"));
        await waitFor(() => expect(m.sites.delete).toHaveBeenCalledWith("site-1"));
        await waitFor(() => expect(screen.queryByText("myapp.test")).not.toBeInTheDocument());
    });

    it("changes PHP version via select dropdown", async () => {
        setupDefaults({
            phpVersions: [
                mockPhpVersion({ version: "8.2.0", major: "8.2" }),
                mockPhpVersion({ version: "8.1.0", major: "8.1" }),
            ],
        });
        renderSites();
        await waitFor(() => screen.getByText("myapp.test"));
        await userEvent.click(screen.getByText("myapp.test"));
        await waitFor(() => screen.getByRole("combobox"));

        await userEvent.selectOptions(screen.getByRole("combobox"), "8.1");
        await waitFor(() =>
            expect(m.sites.update).toHaveBeenCalledWith("site-1", { php_version: "8.1" })
        );
    });
});

// ── Scan ──────────────────────────────────────────────────────────────────────

describe("Scan", () => {
    it("calls scan API on Scan button click", async () => {
        setupDefaults({ sites: [] });
        m.sites.scan.mockResolvedValue([mockSite()]);
        renderSites();
        await waitFor(() => screen.getByText("Scan"));
        await userEvent.click(screen.getByText("Scan"));
        await waitFor(() => expect(m.sites.scan).toHaveBeenCalled());
    });

    it("shows found sites after scan", async () => {
        setupDefaults({ sites: [] });
        m.sites.scan.mockResolvedValue([mockSite()]);
        renderSites();
        await waitFor(() => screen.getByText("Scan"));
        await userEvent.click(screen.getByText("Scan"));
        await waitFor(() => expect(screen.getByText("myapp.test")).toBeInTheDocument());
    });

    it("shows scanned dirs from config", async () => {
        setupDefaults({ config: mockConfig({ scanned_dirs: ["D:\\projects"] }) });
        renderSites();
        await waitFor(() => expect(screen.getByText("D:\\projects")).toBeInTheDocument());
    });

    it("adds dir via Enter key in input", async () => {
        setupDefaults();
        m.config.update.mockResolvedValue(mockConfig({ scanned_dirs: ["D:\\new"] }));
        renderSites();
        await waitFor(() => screen.getByPlaceholderText("C:\\Projects"));
        await userEvent.type(screen.getByPlaceholderText("C:\\Projects"), "D:\\new");
        await userEvent.keyboard("{Enter}");
        await waitFor(() =>
            expect(m.config.update).toHaveBeenCalledWith(
                expect.objectContaining({ scanned_dirs: expect.arrayContaining(["D:\\new"]) })
            )
        );
    });

    it("removes a scanned dir on ✕ click", async () => {
        setupDefaults({ config: mockConfig({ scanned_dirs: ["D:\\projects"] }) });
        m.config.update.mockResolvedValue(mockConfig({ scanned_dirs: [] }));
        renderSites();
        await waitFor(() => screen.getByText("D:\\projects"));
        await userEvent.click(screen.getByText("✕"));
        await waitFor(() =>
            expect(m.config.update).toHaveBeenCalledWith(
                expect.objectContaining({ scanned_dirs: [] })
            )
        );
    });
});

// ── Information tab ───────────────────────────────────────────────────────────

describe("Information tab", () => {
    it("loads site info on tab switch", async () => {
        await renderAndSelect();
        await userEvent.click(screen.getByText("Information"));
        await waitFor(() => expect(m.sites.info).toHaveBeenCalledWith("site-1"));
    });

    it("shows framework info when available", async () => {
        m.sites.info.mockResolvedValue({
            app_name: "My Laravel App", framework_name: "Laravel",
            framework_version: "11.0", app_env: "local", app_debug: true,
            app_url: "http://myapp.test", app_timezone: "UTC",
            app_locale: "en", maintenance_mode: false,
        });
        setupDefaults({ sites: [mockSite()] });
        render(<MemoryRouter><Sites /></MemoryRouter>);
        await waitFor(() => screen.getByText("myapp.test"));
        await userEvent.click(screen.getByText("myapp.test"));
        await waitFor(() => screen.getByText("Open folder"));
        await userEvent.click(screen.getByText("Information"));
        await waitFor(() => {
            expect(screen.getByText("My Laravel App")).toBeInTheDocument();
            expect(screen.getByText("11.0")).toBeInTheDocument();
        });
    });

    it("shows empty state when no info available", async () => {
        await renderAndSelect();
        await userEvent.click(screen.getByText("Information"));
        await waitFor(() =>
            expect(screen.getByText(/No application info available/i)).toBeInTheDocument()
        );
    });
});

// ── Project type badges ───────────────────────────────────────────────────────

describe("Project type badges", () => {
    const types = [
        { type: "laravel",      label: "Laravel" },
        { type: "wordpress",    label: "WordPress" },
        { type: "codeigniter4", label: "CodeIgniter 4" },
        { type: "codeigniter3", label: "CodeIgniter 3" },
        { type: "spa",          label: "SPA" },
        { type: "static",       label: "Static" },
        { type: "generic",      label: "Generic" },
    ];

    for (const { type, label } of types) {
        it(`shows "${label}" badge for ${type} project`, async () => {
            await renderAndSelect(mockSite({ project_type: type as any }));
            expect(screen.getByText(label)).toBeInTheDocument();
        });
    }
});

// ── Refresh config re-detects project type ────────────────────────────────────

describe("Refresh config", () => {
    it("updates site in list after refreshConfig", async () => {
        const updated = mockSite({ project_type: "codeigniter3" });
        m.sites.refreshConfig.mockResolvedValue(updated);
        await renderAndSelect(mockSite({ project_type: "generic" }));

        expect(screen.getByText("Generic")).toBeInTheDocument();
        await userEvent.click(screen.getByText("↺ Refresh config"));

        await waitFor(() => {
            expect(m.sites.refreshConfig).toHaveBeenCalledWith("site-1");
        });
    });

    it("disables Refresh config button while in progress", async () => {
        m.sites.refreshConfig.mockReturnValue(new Promise(() => {}));
        await renderAndSelect();
        const btn = screen.getByText("↺ Refresh config").closest("button")!;
        await userEvent.click(btn);
        // Button is disabled while loading (shows spinner, text gone)
        await waitFor(() => expect(btn).toBeDisabled());
    });

    it("shows error if refreshConfig fails", async () => {
        m.sites.refreshConfig.mockRejectedValue(new Error("nginx error"));
        await renderAndSelect();
        await userEvent.click(screen.getByText("↺ Refresh config"));
        await waitFor(() => expect(screen.getByText("nginx error")).toBeInTheDocument());
    });
});

// ── PHP version per site ──────────────────────────────────────────────────────

describe("PHP version per site", () => {
    it("disables dropdown when no PHP versions detected", async () => {
        setupDefaults({ phpVersions: [] });
        renderSites();
        await waitFor(() => screen.getByText("myapp.test"));
        await userEvent.click(screen.getByText("myapp.test"));
        await waitFor(() => screen.getByRole("combobox"));
        expect(screen.getByRole("combobox")).toBeDisabled();
    });

    it("shows current php_version as fallback option when no versions detected", async () => {
        setupDefaults({ phpVersions: [], sites: [mockSite({ php_version: "7.4" })] });
        renderSites();
        await waitFor(() => screen.getByText("myapp.test"));
        await userEvent.click(screen.getByText("myapp.test"));
        await waitFor(() => screen.getByRole("combobox"));
        expect(screen.getByRole("combobox")).toHaveValue("7.4");
    });

    it("refreshes config after PHP version change", async () => {
        setupDefaults({
            phpVersions: [
                mockPhpVersion({ version: "8.2.0", major: "8.2" }),
                mockPhpVersion({ version: "8.1.0", major: "8.1" }),
            ],
        });
        m.sites.update.mockResolvedValue(mockSite({ php_version: "8.1" }));
        renderSites();
        await waitFor(() => screen.getByText("myapp.test"));
        await userEvent.click(screen.getByText("myapp.test"));
        await waitFor(() => screen.getByRole("combobox"));
        await userEvent.selectOptions(screen.getByRole("combobox"), "8.1");
        await waitFor(() => expect(m.sites.refreshConfig).toHaveBeenCalled());
    });
});

// ── Error handling ────────────────────────────────────────────────────────────

describe("Error handling", () => {
    it("shows error message when scan fails", async () => {
        setupDefaults({ sites: [] });
        m.sites.scan.mockRejectedValue(new Error("Directory not found"));
        renderSites();
        await waitFor(() => screen.getByText("Scan"));
        await userEvent.click(screen.getByText("Scan"));
        await waitFor(() => expect(screen.getByText("Directory not found")).toBeInTheDocument());
    });

    it("shows error message when delete fails", async () => {
        m.sites.delete.mockRejectedValue(new Error("Permission denied"));
        await renderAndSelect();
        await userEvent.click(screen.getByText("Remove"));
        await userEvent.click(screen.getByText("Yes"));
        await waitFor(() => expect(screen.getByText("Permission denied")).toBeInTheDocument());
    });

    it("shows error when SSL toggle fails", async () => {
        m.sites.enableSSL.mockRejectedValue(new Error("mkcert not found"));
        await renderAndSelect(mockSite({ ssl_enabled: false }));
        await userEvent.click(screen.getByText("Enable HTTPS"));
        await waitFor(() => expect(screen.getByText("mkcert not found")).toBeInTheDocument());
    });
});
