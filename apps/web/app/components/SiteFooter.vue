<script setup lang="ts">
import { docsPages, docsPath } from "../../seo/site-seo";
const { t } = useSiteCopy();
const { locale } = useI18n();
const config = useRuntimeConfig();
// The docs are a separate build under the same base path, so they are an ordinary link, not a route.
const docsHref = computed(() => docsPath(config.app.baseURL, locale.value));
const connectHref = computed(() => docsPath(config.app.baseURL, locale.value, docsPages.chooseHowToConnect));
const localePath = useLocalePath();
// `data-analytics-settings` marks the button the analytics prompt hands focus to when it has
// nowhere else to return it.
const { open, reopen } = useAnalyticsConsent();
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
      <nav class="footer-links" :aria-label="t('footerNav')">
        <a
          :href="config.public.repositoryUrl"
          class="github-link"
          aria-label="GitHub"
          title="GitHub"
        ><SiteIcon name="github-logo" /></a
        ><NuxtLink :to="`${localePath('/')}#developer-intent`">{{
          t("developerEyebrow")
        }}</NuxtLink
        ><a :href="connectHref">{{ t("remoteEyebrow") }}</a
        ><a :href="docsHref">{{ t("navDocs") }}</a
        ><NuxtLink :to="localePath('/privacy/')">{{
          t("footerPrivacy")
        }}</NuxtLink
        ><a :href="`${config.public.repositoryUrl}/blob/main/LICENSE`">{{
          t("footerLicense")
        }}</a
        ><a :href="`mailto:${config.public.contactEmail}`">{{
          t("footerContact")
        }}</a
        ><button
          type="button"
          data-analytics-settings
          :aria-expanded="open"
          @click="reopen($event)"
        >
          {{ t("footerAnalyticsSettings") }}
        </button>
      </nav>
      <p>{{ t("footerCopyright", { year: config.public.copyrightYear }) }}</p>
    </div>
  </footer>
</template>
