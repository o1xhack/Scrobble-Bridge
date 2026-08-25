import { describe, expect, it } from "vitest";
import {
  cookieHeader,
  hasSapisidCookie,
  parseYouTubeAccountContext,
  shouldRefreshChangedCookie,
  signedHeaders,
} from "./protocol";

describe("credential protocol", () => {
  it("includes only the named YouTube authentication cookies in stable order", () => {
    const header = cookieHeader([
      { name: "unrelated", value: "leak" },
      { name: "SAPISID", value: "two" },
      { name: "__Secure-3PAPISID", value: "one" },
    ] as chrome.cookies.Cookie[]);
    expect(header).toBe("__Secure-3PAPISID=one; SAPISID=two");
    expect(header).not.toContain("leak");
  });

  it("accepts every SAPISID variant supported by the runtime", () => {
    for (const name of ["__Secure-3PAPISID", "SAPISID", "__Secure-1PAPISID"]) {
      expect(
        hasSapisidCookie([
          { name, value: "present" },
        ] as chrome.cookies.Cookie[]),
      ).toBe(true);
    }
    expect(
      hasSapisidCookie([
        { name: "__Secure-1PAPISID", value: "" },
        { name: "SID", value: "present" },
      ] as chrome.cookies.Cookie[]),
    ).toBe(false);
  });

  it("detects the active Google account instead of assuming index zero", () => {
    expect(
      parseYouTubeAccountContext(
        '<script>ytcfg.set({"DATASYNC_ID":"123456789012345678901||","SESSION_INDEX":"1"});</script>',
      ),
    ).toEqual({
      accountId: "123456789012345678901",
      authUser: 1,
      delegatedSessionId: undefined,
    });
  });

  it("detects delegated channel context for brand accounts", () => {
    expect(
      parseYouTubeAccountContext(
        '<script>ytcfg.set({"DATASYNC_ID":"111111111111111111111||222222222222222222222","SESSION_INDEX":2});</script>',
      ),
    ).toEqual({
      accountId: "111111111111111111111",
      authUser: 2,
      delegatedSessionId: "111111111111111111111",
    });
  });

  it("fails closed when YouTube account context is absent", () => {
    expect(() => parseYouTubeAccountContext("<html></html>")).toThrow(
      "Could not detect the active YouTube Music account.",
    );
  });

  it("rejects missing, negative, and out-of-range Google account indices", () => {
    for (const session of ["", "-1", "256", "unknown"]) {
      const html = `<script>ytcfg.set({"DATASYNC_ID":"123456789012345678901||","SESSION_INDEX":"${session}"});</script>`;
      expect(() => parseYouTubeAccountContext(html)).toThrow(
        "Could not detect the active YouTube Music account.",
      );
    }
  });

  it("rejects malformed delegated-account identifiers", () => {
    for (const account of ["", "not-numeric", "123||bad", "1".repeat(129)]) {
      const html = `<script>ytcfg.set({"DATASYNC_ID":"${account}","SESSION_INDEX":"0"});</script>`;
      expect(() => parseYouTubeAccountContext(html)).toThrow(
        "Could not detect the active YouTube Music account.",
      );
    }
  });

  it("refreshes only allowlisted authentication cookies on YouTube domains", () => {
    for (const domain of ["youtube.com", ".youtube.com", "music.youtube.com"]) {
      expect(shouldRefreshChangedCookie({ name: "SAPISID", domain })).toBe(
        true,
      );
    }
    expect(
      shouldRefreshChangedCookie({ name: "unrelated", domain: ".youtube.com" }),
    ).toBe(false);
    expect(
      shouldRefreshChangedCookie({ name: "SAPISID", domain: "notyoutube.com" }),
    ).toBe(false);
  });

  it("creates deterministic HMAC headers for a fixed timestamp and nonce", async () => {
    const first = await signedHeaders(
      '{"test":true}',
      {
        endpoint: "https://nas.example",
        deviceId: "device",
        deviceToken: "token",
        serverId: "server",
      },
      1_700_000_000,
      "0123456789abcdef",
    );
    const second = await signedHeaders(
      '{"test":true}',
      {
        endpoint: "https://nas.example",
        deviceId: "device",
        deviceToken: "token",
        serverId: "server",
      },
      1_700_000_000,
      "0123456789abcdef",
    );
    expect(first).toEqual(second);
    expect(first["X-Scrobble-Signature"]).toMatch(/^[A-Za-z0-9_-]{43}$/);
  });

  it("changes the NAS signature when the payload or nonce changes", async () => {
    const connection = {
      endpoint: "https://nas.example",
      deviceId: "device",
      deviceToken: "token",
      serverId: "server",
    };
    const first = await signedHeaders(
      "first",
      connection,
      1_700_000_000,
      "nonce-one",
    );
    const changedPayload = await signedHeaders(
      "second",
      connection,
      1_700_000_000,
      "nonce-one",
    );
    const changedNonce = await signedHeaders(
      "first",
      connection,
      1_700_000_000,
      "nonce-two",
    );

    expect(first["X-Scrobble-Signature"]).not.toBe(
      changedPayload["X-Scrobble-Signature"],
    );
    expect(first["X-Scrobble-Signature"]).not.toBe(
      changedNonce["X-Scrobble-Signature"],
    );
  });
});
