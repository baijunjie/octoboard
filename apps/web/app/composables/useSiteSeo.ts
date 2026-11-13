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
  ogLocaleAlternates,
  pageKind,
  pageSeoKeys,
  type ContentPageKind,
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
