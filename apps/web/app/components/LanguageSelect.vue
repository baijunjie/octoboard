<script setup lang="ts">
const { t } = useSiteCopy();
const { locale, locales } = useI18n();
const switchLocalePath = useSwitchLocalePath();
const currentLanguage = computed(
  () => locales.value.find((item) => item.code === locale.value)?.name,
);
function changeLocale(event: Event) {
  const path = switchLocalePath(
    (event.target as HTMLSelectElement).value as typeof locale.value,
  );
  if (path) navigateTo(path);
}
</script>

<template>
  <label class="language-select" data-marquee-scope>
    <span class="sr-only">{{ t("navLanguage") }}</span>
    <span class="language-globe" aria-hidden="true" />
    <FadeOverflow class="language-current" aria-hidden="true">{{
      currentLanguage
    }}</FadeOverflow>
    <span class="language-chevron" aria-hidden="true" />
    <select :value="locale" @change="changeLocale">
      <option v-for="item in locales" :key="item.code" :value="item.code">
        {{ item.name }}
      </option>
    </select>
  </label>
</template>
