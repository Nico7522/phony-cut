import { installerName, links, type Page } from "../config";
import { en, type Dict } from "./en";
import { fr } from "./fr";

export type Lang = "en" | "fr";
export const langs: readonly Lang[] = ["en", "fr"];

const dicts: Record<Lang, Dict> = { en, fr };

/** All the visible text of a page, in the given language. */
export function t(lang: Lang): Dict {
  return dicts[lang];
}

/** `/phony-cut` (no trailing slash), from Astro's `base`. */
const base = import.meta.env.BASE_URL.replace(/\/$/, "");

/** URL of a page in a language, respecting the base path: `/phony-cut/fr/faq/`. */
export function pageUrl(lang: Lang, page: Page, hash = ""): string {
  const prefix = lang === "en" ? "" : `/${lang}`;
  const path = page === "" ? "/" : `/${page}/`;
  return `${base}${prefix}${path}${hash ? `#${hash}` : ""}`;
}

/** URL of a file in `public/`, respecting the base path: `/phony-cut/shared/logo.svg`. */
export function assetUrl(path: string): string {
  return `${base}/${path.replace(/^\//, "")}`;
}

export function otherLang(lang: Lang): Lang {
  return lang === "en" ? "fr" : "en";
}

/**
 * Replaces the `{name}` placeholders of a text (links, file names) so that the
 * dictionaries never contain URLs. Unknown placeholders are an error.
 */
export function fill(lang: Lang, text: string): string {
  const values: Record<string, string> = {
    checksums: links.checksums,
    installer: installerName,
    issues: links.issues,
    repo: links.repo,
    scrcpy: links.scrcpy,
    support: pageUrl(lang, "support"),
  };
  return text.replace(/\{(\w+)\}/g, (_, name: string) => {
    const value = values[name];
    if (value === undefined) throw new Error(`Unknown placeholder {${name}} in "${text}"`);
    return value;
  });
}
