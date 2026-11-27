<script setup lang="ts">
import { ArrowLeftRight, FileText } from "@lucide/vue";

const icons = { "arrow-both": ArrowLeftRight, file: FileText };
withDefaults(defineProps<{ name: keyof typeof icons; size?: number }>(), { size: 20 });
</script>

<template>
  <component :is="icons[name]" :size="size" aria-hidden="true" />
</template>
