<script setup lang="ts">
import { remoteGuidePage } from "../../seo/page-outline";
import { docsPath } from "../../seo/site-seo";

const { t } = useSiteCopy();
const { locale } = useI18n();
const config = useRuntimeConfig();
const guideHref = computed(() => docsPath(config.app.baseURL, locale.value, remoteGuidePage));
</script>

<template>
  <section id="remote-connections" class="wrap section-space" aria-labelledby="remote-title">
    <SectionHeading
      heading-id="remote-title"
      number="04"
      :label="t('remoteEyebrow')"
      :title="t('remoteTitle')"
      :description="t('remoteIntro')"
    />
    <RemoteRoute />
    <a class="text-link remote-guide-link" :href="guideHref">
      {{ t("remoteGuideLink") }}<span class="link-arrow" aria-hidden="true" />
    </a>
  </section>
</template>

<style scoped>
.remote-guide-link { margin-top: 24px; }
</style>
