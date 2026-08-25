import type { RuntimePhase } from "./index";
import type { Locale } from "./i18n";

export const PHASE_LABELS: Record<RuntimePhase, string> = {
  needs_setup: "Needs setup",
  idle: "All caught up",
  syncing: "Syncing",
  paused: "Paused",
  retry_waiting: "Retry scheduled",
  needs_attention: "Needs attention",
};

export const PHASE_LABELS_ZH_CN: Record<RuntimePhase, string> = {
  needs_setup: "需要设置",
  idle: "已全部同步",
  syncing: "正在同步",
  paused: "已暂停",
  retry_waiting: "等待重试",
  needs_attention: "需要处理",
};

export function phaseLabel(phase: RuntimePhase, locale: Locale): string {
  return locale === "zh-CN" ? PHASE_LABELS_ZH_CN[phase] : PHASE_LABELS[phase];
}
