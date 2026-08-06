// No i18n library: a property access is the fastest possible lookup.
import type { Lang } from "./locales";
import { locales } from "./locales";

export type { Lang };

type LangEntry = { code: Lang; label: string };

// Endonyms let users find their language in their own script.
export const LANGUAGES: ReadonlyArray<LangEntry> = [
  { code: "ar", label: "العربية" }, { code: "bg", label: "Български" },
  { code: "cs", label: "Čeština" }, { code: "da", label: "Dansk" },
  { code: "de", label: "Deutsch" }, { code: "el", label: "Ελληνικά" },
  { code: "en", label: "English" }, { code: "es", label: "Español" },
  { code: "et", label: "Eesti" }, { code: "fi", label: "Suomi" },
  { code: "fr", label: "Français" }, { code: "he", label: "עברית" },
  { code: "hi", label: "हिन्दी" }, { code: "hr", label: "Hrvatski" },
  { code: "hu", label: "Magyar" }, { code: "it", label: "Italiano" },
  { code: "ja", label: "日本語" }, { code: "ko", label: "한국어" },
  { code: "lt", label: "Lietuvių" }, { code: "lv", label: "Latviešu" },
  { code: "mt", label: "Malti" }, { code: "nb", label: "Bokmål" },
  { code: "nl", label: "Nederlands" }, { code: "nn", label: "Nynorsk" },
  { code: "pl", label: "Polski" }, { code: "pt", label: "Português" },
  { code: "ro", label: "Română" }, { code: "ru", label: "Русский" },
  { code: "sk", label: "Slovenčina" }, { code: "sl", label: "Slovenščina" },
  { code: "sv", label: "Svenska" }, { code: "th", label: "ไทย" },
  { code: "tr", label: "Türkçe" }, { code: "uk", label: "Українська" },
  { code: "vi", label: "Tiếng Việt" }, { code: "zh", label: "中文" },
];

const RTL_LANGS: ReadonlySet<Lang> = new Set<Lang>(["ar", "he"]);

let current: Lang = "en";
export function getCurrentLang(): Lang {
  return current;
}

export function setCurrentLang(code: Lang): void {
  current = code;
  // One-change RTL: flex/grid + logical CSS mirror with dir=rtl.
  const rtl = RTL_LANGS.has(code);
  document.documentElement.dir = rtl ? "rtl" : "ltr";
  document.documentElement.lang = code;
}

// Validate a user-editable settings string (settings.json ui_lang) as a known
// Lang; unknown/missing falls back to "en" (never an invalid cast).
export function asLang(s: string | undefined | null): Lang {
  return s && LANGUAGES.some((l) => l.code === s) ? (s as Lang) : "en";
}

// 3-level fallback: current → English → raw key (visible, never crashes).
export function t(key: string): string {
  return locales[current]?.[key] ?? locales.en[key] ?? key;
}
