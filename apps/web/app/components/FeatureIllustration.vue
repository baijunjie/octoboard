<script setup lang="ts">
import type { featureGroups } from "../../seo/page-outline";
import accountContinuity from "../assets/feature-art/account-continuity.webp";
import quotaScheduling from "../assets/feature-art/quota-scheduling.webp";
import startUsageWindows from "../assets/feature-art/start-usage-windows.webp";
import sharedConfiguration from "../assets/feature-art/shared-configuration.webp";
import freshSession from "../assets/feature-art/fresh-session.webp";
import agentHandoff from "../assets/feature-art/agent-handoff.webp";
import openAgentApp from "../assets/feature-art/open-agent-app.webp";
import crossProjectWork from "../assets/feature-art/cross-project-work.webp";
import nativeProjectSessions from "../assets/feature-art/native-project-sessions.webp";
import keepAwake from "../assets/feature-art/keep-awake.webp";

type Feature = (typeof featureGroups)[number]["features"][number];
const props = defineProps<{ feature: Feature }>();
const artwork: Record<Feature, string | null> = {
  accountContinuity,
  quotaScheduling,
  startUsageWindows,
  sharedConfiguration,
  freshSession,
  agentHandoff,
  openAgentApp,
  crossProjectWork,
  nativeProjectSessions,
  keepAwake,
  managedAccounts: null,
  usageMonitor: null,
  sharedMemory: null,
  sessionIntervention: null,
  scheduledTasks: null,
  processMonitoring: null,
  reviewChanges: null,
};
const image = computed(() => artwork[props.feature]);
</script>

<template>
  <div class="feature-illustration" :class="{ 'product-stage': !image }" aria-hidden="true">
    <img v-if="image" :src="image" width="192" height="128" alt="" loading="lazy" decoding="async" />
    <FeatureAppView v-else :feature="feature" />
  </div>
</template>

<style scoped>
.feature-illustration {
  position: relative;
  overflow: hidden;
  aspect-ratio: 3 / 2;
  background: #111113;
  user-select: none;
}
.feature-illustration img {
  display: block;
  width: 100%;
  height: 100%;
  image-rendering: pixelated;
}
.product-stage {
  display: grid;
  place-items: center;
  padding: clamp(10px, 4%, 24px);
  background:
    radial-gradient(ellipse at 18% -25%, #78757966, transparent 62%),
    radial-gradient(ellipse at 100% 106%, #71392f47, transparent 57%),
    linear-gradient(140deg, #242428, #121214 58%, #1a1719);
}
.product-stage .feature-app-view { max-width: 416px; }
</style>
