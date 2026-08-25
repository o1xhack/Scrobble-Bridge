export { default as StatusBadge } from "./StatusBadge.svelte";
export { PHASE_LABELS, PHASE_LABELS_ZH_CN, phaseLabel } from "./status";
export { dateLocale, LOCALE_STORAGE_KEY, resolveLocale } from "./i18n";
export type { Locale } from "./i18n";
export { classifySyncReport } from "./sync";
export type { SyncFeedback, SyncReport } from "./sync";

import type { SyncReport } from "./sync";

export type RuntimePhase =
  | "needs_setup"
  | "idle"
  | "syncing"
  | "paused"
  | "retry_waiting"
  | "needs_attention";

export interface RuntimeStatus {
  phase: RuntimePhase;
  configured: boolean;
  ytmusic_configured: boolean;
  ytmusic_account_name: string | null;
  ytmusic_channel_handle: string | null;
  lastfm_application_configured: boolean;
  lastfm_authorized: boolean;
  lastfm_username: string | null;
  paused: boolean;
  last_attempt_at: string | null;
  last_success_at: string | null;
  next_scheduled_at: string | null;
  last_error_code: string | null;
  last_error_message: string | null;
  last_report: SyncReport | null;
  pending: number;
  retryable: number;
  rejected: number;
}

export type ActivityStatus =
  "pending" | "submitting" | "accepted" | "retryable" | "rejected";

export interface ActivityTrack {
  source_id: string | null;
  title: string;
  artist: string;
  album: string | null;
  duration_seconds: number | null;
}

export interface ActivityEntry {
  candidate: {
    id: string;
    account_id: string;
    track: ActivityTrack;
    started_at: string;
    timestamp_is_estimated: boolean;
    source_position: number;
    fingerprint: string;
  };
  status: ActivityStatus;
  attempt_count: number;
  next_attempt_at: string;
  last_error_code: string | null;
  created_at: string;
  updated_at: string;
}

export interface ActivityPage {
  items: ActivityEntry[];
  total: number;
  limit: number;
  offset: number;
}

export interface YouTubeMusicAccountInfo {
  account_name: string;
  channel_handle: string | null;
  photo_url: string | null;
}
