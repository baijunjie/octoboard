<script setup lang="ts">
// The marks are LobeHub's (MIT); their licence text is public/licenses/lobehub-icons.txt.
import { AGENT_MARKS, markArt } from "@octoboard/ui/agent-marks";
const props = defineProps<{ agent: keyof typeof AGENT_MARKS }>();
const id = useId();
// A provider module's own constant SVG, never text from elsewhere.
const art = computed(() => markArt(AGENT_MARKS[props.agent], `mark${id.replace(/[^a-zA-Z0-9]/g, "")}`));
</script>

<template>
  <span class="agent-mark" aria-hidden="true">
    <svg v-if="art" viewBox="0 0 24 24" fill="none" v-html="art" />
    <svg v-else viewBox="0 0 24 24" :fill="AGENT_MARKS[agent].fill" fill-rule="evenodd"><path :d="AGENT_MARKS[agent].d" /></svg>
  </span>
</template>

<style scoped>
.agent-mark { display: inline-flex; flex: 0 0 auto; width: 22px; height: 22px; color: #dedede; }
svg { width: 100%; height: 100%; }
</style>
