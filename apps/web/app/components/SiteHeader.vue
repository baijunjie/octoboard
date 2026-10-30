<script setup lang="ts">
import { pageKind } from "../../seo/site-seo";
const { t } = useSiteCopy();
const config = useRuntimeConfig();
const localePath = useLocalePath();
const route = useRoute();
const isHome = computed(() => pageKind(route.path) === "home");
const glass = ref<HTMLElement | null>(null);
useLiquidGlass(glass);
useLiquidGlassPointer(glass);
const releasesUrl = `${config.public.repositoryUrl}/releases`;
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
      </nav>
      <div class="nav-actions">
        <SectionNavigation v-if="isHome" />
        <LanguageSelect />
        <a :href="releasesUrl" class="button nav-cta"><SiteIcon name="download-simple" />{{ t("heroCta") }}</a>
      </div>
    </div>
  </header>
</template>
