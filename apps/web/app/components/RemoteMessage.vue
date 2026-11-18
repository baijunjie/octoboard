<script setup lang="ts">
import { remoteAppLabels, remoteCode } from "../../seo/remote-references";

defineProps<{ keypath: string }>();
const { t } = useSiteCopy();
const config = useRuntimeConfig();
</script>

<template>
  <i18n-t :keypath="keypath" scope="global">
    <template v-for="(_, key) in remoteAppLabels" :key="key" #[key]>
      <bdi class="remote-reference">{{ t(key) }}</bdi>
    </template>
    <template #settingsPath>
      <bdi class="remote-reference">
        {{ t("remoteLabelSettings") }} <span class="reference-chevron" aria-hidden="true">›</span> {{ t("remoteLabelAccess") }}
      </bdi>
    </template>
    <template v-for="(command, key) in remoteCode" :key="key" #[key]>
      <code dir="ltr">{{ command }}</code>
    </template>
    <template #appName>{{ config.public.appName }}</template>
  </i18n-t>
</template>

<style scoped>
.remote-reference {
  padding: 0.12em 0.34em;
  border-radius: 0.3em;
  background: #292627;
  color: var(--text);
  font: inherit;
  white-space: nowrap;
  box-decoration-break: clone;
  -webkit-box-decoration-break: clone;
}
.reference-chevron { display: inline-block; color: var(--muted); }
.remote-reference:dir(rtl) .reference-chevron { transform: scaleX(-1); }
code { direction: ltr; unicode-bidi: isolate; }
</style>
