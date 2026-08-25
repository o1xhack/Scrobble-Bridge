<script lang="ts">
  import { dateLocale, type ActivityEntry, type Locale } from "@scrobble-bridge/ui";
  import { CaretRight, MusicNotesSimple } from "phosphor-svelte";
  import ActivityStatus from "./ActivityStatus.svelte";

  let {
    entries,
    locale,
    onSelect,
  }: {
    entries: ActivityEntry[];
    locale: Locale;
    onSelect: (entry: ActivityEntry) => void;
  } = $props();

  const COPY = {
    en: { time: "Time", song: "Song", source: "Source", result: "Result", today: "Today", yesterday: "Yesterday" },
    "zh-CN": { time: "时间", song: "歌曲", source: "来源", result: "同步结果", today: "今天", yesterday: "昨天" },
  } as const;

  let text = $derived(COPY[locale]);

  function dateKey(value: string) {
    const date = new Date(value);
    return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
  }

  function dateLabel(value: string) {
    const date = new Date(value);
    const today = new Date();
    const yesterday = new Date(today);
    yesterday.setDate(today.getDate() - 1);
    if (dateKey(value) === dateKey(today.toISOString())) return text.today;
    if (dateKey(value) === dateKey(yesterday.toISOString())) return text.yesterday;
    return new Intl.DateTimeFormat(dateLocale(locale), {
      month: "short",
      day: "numeric",
      year: date.getFullYear() === today.getFullYear() ? undefined : "numeric",
    }).format(date);
  }

  function timeLabel(value: string) {
    return new Intl.DateTimeFormat(dateLocale(locale), {
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(value));
  }

  function thumbnail(entry: ActivityEntry) {
    const id = entry.candidate.track.source_id;
    return id ? `https://i.ytimg.com/vi/${encodeURIComponent(id)}/mqdefault.jpg` : null;
  }
</script>

<div class="activity-list" data-testid="activity-list">
  <div class="activity-header" aria-hidden="true">
    <span>{text.time}</span><span>{text.song}</span><span>{text.source}</span><span>{text.result}</span><span></span>
  </div>
  {#each entries as entry, index (entry.candidate.id)}
    {#if index === 0 || dateKey(entries[index - 1].candidate.started_at) !== dateKey(entry.candidate.started_at)}
      <div class="date-divider">{dateLabel(entry.candidate.started_at)}</div>
    {/if}
    <button class="activity-row" type="button" onclick={() => onSelect(entry)} aria-label={`${entry.candidate.track.title} — ${entry.candidate.track.artist}`}>
      <time datetime={entry.candidate.started_at}>{timeLabel(entry.candidate.started_at)}</time>
      <span class="track-cell">
        <span class="artwork">
          <MusicNotesSimple size={20} weight="duotone" aria-hidden="true" />
          {#if thumbnail(entry)}
            <img src={thumbnail(entry) ?? ""} alt="" loading="lazy" onerror={(event) => event.currentTarget.remove()} />
          {/if}
        </span>
        <span class="track-copy"><strong>{entry.candidate.track.title}</strong><small>{entry.candidate.track.artist}</small></span>
      </span>
      <span class="source">YouTube Music</span>
      <ActivityStatus status={entry.status} {locale} />
      <CaretRight size={18} aria-hidden="true" />
    </button>
  {/each}
</div>

<style>
  .activity-list {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 12px;
    min-width: 0;
    overflow: hidden;
  }
  .activity-header,
  .activity-row {
    align-items: center;
    display: grid;
    grid-template-columns: 112px minmax(270px, 1.3fr) minmax(150px, 0.75fr) minmax(175px, 0.85fr) 24px;
  }
  .activity-header {
    background: #fbfcfb;
    border-bottom: 1px solid var(--line);
    color: var(--muted);
    font-size: 0.75rem;
    font-weight: 650;
    min-height: 38px;
    padding: 0 20px;
  }
  .date-divider {
    background: #fbfcfb;
    border-bottom: 1px solid var(--line);
    color: var(--muted);
    font-size: 0.76rem;
    font-weight: 700;
    padding: 7px 20px;
  }
  .activity-row {
    background: var(--surface);
    border: 0;
    border-bottom: 1px solid var(--line);
    color: var(--ink);
    font: inherit;
    min-height: 58px;
    padding: 6px 20px;
    text-align: left;
    transition: background 120ms ease;
    width: 100%;
  }
  .activity-row:last-child {
    border-bottom: 0;
  }
  .activity-row:hover {
    background: #f7faf8;
  }
  .activity-row:focus-visible {
    box-shadow: inset 0 0 0 3px color-mix(in srgb, var(--accent) 24%, transparent);
    outline: 0;
  }
  time {
    color: var(--ink);
    font-size: 0.9rem;
    font-variant-numeric: tabular-nums;
    font-weight: 650;
  }
  .track-cell {
    align-items: center;
    display: flex;
    gap: 12px;
    min-width: 0;
  }
  .artwork {
    align-items: center;
    background: #edf2ef;
    border-radius: 8px;
    color: var(--accent);
    display: flex;
    flex: 0 0 auto;
    height: 42px;
    justify-content: center;
    overflow: hidden;
    position: relative;
    width: 42px;
  }
  .artwork img {
    height: 100%;
    object-fit: cover;
    position: absolute;
    width: 100%;
  }
  .track-copy {
    display: grid;
    gap: 3px;
    min-width: 0;
  }
  .track-copy strong,
  .track-copy small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .track-copy strong {
    font-size: 0.92rem;
    font-weight: 700;
  }
  .track-copy small,
  .source {
    color: var(--muted);
    font-size: 0.8rem;
  }
  @media (max-width: 820px) {
    .activity-header,
    .activity-row {
      grid-template-columns: 76px minmax(220px, 1fr) minmax(145px, auto) 22px;
    }
    .activity-header span:nth-child(3),
    .activity-row .source {
      display: none;
    }
  }
  @media (max-width: 650px) {
    .activity-header {
      display: none;
    }
    .activity-row {
      gap: 8px;
      grid-template-columns: 56px minmax(0, 1fr) 22px;
      padding: 10px 14px;
    }
    .activity-row :global(.activity-status) {
      grid-column: 2;
    }
    .activity-row > :global(svg:last-child) {
      grid-column: 3;
      grid-row: 1 / span 2;
    }
  }
</style>
