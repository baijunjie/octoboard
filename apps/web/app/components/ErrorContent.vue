<script setup lang="ts">
const props = defineProps<{ statusCode: number }>();
const { t } = useSiteCopy();
const config = useRuntimeConfig();
const localePath = useLocalePath();
const notFound = computed(() => props.statusCode === 404);
const homeHref = computed(
  () => `${config.app.baseURL.replace(/\/$/, "")}${localePath("/")}`,
);
useSeoMeta({
  title: () =>
    t(notFound.value ? "errorNotFoundTitle" : "errorTitle").replaceAll(
      "\n",
      " ",
    ),
  robots: "noindex, nofollow",
});
</script>

<template>
  <main class="error-page">
    <a :href="homeHref" class="brand" :aria-label="config.public.appName"
      ><img
        :src="`${config.app.baseURL}brand.png`"
        alt=""
        width="160"
        height="54"
    /></a>
    <div class="error-layout wrap">
      <div>
        <p class="eyebrow"><span class="red-square" />{{ statusCode }}</p>
        <h1 class="preserve-lines">
          {{ t(notFound ? "errorNotFoundTitle" : "errorTitle") }}
        </h1>
        <p class="error-description">
          {{ t(notFound ? "errorNotFoundDescription" : "errorDescription") }}
        </p>
        <a :href="homeHref" class="button"
          >{{ t("errorHome") }}<span class="link-arrow" aria-hidden="true"
        /></a>
      </div>
      <img
        class="error-mascot"
        :src="`${config.app.baseURL}logo.png`"
        :alt="t('heroAssetAlt')"
        width="1254"
        height="1254"
      />
    </div>
  </main>
</template>
