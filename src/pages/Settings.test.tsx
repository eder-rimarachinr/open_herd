import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import Settings from "./Settings";
import { mockConfig } from "../test/mocks/api";

vi.mock("../api/client", () => ({
  api: {
    config: { get: vi.fn(), update: vi.fn() },
    php:    { versions: vi.fn() },
    node:   { status: vi.fn() },
  },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

import { api } from "../api/client";
const m = api as unknown as Record<string, Record<string, ReturnType<typeof vi.fn>>>;

const renderSettings = () => render(<MemoryRouter><Settings /></MemoryRouter>);

beforeEach(() => {
  vi.clearAllMocks();
  m.config.get.mockResolvedValue(mockConfig());
  m.php.versions.mockResolvedValue([]);
});

describe("Data paths", () => {
  it("shows where Node versions live, even when nvm keeps them outside .phpenv", async () => {
    m.node.status.mockResolvedValue({
      mode: "nvm-windows", store_dir: "C:\\Users\\user\\AppData\\Local\\nvm", link_path: "C:\\nvm4w\\nodejs",
      active: "24.21.0", installed: [], warnings: [],
    });
    renderSettings();
    expect(await screen.findByText("C:\\Users\\user\\AppData\\Local\\nvm")).toBeInTheDocument();
    expect(screen.getByText("Node")).toBeInTheDocument();
    expect(screen.getByText(/nvm-windows/)).toBeInTheDocument();
  });

  it("still renders the other paths if the Node status fails", async () => {
    m.node.status.mockRejectedValue(new Error("not supported"));
    renderSettings();
    expect(await screen.findByText("C:\\Users\\user\\.phpenv\\php")).toBeInTheDocument();
    expect(screen.queryByText("Node")).not.toBeInTheDocument();
  });
});
