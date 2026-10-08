<script setup lang="ts">
const { locale } = useI18n();
const config = useRuntimeConfig();
const route = useRoute();
const head = useLocaleHead({ seo: true });
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
  meta: (head.value.meta || []).map((meta) => ({
    property: meta.property ? String(meta.property) : undefined,
    content:
      meta.property === "og:url"
        ? directoryUrl(meta.content)
        : String(meta.content),
  })),
}));
useSeoMeta({
  ogImage: `${config.public.siteUrl}branding/logo.png`,
  ogImageWidth: 640,
  ogImageHeight: 640,
  ogImageAlt: config.public.appName,
  ogType: "website",
  ogSiteName: config.public.appName,
  twitterCard: "summary",
  twitterImage: `${config.public.siteUrl}branding/logo.png`,
});
</script>

<template>
  <div id="site-shell">
    <SiteHeader v-if="route.meta.siteChrome !== false" />
    <NuxtPage />
    <SiteFooter v-if="route.meta.siteChrome !== false" />
  </div>
</template>
