import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import Nginx from "./Nginx";
import { ToastProvider } from "../context/ToastContext";

vi.mock("../api/client", () => ({
  api: {
    nginx: {
      info:             vi.fn(),
      downloadProgress: vi.fn(),
      download:         vi.fn(),
      start:            vi.fn(),
      stop:             vi.fn(),
      reload:           vi.fn(),
      allowPorts:       vi.fn(),
    },
    config: { get: vi.fn() },
    sites:  { list: vi.fn() },
  },
}));

vi.mock("../components/LogView", () => ({ default: () => null }));

import { api } from "../api/client";
const m = api as Record<string, any>;

const SETCAP = "sudo setcap cap_net_bind_service=+ep '/usr/sbin/nginx'";

const info = (port_access = {}) => ({
  installed: true, running: false, version: "1.28.3", binary_path: "/usr/sbin/nginx",
  config_valid: null, config_error: "", error_log: "", downloadable: false, os: "linux",
  port_access: { needed: false, commands: [], can_prompt: false, ...port_access },
});

const renderNginx = () => render(<MemoryRouter><ToastProvider><Nginx /></ToastProvider></MemoryRouter>);

beforeEach(() => {
  vi.clearAllMocks();
  m.config.get.mockResolvedValue({ http_port: 80, https_port: 443 });
  m.sites.list.mockResolvedValue([]);
});

describe("Nginx port permission", () => {
  it("stays hidden when nothing is needed", async () => {
    m.nginx.info.mockResolvedValue(info());
    renderNginx();
    await screen.findByText("/usr/sbin/nginx", { exact: false });
    expect(screen.queryByTestId("nginx-port-access")).not.toBeInTheDocument();
  });

  it("offers the password prompt and refreshes once it is granted", async () => {
    m.nginx.info
      .mockResolvedValueOnce(info({ needed: true, commands: [SETCAP], can_prompt: true }))
      .mockResolvedValue(info());
    m.nginx.allowPorts.mockResolvedValue({ needed: false, commands: [], can_prompt: true });
    renderNginx();
    const panel = await screen.findByTestId("nginx-port-access");
    expect(within(panel).getByText(SETCAP)).toBeInTheDocument();

    await userEvent.click(within(panel).getByRole("button", { name: /asks for your password/ }));
    expect(m.nginx.allowPorts).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByTestId("nginx-port-access")).not.toBeInTheDocument());
    expect(await screen.findByText(/can use ports 80 and 443/)).toBeInTheDocument();
  });

  it("only shows the commands when no password prompt is available", async () => {
    const disable = "sudo systemctl disable --now nginx";
    m.nginx.info.mockResolvedValue(info({ needed: true, commands: [disable, SETCAP], can_prompt: false }));
    renderNginx();
    const panel = await screen.findByTestId("nginx-port-access");
    expect(within(panel).getByText(/Run this once in a terminal/)).toBeInTheDocument();
    expect(within(panel).getByText(/own nginx service is using port 80/)).toBeInTheDocument();
    expect(within(panel).queryByRole("button")).not.toBeInTheDocument();
  });

  it("a cancelled prompt shows the error and keeps the panel", async () => {
    m.nginx.info.mockResolvedValue(info({ needed: true, commands: [SETCAP], can_prompt: true }));
    m.nginx.allowPorts.mockRejectedValue(new Error("Cancelled: nothing was changed."));
    renderNginx();
    await userEvent.click(await screen.findByRole("button", { name: /asks for your password/ }));
    expect((await screen.findAllByText(/Cancelled: nothing was changed/)).length).toBeGreaterThan(0);
    expect(screen.getByTestId("nginx-port-access")).toBeInTheDocument();
  });
});
