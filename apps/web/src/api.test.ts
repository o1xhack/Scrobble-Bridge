import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiClient } from "./api";

afterEach(() => vi.unstubAllGlobals());

describe("daemon API client", () => {
  it("keeps the admin token in an authorization header", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({ configured: false, phase: "needs_setup" }),
        {
          status: 200,
          headers: { "Content-Type": "application/json" },
        },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);

    await new ApiClient("private-admin-token").status();

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/status",
      expect.objectContaining({
        headers: expect.objectContaining({
          Authorization: "Bearer private-admin-token",
        }),
      }),
    );
  });

  it("surfaces the daemon error message without returning a successful value", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({ message: "A valid admin bearer token is required" }),
          {
            status: 401,
            headers: { "Content-Type": "application/json" },
          },
        ),
      ),
    );

    await expect(new ApiClient("wrong").status()).rejects.toThrow(
      "A valid admin bearer token is required",
    );
  });
});
