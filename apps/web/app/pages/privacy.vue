<script setup lang="ts">
import { githubPrivacyUrl, privacySections } from "../../seo/page-outline";

const { t } = useSiteCopy();
const config = useRuntimeConfig();
const localePath = useLocalePath();
useSeoMeta({
  title: () => t("privacySeoTitle"),
  description: () => t("privacySeoDescription"),
  ogTitle: () => t("privacySeoTitle"),
  ogDescription: () => t("privacySeoDescription"),
});
</script>

<template>
  <main id="main-content" class="privacy-page wrap">
    <NuxtLink class="text-link privacy-back" :to="localePath('/')"
      ><span aria-hidden="true">←</span> {{ t("privacyBack") }}</NuxtLink
    >
    <header class="privacy-heading">
      <p class="eyebrow">{{ t("privacyEyebrow") }}</p>
      <h1>{{ t("privacyTitle") }}</h1>
      <p class="privacy-intro">{{ t("privacyIntro") }}</p>
      <p class="privacy-updated">{{ t("privacyUpdated") }}</p>
    </header>
    <div class="privacy-sections">
      <section
        v-for="(section, index) in privacySections"
        :key="section"
        class="privacy-section"
      >
        <span class="privacy-number">0{{ index + 1 }}</span>
        <div>
          <h2>{{ t(`privacy${section}Title`) }}</h2>
          <p>{{ t(`privacy${section}Body`) }}</p>
          <a
            v-if="section === 'Website'"
            class="text-link"
            :href="githubPrivacyUrl"
            >{{ t("privacyGithubLink") }} ↗</a
          ><a
            v-if="section === 'Contact'"
            class="text-link"
            :href="`mailto:${config.public.contactEmail}`"
            >{{ config.public.contactEmail }} ↗</a
          >
        </div>
      </section>
    </div>
  </main>
</template>
