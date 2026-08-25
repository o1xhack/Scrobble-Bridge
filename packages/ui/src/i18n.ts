export type Locale = "en" | "zh-CN";

export const LOCALE_STORAGE_KEY = "scrobble-bridge-locale";

export function resolveLocale(
  preferred?: string | null,
  systemLanguage = "en",
): Locale {
  const candidate = preferred || systemLanguage;
  return candidate.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}

export function dateLocale(locale: Locale): string {
  return locale === "zh-CN" ? "zh-CN" : "en";
}
