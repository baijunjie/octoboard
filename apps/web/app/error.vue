<script setup lang="ts">
import type { NuxtError } from "#app";
defineProps<{ error: NuxtError }>();
</script>

<template>
  <ErrorContent :status-code="error.statusCode ?? 500" />
</template>
