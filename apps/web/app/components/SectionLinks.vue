<script setup lang="ts">
import { featureGroups, homeSections } from "../../seo/page-outline";

defineProps<{ activeSection: string; activeGroup: string; expanded?: boolean }>();
const emit = defineEmits<{ navigate: [id: string] }>();
const { t } = useSiteCopy();
</script>

<template>
  <ol class="contents-links">
    <li v-for="section in homeSections" :key="section.id">
      <a :href="`#${section.id}`" :aria-current="activeSection === section.id ? 'location' : undefined" @click="emit('navigate', section.id)">
        {{ t(section.key) }}
      </a>
      <ol v-if="section.id === 'benefits'" class="contents-children" :class="{ expanded: expanded || activeSection === 'benefits' }">
        <li v-for="group in featureGroups" :key="group.id">
          <a :href="`#${group.id}`" :aria-current="activeGroup === group.id ? 'location' : undefined" @click="emit('navigate', group.id)">{{ t(group.key) }}</a>
        </li>
      </ol>
    </li>
  </ol>
</template>

<style scoped>
.contents-links, .contents-children { list-style: none; padding: 0; margin: 0; }
.contents-links a { display: block; padding: 9px 14px; border-inline-start: 1px solid var(--edge); color: var(--muted); font-size: 13px; line-height: 1.6; overflow-wrap: anywhere; }
.contents-links a:focus-visible { outline-offset: -2px; }
.contents-links a:hover { color: var(--text); background: #ffffff08; }
.contents-links a[aria-current] { color: var(--text); border-color: var(--red); background: #ff4b3e0c; }
.contents-children { display: none; padding-inline-start: 14px; }
.contents-children.expanded, li:focus-within > .contents-children { display: block; }
.contents-children a { font-size: 12px; padding-block: 7px; }
</style>
