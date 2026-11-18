<script setup lang="ts">
const { t } = useSiteCopy();
const localePath = useLocalePath();
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
    <NuxtLink class="text-link remote-guide-link" :to="localePath('/remote/')">
      {{ t("remoteGuideLink") }}<span class="link-arrow" aria-hidden="true" />
    </NuxtLink>
  </section>
</template>

<style scoped>
.remote-guide-link { margin-top: 24px; }
</style>
