import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import Composer from "./Composer";
import { ToastProvider } from "../context/ToastContext";

vi.mock("../api/client", () => ({
  api: {
    composer: {
      status:           vi.fn(),
      install:          vi.fn(),
      installProgress:  vi.fn(),
      enableExtensions: vi.fn(),
    },
  },
}));

import { api } from "../api/client";
const m = api as Record<string, any>;

const renderPage = () => render(<MemoryRouter><ToastProvider><Composer /></ToastProvider></MemoryRouter>);

const status = (overrides = {}) => ({
  installed: false,
  version: null,
  latest: "2.11.2",
  update_available: false,
  phar_path: "C:\\phpenv\\composer\\composer.phar",
  command_path: "C:\\phpenv\\bin\\composer.bat",
  bin_dir: "C:\\phpenv\\bin",
  default_php: "8.4",
  php_version: "8.4.10",
  command_php: null,
  missing_extensions: [],
  can_enable_extensions: false,
  warnings: [],
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
  m.composer.installProgress.mockResolvedValue({ state: "idle", message: "", percent: 0 });
});

describe("Composer page", () => {
  it("offers to install the newest compatible release", async () => {
    m.composer.status.mockResolvedValue(status());
    m.composer.install.mockResolvedValue({ ok: true, state: "pending" });
    renderPage();
    const btn = await screen.findByRole("button", { name: /Install 2.11.2/ });
    expect(screen.getByText("Not installed")).toBeInTheDocument();
    await userEvent.click(btn);
    expect(m.composer.install).toHaveBeenCalled();
    expect(await screen.findByTestId("composer-progress")).toBeInTheDocument();
  });

  it("blocks the install when the default PHP is missing", async () => {
    m.composer.status.mockResolvedValue(status({ php_version: null }));
    renderPage();
    expect(await screen.findByRole("button", { name: /Install/ })).toBeDisabled();
    expect(screen.getByText(/Composer needs the default PHP/)).toBeInTheDocument();
  });

  it("shows the update and the warnings", async () => {
    m.composer.status.mockResolvedValue(status({
      installed: true, version: "2.10.0", update_available: true,
      warnings: ["Another Composer comes first on the PATH: C:\\ProgramData\\ComposerSetup\\bin."],
    }));
    renderPage();
    expect(await screen.findByRole("button", { name: /Update to 2.11.2/ })).toBeInTheDocument();
    expect(screen.getByText("2.10.0")).toBeInTheDocument();
    expect(screen.getByText(/ComposerSetup/)).toBeInTheDocument();
  });

  it("offers Repair when up to date", async () => {
    m.composer.status.mockResolvedValue(status({ installed: true, version: "2.11.2" }));
    m.composer.install.mockResolvedValue({ ok: true, state: "done" });
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: /Repair/ }));
    await waitFor(() => expect(m.composer.status).toHaveBeenCalledTimes(2));
  });

  it("enables missing extensions", async () => {
    m.composer.status.mockResolvedValue(status({ missing_extensions: ["zip"], can_enable_extensions: true }));
    m.composer.enableExtensions.mockResolvedValue(status());
    renderPage();
    expect(await screen.findByTestId("composer-extensions")).toHaveTextContent("zip");
    await userEvent.click(screen.getByRole("button", { name: "Enable" }));
    await waitFor(() => expect(screen.queryByTestId("composer-extensions")).not.toBeInTheDocument());
  });

  it("shows install errors", async () => {
    m.composer.status.mockResolvedValue(status());
    m.composer.install.mockRejectedValue(new Error("getcomposer.org unreachable"));
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: /Install/ }));
    expect(await screen.findByText(/getcomposer.org unreachable/)).toBeInTheDocument();
  });
});
