// The word for "Docs" in each language: the website's own (`navDocs` in `apps/web/i18n/locales/<tag>.json`), which
// `webParity.test.ts` holds this table to. The docs keep a copy rather than import the website's JSON into the Astro
// build. `docsTitles` are the whole site titles (`<title>`, `og:site_name`), worded as each language says it, which the
// website has no counterpart for; `{name}` stands for the product name.
import type { Language } from "@octoboard/ui/languages";

export const docsWords: Record<Language, string> = {
  en: "Docs",
  ar: "الوثائق",
  de: "Dokumentation",
  es: "Documentación",
  fr: "Documentation",
  hi: "दस्तावेज़",
  id: "Dokumentasi",
  it: "Documentazione",
  ja: "ドキュメント",
  ko: "문서",
  pt: "Documentação",
  ru: "Документация",
  th: "เอกสาร",
  tr: "Belgeler",
  vi: "Tài liệu",
  "zh-CN": "文档",
  "zh-TW": "說明文件",
};

export const docsTitles: Record<Language, string> = {
  en: "{name} Docs",
  ar: "وثائق {name}",
  de: "{name}-Dokumentation",
  es: "Documentación de {name}",
  fr: "Documentation d’{name}",
  hi: "{name} दस्तावेज़",
  id: "Dokumentasi {name}",
  it: "Documentazione di {name}",
  ja: "{name} ドキュメント",
  ko: "{name} 문서",
  pt: "Documentação do {name}",
  ru: "Документация {name}",
  th: "เอกสาร {name}",
  tr: "{name} Belgeleri",
  vi: "Tài liệu {name}",
  "zh-CN": "{name} 文档",
  "zh-TW": "{name} 說明文件",
};
