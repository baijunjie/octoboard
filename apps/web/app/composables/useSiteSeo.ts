import { defineLink } from "@unhead/vue";
import {
  homeFeatures,
  homeQuestions,
} from "../../seo/page-outline";
import {
  buildJsonLd,
  canonicalUrl,
  languageHomeUrl,
  localeByCode,
  localeCodeFromPath,
  locales,
  markdownUrl,
  ogLocaleAlternates,
  pageKind,
} from "../../seo/site-seo";

export function useSiteSeo() {
  const route = useRoute();
  const config = useRuntimeConfig();
  const { t } = useSiteCopy();
  const siteUrl = config.public.siteUrl;
  const kind = computed(() => pageKind(route.path));
  const code = computed(() => localeCodeFromPath(route.path));
  const locale = computed(() => localeByCode(code.value));
  const canonical = computed(() => canonicalUrl(siteUrl, route.path));
  const title = computed(() =>
    t(kind.value === "privacy" ? "privacySeoTitle" : "seoTitle"),
  );
  const description = computed(() =>
    t(kind.value === "privacy" ? "privacySeoDescription" : "seoDescription"),
  );
  const siteDescription = computed(() => t("seoDescription"));

  useHead(() => {
    const contentPage = kind.value === "home" || kind.value === "privacy";
    const graph = contentPage
      ? JSON.stringify(
          buildJsonLd({
            page: kind.value === "privacy" ? "privacy" : "home",
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
            languages: locales.map((item) => item.language),
            featureTitles: homeFeatures.map((feature) =>
              t(`feature${feature.key}Title`),
            ),
            questions: homeQuestions.map((question) => ({
              question: t(`faq${question}Question`),
              answer: t(`faq${question}Answer`),
            })),
          }),
        ).replaceAll("<", "\\u003c")
      : "";
    return {
      meta: [
        { property: "og:locale", content: locale.value.ogLocale },
        ...ogLocaleAlternates(code.value).map((content) => ({
          property: "og:locale:alternate",
          content,
        })),
      ],
      link: contentPage
        ? [
            {
              rel: "alternate",
              type: "text/markdown",
              href: markdownUrl(siteUrl, route.path),
            },
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
}
