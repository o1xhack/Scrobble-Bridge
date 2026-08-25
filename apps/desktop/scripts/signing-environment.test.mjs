import { describe, expect, it } from "vitest";
import { createSigningEnvironment } from "./signing-environment.mjs";

describe("macOS signing environment", () => {
  it("removes an empty GitHub Actions signing identity", () => {
    const environment = { APPLE_SIGNING_IDENTITY: "", PATH: "/usr/bin" };

    expect(createSigningEnvironment(environment, undefined)).toEqual({
      PATH: "/usr/bin",
    });
    expect(environment.APPLE_SIGNING_IDENTITY).toBe("");
  });

  it("uses an available Developer ID without dropping unrelated settings", () => {
    expect(
      createSigningEnvironment(
        { APPLE_SIGNING_IDENTITY: "", SCROBBLE_LASTFM_API_KEY: "test-key" },
        "Developer ID Application: Example",
      ),
    ).toEqual({
      APPLE_SIGNING_IDENTITY: "Developer ID Application: Example",
      SCROBBLE_LASTFM_API_KEY: "test-key",
    });
  });
});
