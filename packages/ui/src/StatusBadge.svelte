<script lang="ts">
  import type { RuntimePhase } from "./index";
  import type { Locale } from "./i18n";
  import { phaseLabel } from "./status";

  let { phase, locale = "en" }: { phase: RuntimePhase; locale?: Locale } =
    $props();
</script>

<span
  class:attention={phase === "needs_attention"}
  class:waiting={phase === "retry_waiting"}
  class:quiet={phase === "paused" || phase === "needs_setup"}
>
  <i></i>{phaseLabel(phase, locale)}
</span>

<style>
  span {
    align-items: center;
    background: color-mix(in srgb, var(--success) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--success) 28%, transparent);
    border-radius: 999px;
    color: var(--success);
    display: inline-flex;
    font-size: 0.76rem;
    font-weight: 700;
    gap: 0.45rem;
    letter-spacing: 0.035em;
    padding: 0.42rem 0.7rem;
    text-transform: uppercase;
  }

  i {
    background: currentColor;
    border-radius: 50%;
    height: 0.48rem;
    width: 0.48rem;
  }

  .attention {
    background: color-mix(in srgb, var(--danger) 10%, transparent);
    border-color: color-mix(in srgb, var(--danger) 28%, transparent);
    color: var(--danger);
  }

  .waiting {
    background: color-mix(in srgb, var(--warning) 12%, transparent);
    border-color: color-mix(in srgb, var(--warning) 28%, transparent);
    color: var(--warning-text);
  }

  .quiet {
    background: color-mix(in srgb, var(--muted) 12%, transparent);
    border-color: color-mix(in srgb, var(--muted) 25%, transparent);
    color: var(--muted);
  }
</style>
