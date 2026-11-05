import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import Node from "./Node";

vi.mock("../api/client", () => ({
  api: {
    node: {
      status:          vi.fn(),
      releases:        vi.fn(),
      install:         vi.fn(),
      installProgress: vi.fn(),
      activate:        vi.fn(),
      uninstall:       vi.fn(),
    },
  },
}));

import { api } from "../api/client";
const m = api as Record<string, any>;

const renderNode = () => render(<MemoryRouter><Node /></MemoryRouter>);

const status = (overrides = {}) => ({
  mode: "nvm-windows",
  store_dir: "C:\\Users\\me\\AppData\\Local\\nvm",
  link_path: "C:\\nvm4w\\nodejs",
  active: "22.19.0",
  installed: [
    { version: "24.21.0", path: "C:\\nvm\\v24.21.0" },
    { version: "22.19.0", path: "C:\\nvm\\v22.19.0" },
  ],
  warnings: [],
  ...overrides,
});

const release = (overrides = {}) => ({
  version: "24.21.0", major: 24, date: "2026-10-01", lts: "Krypton",
  security: false, installed: false, active: false, update_available: false, end_of_life: false,
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
  m.node.status.mockResolvedValue(status());
  m.node.releases.mockResolvedValue([
    release({ installed: true }),
    release({ version: "26.13.0", major: 26, lts: "Lithium" }),
  ]);
  m.node.install.mockResolvedValue({ ok: true, state: "pending" });
  m.node.installProgress.mockResolvedValue({ version: "26.13.0", state: "done", percent: 100, message: "" });
});

describe("Node page", () => {
  it("shows the active version and who manages it", async () => {
    renderNode();
    const hero = await screen.findByTestId("node-active");
    expect(within(hero).getByText("v22.19.0")).toBeInTheDocument();
    expect(within(hero).getByText(/nvm-windows/)).toBeInTheDocument();
    expect(within(hero).getByText("C:\\nvm4w\\nodejs")).toBeInTheDocument();
  });

  it("activate button calls api.node.activate and shows the new version", async () => {
    m.node.activate.mockResolvedValue(status({ active: "24.21.0" }));
    renderNode();
    await userEvent.click(await screen.findByRole("button", { name: "Activate Node 24.21.0" }));
    expect(m.node.activate).toHaveBeenCalledWith("24.21.0");
    await waitFor(() => expect(within(screen.getByTestId("node-active")).getByText("v24.21.0")).toBeInTheDocument());
  });

  it("the active version cannot be uninstalled", async () => {
    renderNode();
    expect(await screen.findByRole("button", { name: "Uninstall Node 22.19.0" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Uninstall Node 24.21.0" })).toBeEnabled();
  });

  it("installed releases show no Install button; others do and install them", async () => {
    renderNode();
    await screen.findByText("26.13.0");
    expect(screen.queryByRole("button", { name: "Install Node 24.21.0" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Install Node 26.13.0" }));
    expect(m.node.install).toHaveBeenCalledWith("26.13.0");
  });

  it("a releases error shows a message but the installed list still renders", async () => {
    m.node.releases.mockRejectedValue(new Error("nodejs.org unreachable"));
    renderNode();
    expect(await screen.findByText(/nodejs.org unreachable/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Activate Node 24.21.0" })).toBeInTheDocument();
  });

  it("Update on the active major activates the new patch once installed", async () => {
    m.node.releases.mockResolvedValue([
      release({ version: "22.21.0", major: 22, lts: "Jod", update_available: true }),
      release({ version: "24.22.0", major: 24, update_available: true }),
    ]);
    m.node.installProgress.mockResolvedValue({ version: "22.21.0", state: "done", percent: 100, message: "" });
    m.node.activate.mockResolvedValue(status({ active: "22.21.0" }));
    renderNode();
    await userEvent.click(await screen.findByRole("button", { name: "Install Node 22.21.0" }));
    await waitFor(() => expect(m.node.activate).toHaveBeenCalledWith("22.21.0"), { timeout: 4000 });
  });

  it("Update on a major that is not active only installs", async () => {
    m.node.releases.mockResolvedValue([release({ version: "24.22.0", major: 24, update_available: true })]);
    m.node.installProgress.mockResolvedValue({ version: "24.22.0", state: "done", percent: 100, message: "" });
    renderNode();
    await userEvent.click(await screen.findByRole("button", { name: "Install Node 24.22.0" }));
    await waitFor(() => expect(m.node.installProgress).toHaveBeenCalled(), { timeout: 4000 });
    await waitFor(() => expect(m.node.status).toHaveBeenCalledTimes(2), { timeout: 4000 });
    expect(m.node.activate).not.toHaveBeenCalled();
  });

  it("shows PATH warnings", async () => {
    m.node.status.mockResolvedValue(status({ warnings: ["Another Node on the system PATH: C:\\Program Files\\nodejs"] }));
    renderNode();
    expect(await screen.findByText(/C:\\Program Files\\nodejs/)).toBeInTheDocument();
  });
  it("offers the update of an installed line on its own row", async () => {
    m.node.releases.mockResolvedValue([release({ version: "24.22.0", major: 24, update_available: true })]);
    renderNode();
    const row = await screen.findByTestId("node-installed-24.21.0");
    expect(within(row).getByRole("button", { name: "Install Node 24.22.0" })).toHaveTextContent("Update to 24.22.0");
    expect(screen.queryByTestId("node-release-24")).not.toBeInTheDocument();
  });

  it("keeps end-of-life lines behind a toggle and installs them on demand", async () => {
    m.node.releases.mockResolvedValue([
      release({ version: "26.13.0", major: 26, lts: "Lithium" }),
      release({ version: "10.24.1", major: 10, lts: "Dubnium", date: "2021-04-06", end_of_life: true }),
      release({ version: "8.17.0", major: 8, lts: "Carbon", date: "2019-12-17", end_of_life: true }),
    ]);
    m.node.installProgress.mockResolvedValue({ version: "10.24.1", state: "done", percent: 100, message: "" });
    renderNode();
    const toggle = await screen.findByRole("button", { name: /Show 2 end-of-life versions/ });
    expect(screen.getByTestId("node-release-26")).toBeInTheDocument();
    expect(screen.queryByTestId("node-release-10")).not.toBeInTheDocument();

    await userEvent.click(toggle);
    const node10 = screen.getByTestId("node-release-10");
    expect(within(node10).getByText("End of life")).toBeInTheDocument();
    expect(within(node10).getByText("LTS Dubnium")).toBeInTheDocument();
    await userEvent.click(within(node10).getByRole("button", { name: "Install Node 10.24.1" }));
    await waitFor(() => expect(m.node.install).toHaveBeenCalledWith("10.24.1"));
  });

  it("marks an installed version whose line is end of life", async () => {
    m.node.status.mockResolvedValue(status({ installed: [{ version: "10.22.0", path: "C:\nvm\v10.22.0" }], active: "10.22.0" }));
    m.node.releases.mockResolvedValue([release({ version: "10.24.1", major: 10, lts: "Dubnium", end_of_life: true, update_available: true })]);
    renderNode();
    const row = await screen.findByTestId("node-installed-10.22.0");
    expect(within(row).getByText("End of life")).toBeInTheDocument();
    expect(within(row).getByRole("button", { name: "Install Node 10.24.1" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /end-of-life/ })).not.toBeInTheDocument();
  });
});

