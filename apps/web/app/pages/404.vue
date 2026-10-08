<script setup lang="ts">
definePageMeta({ siteChrome: false });
defineI18nRoute(false);
</script>

<template>
  <ErrorContent :status-code="404" />
</template>
