<script setup lang="ts">
import { docsPath, pageKind } from "../../seo/site-seo";
const { t } = useSiteCopy();
const { locale } = useI18n();
const config = useRuntimeConfig();
const localePath = useLocalePath();
const route = useRoute();
// The docs are a separate build under the same base path, so they are an ordinary link, not a route.
const docsHref = computed(() => docsPath(config.app.baseURL, locale.value));
const isHome = computed(() => pageKind(route.path) === "home");
const glass = ref<HTMLElement | null>(null);
useLiquidGlass(glass);
useLiquidGlassPointer(glass);
</script>

<template>
  <a class="skip-link" href="#main-content">{{ t("skipContent") }}</a>
  <header class="site-header">
    <div ref="glass" class="nav-pill liquid-glass">
      <NuxtLink :to="localePath('/')" class="brand" :aria-label="config.public.appName">
        <img :src="`${config.app.baseURL}wordmark.png`" alt="" width="113" height="19" />
      </NuxtLink>
      <nav class="desktop-nav" :aria-label="t('navMenu')">
        <NuxtLink :to="`${localePath('/')}#benefits`">{{ t("navFeatures") }}</NuxtLink>
        <NuxtLink :to="`${localePath('/')}#product-preview`">{{ t("previewEyebrow") }}</NuxtLink>
        <a :href="docsHref">{{ t("navDocs") }}</a>
      </nav>
      <div class="nav-actions">
        <SectionNavigation v-if="isHome" />
        <LanguageSelect />
        <NuxtLink :to="localePath('/download/')" class="button nav-cta"><SiteIcon name="download-simple" />{{ t("downloadCta") }}</NuxtLink>
      </div>
    </div>
  </header>
</template>
