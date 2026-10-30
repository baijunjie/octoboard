<script setup lang="ts">
import claude from "../../public/agents/claude.svg?raw";
import codex from "../../public/agents/codex.svg?raw";
import grok from "../../public/agents/grok.svg?raw";
defineProps<{ agent: string }>();
const marks: Record<string, string> = { claude, codex, grok };
</script>

<template><span class="feature-agent-mark" aria-hidden="true" v-html="marks[agent] || marks.claude" /></template>

<style scoped>
.feature-agent-mark { display: inline-flex; flex: 0 0 auto; width: 22px; height: 22px; color: #d9d5d3; }
.feature-agent-mark :deep(svg) { width: 100%; height: 100%; }
</style>
