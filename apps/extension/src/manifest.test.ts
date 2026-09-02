import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

interface Manifest {
  manifest_version: number;
  homepage_url: string;
  permissions: string[];
  optional_permissions: string[];
  optional_host_permissions: string[];
  background: { service_worker: string; type: string };
}

describe("Manifest V3 privacy boundary", () => {
  it("keeps permissions and origins at the reviewed 1.0 set", () => {
    const manifest = JSON.parse(
      readFileSync(new URL("../manifest.json", import.meta.url), "utf8"),
    ) as Manifest;

    expect(manifest.manifest_version).toBe(3);
    expect(manifest.homepage_url).toBe(
      "https://scrobble-bridge.o1xhack.com/",
    );
    expect(manifest.permissions).toEqual([
      "alarms",
      "nativeMessaging",
      "storage",
    ]);
    expect(manifest.optional_permissions).toEqual(["cookies"]);
    expect(manifest.optional_host_permissions).toEqual([
      "https://music.youtube.com/*",
      "https://youtube.com/*",
      "https://*/*",
    ]);
    expect(manifest.background).toEqual({
      service_worker: "background.js",
      type: "module",
    });
  });
});
