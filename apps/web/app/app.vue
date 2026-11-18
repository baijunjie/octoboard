<script setup lang="ts">
const route = useRoute();
useSiteSeo();
</script>

<template>
  <div id="site-shell">
    <SiteHeader v-if="route.meta.siteChrome !== false" />
    <NuxtPage />
    <SiteFooter v-if="route.meta.siteChrome !== false" />
    <AnalyticsConsent v-if="route.meta.siteChrome !== false" />
  </div>
</template>
