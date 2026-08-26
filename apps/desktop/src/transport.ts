import { invoke } from "@tauri-apps/api/core";
import type {
  ActivityEntry,
  ActivityPage,
  ActivityStatus,
  RuntimeStatus,
  SyncReport,
  YouTubeMusicAccountInfo,
} from "@scrobble-bridge/ui";

const demoParameters =
  import.meta.env.DEV && typeof window !== "undefined"
    ? new URLSearchParams(window.location.search)
    : null;
const demoEnabled = demoParameters?.has("demo") ?? false;
const demoUpdateEnabled = demoEnabled && demoParameters?.has("update");

export const isDemoMode = demoEnabled;

export type SoftwareUpdatePhase =
  "idle" | "checking" | "available" | "downloading" | "ready" | "installing";

export interface SoftwareUpdateStatus {
  current_version: string;
  phase: SoftwareUpdatePhase;
  available_version: string | null;
  notes: string | null;
  published_at: string | null;
  last_checked_at: string | null;
  last_successful_check_at: string | null;
  next_check_at: string | null;
  downloaded_bytes: number;
  total_bytes: number | null;
  error: string | null;
}

const DEMO_TRACKS = [
  ["Sunset Drive", "Mila Orange", "dX3k_QDnzHE"],
  ["夜に駆ける", "YOASOBI", "x8VYWazR5mE"],
  ["Sparks", "Coldplay", "Ar48yzjn1PE"],
  ["风景早见", "风景、陈建鑫", "mrZRURcb1cM"],
  ["Plastic Love", "Mariya Takeuchi", "T_lC2O1oIew"],
  ["A Real Hero", "College & Electric Youth", "-DSVDcw6iW8"],
] as const;

const DEMO_ACTIVITY: ActivityEntry[] = Array.from(
  { length: 137 },
  (_, index) => {
    const [title, artist, sourceId] = DEMO_TRACKS[index % DEMO_TRACKS.length];
    const started = new Date();
    if (index < 3) {
      started.setHours(15, 31 - index * 4, 0, 0);
    } else if (index < 6) {
      started.setDate(started.getDate() - 1);
      started.setHours(22, 47 - (index - 3) * 16, 0, 0);
    } else {
      started.setTime(Date.now() - (index - 4) * 6 * 60 * 60_000);
    }
    const targetStatuses: ActivityStatus[] = [
      "accepted",
      "accepted",
      "pending",
      "accepted",
      "rejected",
      "pending",
    ];
    const activityStatus: ActivityStatus =
      targetStatuses[index] ?? (index % 13 === 0 ? "retryable" : "accepted");
    return {
      candidate: {
        id: `demo-${index}`,
        account_id: "demo",
        track: {
          source_id: sourceId,
          title,
          artist,
          album: null,
          duration_seconds: 240,
        },
        started_at: started.toISOString(),
        timestamp_is_estimated: false,
        source_position: index,
        fingerprint: `demo-fingerprint-${index}`,
      },
      status: activityStatus,
      attempt_count:
        activityStatus === "retryable"
          ? 2
          : activityStatus === "pending"
            ? 0
            : 1,
      next_attempt_at: new Date(started.getTime() + 30 * 60_000).toISOString(),
      last_error_code:
        activityStatus === "retryable"
          ? "temporary_unavailable"
          : activityStatus === "rejected"
            ? "invalid_track"
            : null,
      created_at: new Date(started.getTime() + 60_000).toISOString(),
      updated_at: new Date(started.getTime() + 90_000).toISOString(),
    };
  },
);

const DEMO_STATUS: RuntimeStatus = {
  phase: "idle",
  configured: true,
  ytmusic_configured: true,
  ytmusic_account_name: "MS-113",
  ytmusic_channel_handle: "@MS-113",
  lastfm_application_configured: true,
  lastfm_authorized: true,
  lastfm_username: "demo_listener",
  paused: false,
  last_attempt_at: new Date(Date.now() - 4 * 60_000).toISOString(),
  last_success_at: new Date(Date.now() - 4 * 60_000).toISOString(),
  next_scheduled_at: new Date(Date.now() + 26 * 60_000).toISOString(),
  last_error_code: null,
  last_error_message: null,
  last_report: null,
  pending: 2,
  retryable: 1,
  rejected: 1,
};

const DEMO_UPDATE_STATUS: SoftwareUpdateStatus = {
  current_version: "1.0.0",
  phase: demoUpdateEnabled ? "available" : "idle",
  available_version: demoUpdateEnabled ? "1.0.1" : null,
  notes: demoUpdateEnabled
    ? "Improved sleep/wake recovery and update reliability.\nFixed a credential refresh edge case."
    : null,
  published_at: demoUpdateEnabled ? new Date().toISOString() : null,
  last_checked_at: new Date().toISOString(),
  last_successful_check_at: new Date().toISOString(),
  next_check_at: new Date(Date.now() + 24 * 60 * 60_000).toISOString(),
  downloaded_bytes: 0,
  total_bytes: null,
  error: null,
};

function demoActivity(args: {
  limit: number;
  offset: number;
  search: string | null;
  status: ActivityStatus | null;
}): ActivityPage {
  const needle = args.search?.toLocaleLowerCase() ?? "";
  const items = DEMO_ACTIVITY.filter((entry) => {
    const matchesText =
      !needle ||
      entry.candidate.track.title.toLocaleLowerCase().includes(needle) ||
      entry.candidate.track.artist.toLocaleLowerCase().includes(needle);
    return matchesText && (!args.status || entry.status === args.status);
  });
  return {
    items: items.slice(args.offset, args.offset + args.limit),
    total: items.length,
    limit: args.limit,
    offset: args.offset,
  };
}

function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  if (!demoEnabled) return args ? invoke<T>(name, args) : invoke<T>(name);
  if (name === "status") return Promise.resolve(DEMO_STATUS as T);
  if (
    name === "software_update_status" ||
    name === "check_for_software_update" ||
    name === "download_software_update"
  )
    return Promise.resolve(DEMO_UPDATE_STATUS as T);
  if (name === "activity")
    return Promise.resolve(
      demoActivity(args as Parameters<typeof demoActivity>[0]) as T,
    );
  if (name === "refresh_ytmusic_identity")
    return Promise.resolve({
      account_name: "MS-113",
      channel_handle: "@MS-113",
      photo_url: null,
    } as T);
  if (name === "start_lastfm_authorization")
    return Promise.resolve("https://www.last.fm/api/auth/" as T);
  if (name === "export_diagnostics")
    return Promise.resolve("~/Desktop/scrobble-bridge-diagnostics.zip" as T);
  if (name === "sync_now")
    return Promise.resolve({
      source_outcome: "delta",
      overlap_matches: 20,
      discovered: 3,
      enqueued: 3,
      matched_existing: 0,
      submitted: 3,
      accepted: 3,
      retryable: 0,
      rejected: 0,
      gap_best_overlap: null,
    } as T);
  return Promise.resolve(undefined as T);
}

export const desktop = {
  status: () => command<RuntimeStatus>("status"),
  activity: (
    limit = 50,
    offset = 0,
    search?: string,
    status?: ActivityStatus,
  ) =>
    command<ActivityPage>("activity", {
      limit,
      offset,
      search: search || null,
      status: status || null,
    }),
  sync: () => command<SyncReport>("sync_now"),
  pause: () => command<void>("pause_sync"),
  resume: () => command<void>("resume_sync"),
  saveYouTubeMusic: (
    accountId: string,
    authUser: number,
    cookieHeader: string,
    delegatedSessionId: string | null = null,
  ) =>
    command<void>("save_ytmusic_credentials", {
      accountId,
      authUser,
      delegatedSessionId,
      cookieHeader,
    }),
  saveLastFmApplication: (apiKey: string, sharedSecret: string) =>
    command<void>("save_lastfm_application", { apiKey, sharedSecret }),
  refreshYouTubeMusicIdentity: () =>
    command<YouTubeMusicAccountInfo>("refresh_ytmusic_identity"),
  startLastFm: () => command<string>("start_lastfm_authorization"),
  finishLastFm: () => command<void>("finish_lastfm_authorization"),
  exportDiagnostics: () => command<string>("export_diagnostics"),
  updateStatus: () => command<SoftwareUpdateStatus>("software_update_status"),
  checkForUpdate: () =>
    command<SoftwareUpdateStatus>("check_for_software_update"),
  downloadUpdate: () =>
    command<SoftwareUpdateStatus>("download_software_update"),
  installUpdate: () => command<void>("install_software_update"),
};
