<script setup lang="ts">
import { Globe, Laptop, Server } from "@lucide/vue";

const { t } = useSiteCopy();
</script>

<template>
  <figure class="remote-route">
    <div class="route-node route-clients">
      <span><Laptop aria-hidden="true" />{{ t("remoteDiagramApp") }}</span>
      <span><Globe aria-hidden="true" />{{ t("remoteDiagramBrowser") }}</span>
    </div>
    <div class="route-connection">
      <span>SSH</span>
      <FeatureGlyph name="arrow-both" :size="28" />
    </div>
    <div class="route-node route-host">
      <Server aria-hidden="true" />{{ t("remoteDiagramHost") }}
    </div>
  </figure>
</template>

<style scoped>
.remote-route {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(50px, 0.35fr) minmax(0, 1fr);
  align-items: stretch;
  gap: clamp(6px, 2vw, 24px);
  margin: 0;
  padding: clamp(12px, 4vw, 36px);
  border: 1px solid var(--edge);
  border-radius: 12px;
  background: radial-gradient(ellipse at 50% 95%, #30201c55, transparent 80%), #111112;
}
.route-node {
  min-width: 0;
  display: grid;
  align-content: center;
  gap: 10px;
  padding: clamp(8px, 2vw, 24px);
  border: 1px solid #3a363b;
  border-radius: 9px;
  background: linear-gradient(135deg, #282529, #1e1c20);
  font-size: clamp(12px, 1.3vw, 16px);
  overflow-wrap: anywhere;
}
.route-node > span { display: flex; align-items: center; gap: 10px; }
.route-node svg { width: 20px; height: 20px; flex-shrink: 0; color: #d78476; }
.route-connection { display: grid; justify-items: center; align-content: center; gap: 6px; color: #d78476; font-size: 13px; }
.route-host { border-color: #79554e; }
@media (max-width: 600px) {
  .route-node > span { flex-wrap: wrap; gap: 4px; }
}
</style>
