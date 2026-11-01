// Locale registration, robots rules, and structured data for the static site.
// Open Graph locale values use language_TERRITORY. The protocol does not accept a
// bare language or a BCP-47 script tag such as zh-Hans, so each language is pinned
// to its primary market.

import {
  LANGUAGES,
  LANGUAGE_NAMES,
  textDirection,
  type Language,
} from "../../../packages/ui/src/i18n/languages.ts";

export type SiteLocale = {
  code: string;
  language: Language;
  file: string;
  name: string;
  ogLocale: string;
  dir?: "rtl";
};

// The languages, their order, names and direction are the application's own list; the site adds
// only its URL code and Open Graph locale. Exhaustive by type, so a new application language
// fails here until the site gives it a code.
const SITE_LOCALES: Record<Language, { code: string; ogLocale: string }> = {
  ar: { code: "ar", ogLocale: "ar_AR" },
  de: { code: "de", ogLocale: "de_DE" },
  en: { code: "en", ogLocale: "en_US" },
  es: { code: "es", ogLocale: "es_ES" },
  fr: { code: "fr", ogLocale: "fr_FR" },
  hi: { code: "hi", ogLocale: "hi_IN" },
  id: { code: "id", ogLocale: "id_ID" },
  it: { code: "it", ogLocale: "it_IT" },
  ja: { code: "ja", ogLocale: "ja_JP" },
  ko: { code: "ko", ogLocale: "ko_KR" },
  "pt-BR": { code: "pt-BR", ogLocale: "pt_BR" },
  ru: { code: "ru", ogLocale: "ru_RU" },
  th: { code: "th", ogLocale: "th_TH" },
  tr: { code: "tr", ogLocale: "tr_TR" },
  vi: { code: "vi", ogLocale: "vi_VN" },
  "zh-Hans": { code: "zh", ogLocale: "zh_CN" },
  "zh-Hant": { code: "zh-Hant", ogLocale: "zh_TW" },
};

export const locales: readonly SiteLocale[] = LANGUAGES.map((language) => ({
  ...SITE_LOCALES[language],
  language,
  file: `${language}.json`,
  name: LANGUAGE_NAMES[language],
  ...(textDirection(language) === "rtl" ? { dir: "rtl" as const } : {}),
}));

export const pagePaths = ["", "privacy/"] as const;
export type PageKind = "home" | "privacy" | "other";

export function localeByCode(code: string): SiteLocale {
  const locale = locales.find((item) => item.code === code);
  if (!locale) throw new Error(`Unknown locale: ${code}`);
  return locale;
}

function knownLocaleCode(path: string): string {
  const code = path.match(/^\/([^/]+)(?:\/|$)/)?.[1];
  if (!code || !locales.some((locale) => locale.code === code)) return "";
  return code;
}

export function localizedPath(code: string, path: string): string {
  return `${code === "en" ? "" : `${code}/`}${path}`;
}

export function localeCodeFromPath(path: string): string {
  return knownLocaleCode(path) || "en";
}

export function pageKind(path: string): PageKind {
  const normalized = path.endsWith("/") ? path : `${path}/`;
  const code = knownLocaleCode(normalized);
  const rest = code ? normalized.slice(code.length + 1) : normalized;
  if (rest === "/privacy/") return "privacy";
  if (rest === "/") return "home";
  return "other";
}

export function canonicalUrl(siteUrl: string, routePath: string): string {
  const normalized = routePath.endsWith("/") ? routePath : `${routePath}/`;
  return siteUrl + normalized.slice(1);
}

export function languageHomeUrl(siteUrl: string, code: string): string {
  return siteUrl + localizedPath(code, "");
}

export function markdownFile(routePath: string): string {
  const kind = pageKind(routePath);
  if (kind === "other")
    throw new Error(`No Markdown version for ${routePath}`);
  const htmlPath = localizedPath(
    localeCodeFromPath(routePath),
    kind === "privacy" ? "privacy/" : "",
  );
  const trimmed = htmlPath.replace(/\/$/, "");
  return trimmed === "" ? "index.md" : `${trimmed}.md`;
}

export function markdownUrl(siteUrl: string, routePath: string): string {
  return siteUrl + markdownFile(routePath);
}

export function ogLocaleAlternates(code: string): string[] {
  const current = localeByCode(code).ogLocale;
  return locales
    .map((locale) => locale.ogLocale)
    .filter((value) => value !== current);
}

// Search indexing, live AI retrieval, and model training are all allowed.
// Public pages are product information and a privacy policy. Facts that can
// go stale are corrected when an agent reads the current page.
// GitHub Pages cannot send X-Robots-Tag or Vary, so a Markdown copy cannot
// be marked noindex on the response. The copy stays allowed: Disallow would
// hide it from agents that honor this file, and it may be indexed beside
// its HTML page.
// Content-Signal: https://contentsignals.org/
// Content-Usage: IETF AIPREF vocabulary (train-ai).
export function robotsTxt(siteUrl: string): string {
  return [
    "User-agent: *",
    "Allow: /",
    "Content-Usage: train-ai=y",
    "Content-Signal: search=yes, ai-input=yes, ai-train=yes",
    "",
    `Sitemap: ${siteUrl}sitemap.xml`,
    "",
  ].join("\n");
}

export type JsonLdQuestion = { question: string; answer: string };

export type JsonLdInput = {
  page: "home" | "privacy";
  canonical: string;
  websiteUrl: string;
  language: string;
  title: string;
  description: string;
  siteName: string;
  siteDescription: string;
  origin: string;
  siteUrl: string;
  operatorName: string;
  contactEmail: string;
  repositoryUrl: string;
  logoUrl: string;
  licenseUrl: string;
  languages: string[];
  featureTitles: string[];
  questions: JsonLdQuestion[];
};

export function buildJsonLd(input: JsonLdInput): Record<string, unknown> {
  const identity = `${input.origin}/#identity`;
  const website = `${input.websiteUrl}#website`;
  const webpage = `${input.canonical}#webpage`;
  const app = `${input.origin}/#app`;
  const page: Record<string, unknown> = {
    "@type": input.page === "home" ? "FAQPage" : "WebPage",
    "@id": webpage,
    url: input.canonical,
    name: input.title,
    description: input.description,
    inLanguage: input.language,
    isPartOf: { "@id": website },
  };
  const graph: Record<string, unknown>[] = [
    {
      "@type": "Person",
      "@id": identity,
      name: input.operatorName,
      email: input.contactEmail,
      url: input.siteUrl,
    },
    {
      "@type": "WebSite",
      "@id": website,
      url: input.websiteUrl,
      name: input.siteName,
      description: input.siteDescription,
      inLanguage: input.language,
      publisher: { "@id": identity },
    },
    page,
  ];
  // The installer is not published. An Offer, installUrl, or rating would be
  // read as "you can download it now", and there is no real rating to report.
  // FAQ rich results are marked on the homepage only; the privacy page repeats
  // none of these questions.
  if (input.page === "home") {
    page.about = { "@id": app };
    page.mainEntity = input.questions.map((item) => ({
      "@type": "Question",
      name: item.question,
      acceptedAnswer: { "@type": "Answer", text: item.answer },
    }));
    // Feature names are translated per page. They stay on this page so the
    // shared application id below does not carry 17 conflicting bodies.
    page.hasPart = {
      "@type": "ItemList",
      itemListElement: input.featureTitles.map((name, index) => ({
        "@type": "ListItem",
        position: index + 1,
        name,
      })),
    };
    graph.push({
      "@type": "SoftwareApplication",
      "@id": app,
      name: input.siteName,
      url: input.siteUrl,
      image: input.logoUrl,
      applicationCategory: "DeveloperApplication",
      operatingSystem: "macOS",
      inLanguage: input.languages,
      author: { "@id": identity },
      publisher: { "@id": identity },
      sameAs: input.repositoryUrl,
      license: input.licenseUrl,
    });
  }
  return { "@context": "https://schema.org", "@graph": graph };
}
