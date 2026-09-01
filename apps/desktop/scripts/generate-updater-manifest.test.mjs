import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { createUpdaterManifest } from "./generate-updater-manifest.mjs";

describe("updater manifest", () => {
  it("maps a signed platform artifact to the immutable GitHub Release URL", () => {
    const directory = mkdtempSync(join(tmpdir(), "scrobble-updater-"));
    const artifact = join(
      directory,
      "Scrobble Bridge_1.0.0_aarch64.app.tar.gz",
    );
    writeFileSync(artifact, "artifact");
    writeFileSync(`${artifact}.sig`, "signature\n");

    const manifest = createUpdaterManifest({
      version: "1.0.0",
      notes: "Release notes",
      pubDate: "2026-08-25T00:00:00.000Z",
      platforms: { "darwin-aarch64": artifact },
    });

    expect(manifest.platforms["darwin-aarch64"]).toEqual({
      signature: "signature",
      url: "https://github.com/o1xhack/Scrobble-Bridge/releases/download/v1.0.0/Scrobble.Bridge_1.0.0_aarch64.app.tar.gz",
    });
    expect(readFileSync(`${artifact}.sig`, "utf8")).toContain("signature");
  });
});
