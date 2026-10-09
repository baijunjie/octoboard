<script setup lang="ts">
const { t } = useSiteCopy();
const config = useRuntimeConfig();
const localePath = useLocalePath();
</script>

<template>
  <footer class="site-footer wrap">
    <div class="footer-identity">
      <NuxtLink
        :to="localePath('/')"
        class="brand footer-brand"
        :aria-label="config.public.appName"
        ><img
          :src="`${config.app.baseURL}wordmark.png`"
          alt=""
          width="151"
          height="25"
          loading="lazy"
      /></NuxtLink>
      <p>{{ t("footerTagline") }}</p>
    </div>
    <div class="footer-bottom">
      <nav class="footer-links" :aria-label="t('navMenu')">
        <a :href="config.public.repositoryUrl">GitHub</a
        ><NuxtLink :to="localePath('/privacy/')">{{
          t("footerPrivacy")
        }}</NuxtLink
        ><a :href="`${config.public.repositoryUrl}/blob/main/LICENSE`">{{
          t("footerLicense")
        }}</a
        ><a :href="`mailto:${config.public.contactEmail}`">{{
          t("footerContact")
        }}</a>
      </nav>
      <p>{{ t("footerCopyright", { year: 2026 }) }}</p>
    </div>
  </footer>
</template>
