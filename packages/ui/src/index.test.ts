import { describe, expect, it } from "vitest";
import { resolveLocale } from "./i18n";
import { classifySyncReport, type SyncReport } from "./sync";
import { PHASE_LABELS, PHASE_LABELS_ZH_CN, phaseLabel } from "./status";

function report(overrides: Partial<SyncReport> = {}): SyncReport {
  return {
    source_outcome: "delta",
    overlap_matches: 2,
    discovered: 0,
    enqueued: 0,
    matched_existing: 0,
    submitted: 0,
    accepted: 0,
    retryable: 0,
    rejected: 0,
    gap_best_overlap: null,
    ...overrides,
  };
}

describe("shared status language", () => {
  it("has a user-facing label for every runtime phase", () => {
    expect(Object.keys(PHASE_LABELS).sort()).toEqual([
      "idle",
      "needs_attention",
      "needs_setup",
      "paused",
      "retry_waiting",
      "syncing",
    ]);
    expect(Object.keys(PHASE_LABELS_ZH_CN).sort()).toEqual(
      Object.keys(PHASE_LABELS).sort(),
    );
    expect(phaseLabel("syncing", "zh-CN")).toBe("正在同步");
    expect(phaseLabel("syncing", "en")).toBe("Syncing");
  });

  it("uses Simplified Chinese for every Chinese system locale", () => {
    expect(resolveLocale(null, "zh-CN")).toBe("zh-CN");
    expect(resolveLocale(undefined, "zh-Hans-US")).toBe("zh-CN");
    expect(resolveLocale("en", "zh-CN")).toBe("en");
  });
});

describe("classifySyncReport", () => {
  it("never classifies a history gap as success", () => {
    expect(
      classifySyncReport(
        report({ source_outcome: "gap", gap_best_overlap: 0 }),
      ),
    ).toEqual({ kind: "error", reason: "history_gap" });
  });

  it("reports incomplete Last.fm submissions as an error", () => {
    expect(classifySyncReport(report({ retryable: 1 }))).toEqual({
      kind: "error",
      reason: "submission_incomplete",
    });
  });

  it("distinguishes successful submissions from a no-change check", () => {
    expect(classifySyncReport(report({ accepted: 3 }))).toEqual({
      kind: "success",
      reason: "synced",
      count: 3,
    });
    expect(classifySyncReport(report())).toEqual({
      kind: "success",
      reason: "no_changes",
    });
  });
});
