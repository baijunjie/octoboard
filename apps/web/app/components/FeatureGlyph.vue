<script setup lang="ts">
import {
  ArrowDown, ArrowLeftRight, ArrowRight, ArrowUp,
  Check, ChevronRight, Circle, CircleCheck, CirclePause, CirclePlay,
  CircleUserRound, CodeXml, Ellipsis, FileText,
  Folder, GitBranch, History, Layers, MessageSquareText,
  ShieldCheck, TextQuote,
} from "@lucide/vue";

const icons = {
  account: CircleUserRound,
  chat: MessageSquareText,
  file: FileText,
  folder: Folder,
  check: Check,
  history: History,
  shield: ShieldCheck,
  branch: GitBranch,
  coordinator: Layers,
  "pause-circle": CirclePause,
  "check-circle": CircleCheck,
  "play-circle": CirclePlay,
  "arrow-right": ArrowRight,
  "arrow-down": ArrowDown,
  "arrow-up": ArrowUp,
  "arrow-both": ArrowLeftRight,
  working: Circle,
  more: Ellipsis,
  context: TextQuote,
  code: CodeXml,
  prompt: ChevronRight,
};
withDefaults(defineProps<{ name: keyof typeof icons; size?: number }>(), { size: 20 });
</script>

<template>
  <component :is="icons[name]" :size="size" :fill="name === 'working' ? 'currentColor' : 'none'" :stroke-width="name === 'working' ? 0 : 2" aria-hidden="true" />
</template>
