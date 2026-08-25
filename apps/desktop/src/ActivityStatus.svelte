<script lang="ts">
  import type { ActivityStatus, Locale } from "@scrobble-bridge/ui";
  import {
    ArrowsClockwise,
    CheckCircle,
    Clock,
    WarningCircle,
  } from "phosphor-svelte";

  let { status, locale }: { status: ActivityStatus; locale: Locale } = $props();

  const LABELS = {
    en: {
      accepted: "Synced to Last.fm",
      pending: "Waiting",
      submitting: "Syncing",
      retryable: "Retry scheduled",
      rejected: "Needs attention",
    },
    "zh-CN": {
      accepted: "已同步到 Last.fm",
      pending: "等待同步",
      submitting: "正在同步",
      retryable: "等待重试",
      rejected: "需要处理",
    },
  } as const;

  let label = $derived(LABELS[locale][status]);
</script>

<span class:success={status === "accepted"} class:warning={status === "pending" || status === "submitting" || status === "retryable"} class:danger={status === "rejected"} class="activity-status">
  {#if status === "accepted"}
    <CheckCircle size={18} weight="fill" aria-hidden="true" />
  {:else if status === "submitting"}
    <ArrowsClockwise size={18} aria-hidden="true" />
  {:else if status === "rejected"}
    <WarningCircle size={18} weight="fill" aria-hidden="true" />
  {:else}
    <Clock size={18} aria-hidden="true" />
  {/if}
  <span>{label}</span>
</span>

<style>
  .activity-status {
    align-items: center;
    color: var(--muted);
    display: inline-flex;
    font-size: 0.82rem;
    font-weight: 650;
    gap: 7px;
    white-space: nowrap;
  }
  .success {
    color: var(--success);
  }
  .warning {
    color: var(--warning-text);
  }
  .danger {
    color: var(--danger);
  }
</style>
