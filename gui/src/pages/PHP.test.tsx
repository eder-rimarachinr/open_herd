import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import PHP from "./PHP";
import { mockConfig } from "../test/mocks/api";

// ── Module mock ───────────────────────────────────────────────────────────────

vi.mock("../api/client", () => ({
  api: {
    php: {
      catalog:         vi.fn(),
      detect:          vi.fn(),
      install:         vi.fn(),
      installProgress: vi.fn(),
      versions:        vi.fn(),
      start:           vi.fn(),
      stop:            vi.fn(),
    },
    config: { get: vi.fn(), update: vi.fn() },
    services: { start: vi.fn(), status: vi.fn(), stop: vi.fn() },
  },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

import { api } from "../api/client";
const m = api as Record<string, any>;

// ── Helpers ───────────────────────────────────────────────────────────────────

const renderPHP = () => render(<MemoryRouter><PHP /></MemoryRouter>);

const mockCatalogEntry = (overrides = {}) => ({
  major: "8.2",
  latest_patch: "8.2.29",
  installed_patch: undefined,
  installed: false,
  running: false,
  has_update: false,
  security_only: false,
  end_of_life: false,
  ...overrides,
});

function setupDefaults(overrides: { catalog?: any[]; config?: any } = {}) {
  m.php.catalog.mockResolvedValue(overrides.catalog ?? [mockCatalogEntry()]);
  m.config.get.mockResolvedValue(overrides.config ?? mockConfig({ default_php: "8.2" }));
  m.config.update.mockImplementation((body: any) =>
    Promise.resolve(mockConfig(body))
  );
  m.php.detect.mockResolvedValue(overrides.catalog ?? [mockCatalogEntry()]);
  m.services.start.mockResolvedValue({});
}

beforeEach(() => {
  vi.clearAllMocks();
  m.php.install.mockResolvedValue({});
  m.php.installProgress.mockResolvedValue({ state: "done", percent: 100, message: "" });
});

// ── Loading ───────────────────────────────────────────────────────────────────

describe("Loading", () => {
  it("shows loading text while catalog is fetching", () => {
    m.php.catalog.mockReturnValue(new Promise(() => {}));
    m.config.get.mockReturnValue(new Promise(() => {}));
    renderPHP();
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });
});

// ── Catalog display ───────────────────────────────────────────────────────────

describe("Catalog display", () => {
  it("renders PHP version rows from catalog", async () => {
    setupDefaults({ catalog: [mockCatalogEntry({ major: "8.2" })] });
    renderPHP();
    await waitFor(() => expect(screen.getByText(/8\.2/)).toBeInTheDocument());
  });

  it("shows multiple versions", async () => {
    setupDefaults({
      catalog: [
        mockCatalogEntry({ major: "8.4" }),
        mockCatalogEntry({ major: "8.3" }),
        mockCatalogEntry({ major: "8.2" }),
      ],
    });
    renderPHP();
    await waitFor(() => {
      expect(screen.getByText(/8\.4/)).toBeInTheDocument();
      expect(screen.getByText(/8\.3/)).toBeInTheDocument();
      expect(screen.getByText(/8\.2/)).toBeInTheDocument();
    });
  });

  it("shows installed patch version when installed", async () => {
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.2", installed: true, installed_patch: "8.2.10" })],
    });
    renderPHP();
    await waitFor(() => expect(screen.getByText(/8\.2\.10/)).toBeInTheDocument());
  });

  it("shows checkmark for installed versions", async () => {
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.2", installed: true, installed_patch: "8.2.10" })],
    });
    renderPHP();
    await waitFor(() => expect(screen.getByText("✓")).toBeInTheDocument());
  });

  it("shows EOL tag for end-of-life versions", async () => {
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "7.4", end_of_life: true })],
    });
    renderPHP();
    await waitFor(() => expect(screen.getByText("EOL")).toBeInTheDocument());
  });

  it("shows security tag for security-only versions", async () => {
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.1", security_only: true })],
    });
    renderPHP();
    await waitFor(() => expect(screen.getByText("security")).toBeInTheDocument());
  });

  it("shows Install button for non-installed versions", async () => {
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.3", installed: false })],
    });
    renderPHP();
    await waitFor(() => expect(screen.getByText("Install")).toBeInTheDocument());
  });

  it("shows Update button for installed versions with update", async () => {
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.2", installed: true, has_update: true, installed_patch: "8.2.0" })],
    });
    renderPHP();
    await waitFor(() => expect(screen.getByText("Update")).toBeInTheDocument());
  });

  it("does not show Install/Update for current without update", async () => {
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.2", installed: true, has_update: false, installed_patch: "8.2.29" })],
    });
    renderPHP();
    await waitFor(() => screen.getByText(/8\.2/));
    expect(screen.queryByText("Install")).not.toBeInTheDocument();
    expect(screen.queryByText("Update")).not.toBeInTheDocument();
  });
});

// ── Active PHP ────────────────────────────────────────────────────────────────

describe("Active PHP", () => {
  it("shows radio button for installed versions", async () => {
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.2", installed: true, installed_patch: "8.2.10" })],
    });
    renderPHP();
    await waitFor(() => expect(screen.getByRole("radio")).toBeInTheDocument());
  });

  it("radio is checked for the default PHP version", async () => {
    setupDefaults({
      catalog: [
        mockCatalogEntry({ major: "8.2", installed: true, installed_patch: "8.2.10" }),
        mockCatalogEntry({ major: "8.1", installed: true, installed_patch: "8.1.5" }),
      ],
      config: mockConfig({ default_php: "8.2" }),
    });
    renderPHP();
    await waitFor(() => {
      const radios = screen.getAllByRole("radio");
      expect(radios[0]).toBeChecked(); // 8.2 first
      expect(radios[1]).not.toBeChecked();
    });
  });

  it("calls config.update when a different version radio is selected", async () => {
    setupDefaults({
      catalog: [
        mockCatalogEntry({ major: "8.2", installed: true, installed_patch: "8.2.10" }),
        mockCatalogEntry({ major: "8.1", installed: true, installed_patch: "8.1.5" }),
      ],
      config: mockConfig({ default_php: "8.2" }),
    });
    renderPHP();
    await waitFor(() => screen.getAllByRole("radio"));
    const radios = screen.getAllByRole("radio");
    await userEvent.click(radios[1]); // select 8.1
    await waitFor(() =>
      expect(m.config.update).toHaveBeenCalledWith(
        expect.objectContaining({ default_php: "8.1" })
      )
    );
  });

  it("does not call config.update when current default radio is clicked", async () => {
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.2", installed: true, installed_patch: "8.2.10" })],
      config: mockConfig({ default_php: "8.2" }),
    });
    renderPHP();
    await waitFor(() => screen.getByRole("radio"));
    await userEvent.click(screen.getByRole("radio"));
    expect(m.config.update).not.toHaveBeenCalled();
  });

  it("calls services.start after changing active PHP", async () => {
    setupDefaults({
      catalog: [
        mockCatalogEntry({ major: "8.2", installed: true, installed_patch: "8.2.10" }),
        mockCatalogEntry({ major: "8.1", installed: true, installed_patch: "8.1.5" }),
      ],
      config: mockConfig({ default_php: "8.2" }),
    });
    renderPHP();
    await waitFor(() => screen.getAllByRole("radio"));
    await userEvent.click(screen.getAllByRole("radio")[1]);
    await waitFor(() => expect(m.services.start).toHaveBeenCalled());
  });
});

// ── Rescan ────────────────────────────────────────────────────────────────────

describe("Rescan", () => {
  it("calls php.detect on Rescan click", async () => {
    setupDefaults();
    renderPHP();
    await waitFor(() => screen.getByText("↺ Rescan"));
    await userEvent.click(screen.getByText("↺ Rescan"));
    await waitFor(() => expect(m.php.detect).toHaveBeenCalled());
  });

  it("shows Scanning… while rescan is in progress", async () => {
    setupDefaults();
    m.php.detect.mockReturnValue(new Promise(() => {}));
    renderPHP();
    await waitFor(() => screen.getByText("↺ Rescan"));
    await userEvent.click(screen.getByText("↺ Rescan"));
    expect(screen.getByText("Scanning…")).toBeInTheDocument();
  });

  it("updates catalog after rescan", async () => {
    setupDefaults({ catalog: [] });
    m.php.detect.mockResolvedValue([mockCatalogEntry({ major: "8.4" })]);
    renderPHP();
    await waitFor(() => screen.getByText("↺ Rescan"));
    await userEvent.click(screen.getByText("↺ Rescan"));
    await waitFor(() => expect(screen.getByText(/8\.4/)).toBeInTheDocument());
  });
});

// ── Install ───────────────────────────────────────────────────────────────────

describe("Install", () => {
  it("calls php.install when Install is clicked", async () => {
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.3", installed: false })],
    });
    renderPHP();
    await waitFor(() => screen.getByText("Install"));
    await userEvent.click(screen.getByText("Install"));
    await waitFor(() =>
      expect(m.php.install).toHaveBeenCalledWith("8.3")
    );
  });

  it("shows pending state immediately after Install click", async () => {
    m.php.install.mockReturnValue(new Promise(() => {}));
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.3", installed: false })],
    });
    renderPHP();
    await waitFor(() => screen.getByText("Install"));
    await userEvent.click(screen.getByText("Install"));
    await waitFor(() => expect(screen.getByText("Starting…")).toBeInTheDocument());
  });

  it("shows error and dismiss button when install fails", async () => {
    m.php.install.mockRejectedValue(new Error("Download failed"));
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.3", installed: false })],
    });
    renderPHP();
    await waitFor(() => screen.getByText("Install"));
    await userEvent.click(screen.getByText("Install"));
    await waitFor(() => expect(screen.getByText(/Download failed/)).toBeInTheDocument());
  });

  it("dismisses error on error button click", async () => {
    m.php.install.mockRejectedValue(new Error("Download failed"));
    setupDefaults({
      catalog: [mockCatalogEntry({ major: "8.3", installed: false })],
    });
    renderPHP();
    await waitFor(() => screen.getByText("Install"));
    await userEvent.click(screen.getByText("Install"));
    await waitFor(() => screen.getByText(/Download failed/));
    await userEvent.click(screen.getByText(/✕/));
    await waitFor(() =>
      expect(screen.queryByText(/Download failed/)).not.toBeInTheDocument()
    );
  });
});

// ── Custom PHP paths ──────────────────────────────────────────────────────────

describe("Custom PHP paths", () => {
  it("renders custom php dirs from config", async () => {
    setupDefaults({
      config: mockConfig({ custom_php_dirs: ["C:\\php83"] }),
    });
    renderPHP();
    await waitFor(() => expect(screen.getByText("C:\\php83")).toBeInTheDocument());
  });

  it("adds custom dir on Enter key", async () => {
    setupDefaults();
    m.config.update.mockResolvedValue(mockConfig({ custom_php_dirs: ["C:\\php83"] }));
    renderPHP();
    await waitFor(() => screen.getByPlaceholderText(/C:\\path/i));
    await userEvent.type(screen.getByPlaceholderText(/C:\\path/i), "C:\\php83");
    await userEvent.keyboard("{Enter}");
    await waitFor(() =>
      expect(m.config.update).toHaveBeenCalledWith(
        expect.objectContaining({ custom_php_dirs: expect.arrayContaining(["C:\\php83"]) })
      )
    );
  });

  it("adds custom dir on Add & Rescan click", async () => {
    setupDefaults();
    m.config.update.mockResolvedValue(mockConfig({ custom_php_dirs: ["C:\\php83"] }));
    renderPHP();
    await waitFor(() => screen.getByPlaceholderText(/C:\\path/i));
    await userEvent.type(screen.getByPlaceholderText(/C:\\path/i), "C:\\php83");
    await userEvent.click(screen.getByText("Add & Rescan"));
    await waitFor(() => expect(m.config.update).toHaveBeenCalled());
  });

  it("removes custom dir on ✕ click", async () => {
    setupDefaults({
      config: mockConfig({ custom_php_dirs: ["C:\\php83"] }),
    });
    m.config.update.mockResolvedValue(mockConfig({ custom_php_dirs: [] }));
    renderPHP();
    await waitFor(() => screen.getByText("C:\\php83"));
    await userEvent.click(screen.getByTitle("Remove path"));
    await waitFor(() =>
      expect(m.config.update).toHaveBeenCalledWith(
        expect.objectContaining({ custom_php_dirs: [] })
      )
    );
  });

  it("triggers rescan after adding a custom dir", async () => {
    setupDefaults();
    m.config.update.mockResolvedValue(mockConfig({ custom_php_dirs: ["C:\\php83"] }));
    renderPHP();
    await waitFor(() => screen.getByPlaceholderText(/C:\\path/i));
    await userEvent.type(screen.getByPlaceholderText(/C:\\path/i), "C:\\php83");
    await userEvent.click(screen.getByText("Add & Rescan"));
    await waitFor(() => expect(m.php.detect).toHaveBeenCalled());
  });

  it("clears input after adding dir", async () => {
    setupDefaults();
    m.config.update.mockResolvedValue(mockConfig({ custom_php_dirs: ["C:\\php83"] }));
    renderPHP();
    await waitFor(() => screen.getByPlaceholderText(/C:\\path/i));
    const input = screen.getByPlaceholderText(/C:\\path/i);
    await userEvent.type(input, "C:\\php83");
    await userEvent.click(screen.getByText("Add & Rescan"));
    await waitFor(() => expect((input as HTMLInputElement).value).toBe(""));
  });

  it("does not add empty custom dir", async () => {
    setupDefaults();
    renderPHP();
    await waitFor(() => screen.getByText("Add & Rescan"));
    await userEvent.click(screen.getByText("Add & Rescan"));
    expect(m.config.update).not.toHaveBeenCalled();
  });
});
