export interface SyncReport {
  source_outcome: "baseline" | "delta" | "gap";
  overlap_matches: number;
  discovered: number;
  enqueued: number;
  matched_existing: number;
  submitted: number;
  accepted: number;
  retryable: number;
  rejected: number;
  gap_best_overlap: number | null;
}

export type SyncFeedback =
  | { kind: "success"; reason: "baseline" | "no_changes" }
  | { kind: "success"; reason: "synced"; count: number }
  | { kind: "error"; reason: "history_gap" | "submission_incomplete" };

export function classifySyncReport(report: SyncReport): SyncFeedback {
  if (report.source_outcome === "gap") {
    return { kind: "error", reason: "history_gap" };
  }
  if (report.retryable > 0 || report.rejected > 0) {
    return { kind: "error", reason: "submission_incomplete" };
  }
  if (report.source_outcome === "baseline") {
    return { kind: "success", reason: "baseline" };
  }
  if (report.accepted > 0) {
    return { kind: "success", reason: "synced", count: report.accepted };
  }
  return { kind: "success", reason: "no_changes" };
}
