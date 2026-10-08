<script setup lang="ts">
const props = defineProps<{
  status: "working" | "idle";
  animating: boolean;
}>();
const { t } = useSiteCopy();
const label = computed(() =>
  t(props.status === "working" ? "demoStatusWorking" : "demoStatusIdle"),
);

// MessageCircleMore uses the original lucide-react 1.52.0 paths, as in the application StatusIcon.
// Copyright 2026 Lucide Icons and Contributors; ISC license in public/licenses/lucide-icons.txt.
const messagePaths = [
  "M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719",
  "M8 12h.01",
  "M12 12h.01",
  "M16 12h.01",
];
</script>

<template>
  <span
    class="workflow-status"
    :class="{ working: status === 'working', paused: !animating }"
    role="img"
    :aria-label="label"
  >
    <span class="status-glyph" aria-hidden="true">
      <span v-if="status === 'working'" class="working-dot"
        ><span class="working-ping"
      /></span>
      <svg
        v-else
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path v-for="path in messagePaths" :key="path" :d="path" />
      </svg>
    </span>
    <span class="status-label" aria-hidden="true">
      <span :class="{ current: status === 'working' }">{{
        t("demoStatusWorking")
      }}</span>
      <span :class="{ current: status === 'idle' }">{{
        t("demoStatusIdle")
      }}</span>
    </span>
  </span>
</template>

<style scoped>
.workflow-status {
  display: flex;
  align-items: center;
  gap: 8px;
  color: #72d6a0;
  font-size: 12px;
  line-height: 16px;
}
.workflow-status.working {
  color: var(--red);
}
.status-glyph {
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}
.status-glyph svg {
  width: 16px;
  height: 16px;
}
.working-dot,
.working-ping {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: currentColor;
}
.working-dot {
  position: relative;
}
.working-ping {
  position: absolute;
  inset: 0;
  animation: status-ping 1000ms cubic-bezier(0, 0, 0.2, 1) infinite;
}
.paused .working-ping {
  animation-play-state: paused;
}
/* Reserve both labels so status changes never move diagram endpoints. */
.status-label {
  display: grid;
  min-width: 0;
  overflow-wrap: anywhere;
}
.status-label > span {
  grid-area: 1 / 1;
  visibility: hidden;
}
.status-label > .current {
  visibility: visible;
}
@keyframes status-ping {
  75%,
  100% {
    transform: scale(2);
    opacity: 0;
  }
}
@media (prefers-reduced-motion: reduce) {
  .working-ping {
    animation: none;
    display: none;
  }
}
</style>
