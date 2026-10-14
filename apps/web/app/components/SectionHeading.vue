<script setup lang="ts">
defineProps<{
  number: string;
  label: string;
  title: string;
  description?: string;
}>();
</script>

<template>
  <header class="section-heading">
    <div class="section-kicker">
      <span class="section-number" aria-hidden="true" dir="ltr">{{
        number
      }}</span>
      <span class="eyebrow"
        ><span class="red-square" aria-hidden="true" />{{ label }}</span
      >
      <span class="section-rule" aria-hidden="true" />
    </div>
    <div class="section-heading-copy">
      <h2 class="preserve-lines">{{ title }}</h2>
      <p v-if="description">{{ description }}</p>
    </div>
  </header>
</template>
