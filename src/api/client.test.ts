import { afterEach, describe, expect, it, vi } from "vitest";
import { api } from "./client";

const ok = (body: unknown) =>
  new Response(JSON.stringify(body), { status: 200, headers: { "Content-Type": "application/json" } });

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("request retries while the daemon starts", () => {
  it("retries a refused GET until the daemon answers", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn()
      .mockRejectedValueOnce(new TypeError("Failed to fetch"))
      .mockRejectedValueOnce(new TypeError("Failed to fetch"))
      .mockResolvedValueOnce(ok([{ major: "8.5" }]));
    vi.stubGlobal("fetch", fetchMock);

    const result = api.php.versions();
    await vi.runAllTimersAsync();

    await expect(result).resolves.toEqual([{ major: "8.5" }]);
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it("does not retry an HTTP error", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ error: "boom" }), { status: 500 }),
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(api.php.versions()).rejects.toThrow("boom");
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("never retries a write", async () => {
    const fetchMock = vi.fn().mockRejectedValue(new TypeError("Failed to fetch"));
    vi.stubGlobal("fetch", fetchMock);

    await expect(api.php.start("8.5")).rejects.toThrow("Failed to fetch");
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});
