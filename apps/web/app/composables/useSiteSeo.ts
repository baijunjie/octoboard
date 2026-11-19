import { defineLink } from "@unhead/vue";
import { downloadPagePath, downloadTargets } from "../../seo/downloads";
import {
  homeFeatures,
  homeQuestions,
  homeQuestionAnswerKey,
} from "../../seo/page-outline";
import {
  buildJsonLd,
  canonicalUrl,
  languageHomeUrl,
  localeByCode,
  localeCodeFromPath,
  locales,
  markdownUrl,
  isIndexedPage,
  ogLocaleAlternates,
  pageKind,
  pageSeoKeys,
  type ContentPageKind,
} from "../../seo/site-seo";

function directoryUrl(value: unknown) {
  const url = new URL(String(value));
  if (!url.pathname.endsWith("/")) url.pathname += "/";
  return url.href;
}

export function useSiteSeo() {
  const { locale: languageCode } = useI18n();
  const localeHead = useLocaleHead({ seo: true });
  const route = useRoute();
  const config = useRuntimeConfig();
  const { t } = useSiteCopy();
  const siteUrl = config.public.siteUrl;
  const kind = computed(() => pageKind(route.path));
  const code = computed(() => localeCodeFromPath(route.path));
  const locale = computed(() => localeByCode(code.value));
  const indexed = computed(() => isIndexedPage(kind.value));
  const canonical = computed(() => canonicalUrl(siteUrl, route.path));
  const title = computed(() =>
    t(pageSeoKeys[kind.value as ContentPageKind]?.title ?? pageSeoKeys.home.title),
  );
  const description = computed(() =>
    t(pageSeoKeys[kind.value as ContentPageKind]?.description ?? pageSeoKeys.home.description),
  );
  const siteDescription = computed(() => t("workspaceSeoSummary"));

  useHead(() => {
    const contentPage = kind.value !== "other";
    const graph = contentPage
      ? JSON.stringify(
        buildJsonLd({
          page: kind.value as Exclude<typeof kind.value, "other">,
          canonical: canonical.value,
          websiteUrl: languageHomeUrl(siteUrl, code.value),
          language: locale.value.language,
          title: title.value,
          description: description.value,
          siteName: config.public.appName,
          siteDescription: siteDescription.value,
          origin: config.public.siteOrigin,
          siteUrl,
          operatorName: config.public.operatorName,
          contactEmail: config.public.contactEmail,
          repositoryUrl: config.public.repositoryUrl,
          logoUrl: `${siteUrl}logo.png`,
          licenseUrl: `${config.public.repositoryUrl}/blob/main/LICENSE`,
          downloadUrls: downloadTargets.map(
            ({ id }) => `${siteUrl}${downloadPagePath(id)}`,
          ),
          languages: locales.map((item) => item.language),
          featureTitles: homeFeatures.map((feature) =>
            t(`${feature.key}FeatureTitle`).replaceAll("\n", " "),
          ),
          questions: homeQuestions.map((question) => ({
            question: t(`faq${question}Question`),
            answer: t(homeQuestionAnswerKey(question)),
          })),
        }),
      ).replaceAll("<", "\\u003c")
      : "";
    return {
      meta: [
        ...(indexed.value ? [] : [{ name: "robots", content: "noindex, follow" }]),
        { property: "og:locale", content: locale.value.ogLocale },
        ...ogLocaleAlternates(code.value).map((content) => ({
          property: "og:locale:alternate",
          content,
        })),
      ],
      link: contentPage
        ? [
          // A page that asks not to be indexed does not point search engines at its Markdown copy.
          ...(indexed.value
            ? [defineLink({ rel: "alternate", type: "text/markdown", href: markdownUrl(siteUrl, route.path) })]
            : []),
          defineLink({ rel: "describedby", href: `${siteUrl}llms.txt` }),
        ]
        : [],
      script: contentPage
        ? [
          {
            key: "site-schema",
            type: "application/ld+json",
            innerHTML: graph,
          },
        ]
        : [],
    };
  });

  useHead(() => ({
    htmlAttrs: {
      lang: localeHead.value.htmlAttrs?.lang || languageCode.value,
      dir: languageCode.value === "ar" ? "rtl" : "ltr",
    },
    // A page that asks not to be indexed has no URL to prefer, so it names no canonical and no language alternates.
    link: (indexed.value ? localeHead.value.link || [] : []).map((link) =>
      link.rel === "canonical"
        ? { rel: "canonical" as const, href: directoryUrl(link.href) }
        : {
          rel: "alternate" as const,
          type: "text/html",
          href: directoryUrl(link.href),
          hreflang: String(link.hreflang || ""),
        },
    ),
    // Use the locale tags above: they carry territories where i18n would use a script code.
    meta: (localeHead.value.meta || [])
      .filter(
        (meta) =>
          meta.property !== "og:locale" &&
          meta.property !== "og:locale:alternate",
      )
      .map((meta) => ({
        property: meta.property ? String(meta.property) : undefined,
        content:
          meta.property === "og:url"
            ? directoryUrl(meta.content)
            : String(meta.content),
      })),
  }));
  // One card per locale code, drawn by scripts/generate-og-images.py.
  // The site build does not redraw them.
  const shareImage = () => `${config.public.siteUrl}og-image-${languageCode.value}.png`;
  useSeoMeta({
    ogImage: shareImage,
    ogImageWidth: 1200,
    ogImageHeight: 630,
    ogImageAlt: () =>
      `${config.public.appName} — ${t("brandSlogan")} — ${t("brandSloganMeaning")} — ${[t("workspaceTitle"), t("workspaceTitleSecond"), t("workspaceTitleAccent")].join(" ")}`,
    ogType: "website",
    ogSiteName: config.public.appName,
    twitterCard: "summary_large_image",
    twitterImage: shareImage,
  });
}
