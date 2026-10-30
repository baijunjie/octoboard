<script setup lang="ts">
import { featureGroups } from "../../seo/page-outline";

const { t } = useSiteCopy();
</script>

<template>
  <p class="feature-caption">{{ t("continuityIllustration") }}</p>
  <div class="feature-wall">
    <section v-for="(group, index) in featureGroups" :id="group.id" :key="group.id" class="feature-group" :aria-labelledby="`${group.id}-heading`">
      <div class="feature-group-heading">
        <span class="feature-group-index" aria-hidden="true">0{{ index + 1 }}</span>
        <SiteIcon :name="group.icon" />
        <h3 :id="`${group.id}-heading`">{{ t(group.key) }}</h3>
      </div>
      <div class="feature-grid">
        <article v-for="key in group.features" :key="key" class="feature-card" :class="{ 'feature-card-wide': key === 'reviewChanges' }">
          <FeatureIllustration :feature="key" />
          <div class="feature-copy">
            <span class="feature-card-mark" aria-hidden="true" />
            <h4>{{ t(`${key}FeatureTitle`) }}</h4>
            <p>{{ t(`${key}FeatureDescription`) }}</p>
          </div>
        </article>
      </div>
    </section>
  </div>
</template>

<style scoped>
.feature-caption { color: var(--muted); font-size: 13px; margin-bottom: 24px; }
.feature-wall { display: grid; gap: 56px; }
.feature-group-heading { display: flex; align-items: center; gap: 16px; margin-bottom: 24px; }
.feature-group-heading .site-icon { width: 20px; height: 20px; color: var(--red); }
.feature-group-heading h3 { font-size: 20px; }
.feature-group-index { color: var(--muted); font-size: 13px; font-variant-numeric: tabular-nums; margin-inline-end: 8px; }
.feature-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 16px; }
.feature-card { min-width: 0; overflow: hidden; background: #0d0d0d; border: 1px solid var(--edge); border-radius: 12px; }
.feature-copy { padding: 28px; }
.feature-card-mark { display: block; width: 20px; height: 2px; background: var(--red); margin-bottom: 24px; }
.feature-card h4 { font-size: 19px; font-weight: 500; line-height: 1.5; }
.feature-card p { color: var(--muted); font-size: 14px; line-height: 1.9; margin-top: 16px; }
.feature-card-wide { grid-column: 1 / -1; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr); }
.feature-card-wide .feature-copy { display: grid; align-content: center; }
.feature-card-wide :deep(.feature-illustration) { height: 100%; min-height: 272px; border-bottom: 0; border-inline-end: 1px solid var(--edge); }
@media (max-width: 1050px) { .feature-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
@media (max-width: 600px) { .feature-card-wide { display: block; } .feature-card-wide :deep(.feature-illustration) { height: 272px; border-inline-end: 0; border-bottom: 1px solid var(--edge); } .feature-grid { grid-template-columns: minmax(0, 1fr); } .feature-copy { padding: 24px; } .feature-wall { gap: 40px; } }
</style>
