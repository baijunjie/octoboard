<script setup lang="ts">
const { locale } = useI18n();
const { t } = useSiteCopy();
const config = useRuntimeConfig();
const route = useRoute();
const head = useLocaleHead({ seo: true });
useSiteSeo();
function directoryUrl(value: unknown) {
  const url = new URL(String(value));
  if (!url.pathname.endsWith("/")) url.pathname += "/";
  return url.href;
}
useHead(() => ({
  htmlAttrs: {
    lang: head.value.htmlAttrs?.lang || locale.value,
    dir: locale.value === "ar" ? "rtl" : "ltr",
  },
  link: (head.value.link || []).map((link) =>
    link.rel === "canonical"
      ? { rel: "canonical" as const, href: directoryUrl(link.href) }
      : {
          rel: "alternate" as const,
          type: "text/html",
          href: directoryUrl(link.href),
          hreflang: String(link.hreflang || ""),
        },
  ),
  // i18n turns zh-Hans into og:locale zh_Hans. Open Graph only accepts
  // language_TERRITORY, so those tags are replaced in useSiteSeo.
  meta: (head.value.meta || [])
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
// One card per route code, drawn by scripts/generate-og-images.py.
// The site build does not redraw them.
const shareImage = () => `${config.public.siteUrl}og-image-${locale.value}.png`;
useSeoMeta({
  ogImage: shareImage,
  ogImageWidth: 1200,
  ogImageHeight: 630,
  // brand-slogan.png carries this English slogan on every language's card.
  ogImageAlt: () =>
    `${config.public.appName} — The Programming Terminator — ${[t("heroTitle"), t("heroTitleSecond"), t("heroTitleAccent")].join(" ")}`,
  ogType: "website",
  ogSiteName: config.public.appName,
  twitterCard: "summary_large_image",
  twitterImage: shareImage,
});
</script>

<template>
  <div id="site-shell">
    <SiteHeader v-if="route.meta.siteChrome !== false" />
    <NuxtPage />
    <SiteFooter v-if="route.meta.siteChrome !== false" />
  </div>
</template>
