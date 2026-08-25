import { describe, expect, it } from "vitest";
import { POPUP_COPY, resolveExtensionLocale } from "./i18n";

describe("extension localization", () => {
  it("supports English and Simplified Chinese", () => {
    expect(resolveExtensionLocale("en-US")).toBe("en");
    expect(resolveExtensionLocale("zh-Hans")).toBe("zh-CN");
    expect(POPUP_COPY.en.enableRefresh).toBeTruthy();
    expect(POPUP_COPY["zh-CN"].enableRefresh).toBeTruthy();
    expect(Object.keys(POPUP_COPY.en).sort()).toEqual(
      Object.keys(POPUP_COPY["zh-CN"]).sort(),
    );
  });
});
