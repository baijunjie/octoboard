<script setup lang="ts">
import { linuxCommandParts } from "../../seo/downloads";

defineProps<{ keypath: string }>();
const config = useRuntimeConfig();
</script>

<template>
  <!-- Each command is code that reads left to right in every language, so it is its own isolated run in the sentence. -->
  <i18n-t :keypath="keypath" tag="p">
    <template v-for="(command, name) in linuxCommandParts" :key="name" #[name]>
      <code dir="ltr">{{ command }}</code>
    </template>
    <template #appName>{{ config.public.appName }}</template>
  </i18n-t>
</template>
