// The one bundled dictionary for Settings + overlay. Loaded once from local
// disk; switching language is a pure object lookup (instant). One file per
// language under ./locales/<lang>.ts; this module assembles them into the
// Record<Lang, Dict> every consumer imports.
import type { Lang, Dict } from "./types";
import { ar } from "./locales/ar";
import { bg } from "./locales/bg";
import { cs } from "./locales/cs";
import { da } from "./locales/da";
import { de } from "./locales/de";
import { el } from "./locales/el";
import { en } from "./locales/en";
import { es } from "./locales/es";
import { et } from "./locales/et";
import { fi } from "./locales/fi";
import { fr } from "./locales/fr";
import { he } from "./locales/he";
import { hi } from "./locales/hi";
import { hr } from "./locales/hr";
import { hu } from "./locales/hu";
import { it } from "./locales/it";
import { ja } from "./locales/ja";
import { ko } from "./locales/ko";
import { lt } from "./locales/lt";
import { lv } from "./locales/lv";
import { mt } from "./locales/mt";
import { nb } from "./locales/nb";
import { nl } from "./locales/nl";
import { nn } from "./locales/nn";
import { pl } from "./locales/pl";
import { pt } from "./locales/pt";
import { ro } from "./locales/ro";
import { ru } from "./locales/ru";
import { sk } from "./locales/sk";
import { sl } from "./locales/sl";
import { sv } from "./locales/sv";
import { th } from "./locales/th";
import { tr } from "./locales/tr";
import { uk } from "./locales/uk";
import { vi } from "./locales/vi";
import { zh } from "./locales/zh";

export const locales: Record<Lang, Dict> = {
  ar, bg, cs, da, de, el, en, es, et, fi, fr, he, hi, hr, hu, it, ja, ko, lt, lv, mt, nb, nl, nn, pl, pt, ro, ru, sk, sl, sv, th, tr, uk, vi, zh,
};

export type { Lang } from "./types";
