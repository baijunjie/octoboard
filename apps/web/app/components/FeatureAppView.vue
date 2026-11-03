<script setup lang="ts">
import {
  FEATURE_MESSAGE,
  readFeatureMessage,
} from "@octoboard/ui/preview-messages";
import { localeByCode } from "../../seo/site-seo";
const props = defineProps<{ feature: string }>();
const { locale } = useI18n();
const { t } = useSiteCopy();
const config = useRuntimeConfig();
const host = ref<HTMLElement | null>(null);
const frame = ref<HTMLIFrameElement | null>(null);
const source = computed(() => `${config.app.baseURL}preview/feature-preview.html?feature=${props.feature}&lang=${localeByCode(locale.value).language}`);
function receive(data: unknown) {
  const message = readFeatureMessage(data);
  if (message?.type === FEATURE_MESSAGE.ready && message.feature === props.feature) markReady();
  if (message?.type === FEATURE_MESSAGE.error) markFailed();
}
const { width, height, visible, ready, failed, markReady, markFailed } = usePreviewFrame({
  host,
  frame,
  source,
  rootMargin: "240px",
  initialSize: { width: 320, height: 240 },
  onMessage: receive,
});
const canvasWidth = computed(() => width.value < 350 ? 320 : 400);
const canvasHeight = computed(() => width.value < 350 ? 300 : 260);
const scale = computed(() => Math.min(1, width.value / canvasWidth.value, height.value / canvasHeight.value));
</script>

<template>
  <div ref="host" class="feature-app-view" :class="{ 'is-ready': ready }" aria-hidden="true" inert>
    <div v-if="!ready" class="app-view-placeholder"><FeatureGlyph name="file" :size="24" /><span>{{ t(failed ? 'featurePreviewError' : 'previewLoading') }}</span></div>
    <iframe v-if="visible" :key="source" ref="frame" :src="source" :title="t(`${feature}FeatureTitle`)" :width="canvasWidth" :height="canvasHeight" tabindex="-1" aria-hidden="true" inert sandbox="allow-scripts allow-same-origin" :style="{ width: `${canvasWidth}px`, height: `${canvasHeight}px`, transform: `translate(-50%, -50%) scale(${scale})` }" />
  </div>
</template>

<style scoped>
.feature-app-view { position: relative; width: 100%; height: 100%; min-height: 0; pointer-events: none; }
iframe { position: absolute; top: 50%; left: 50%; width: 400px; height: 260px; max-width: none; border: 0; opacity: 0; transform-origin: center; pointer-events: none; }
.is-ready iframe { opacity: 1; }
.app-view-placeholder { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; gap: 10px; color: #98939c; font-size: 12px; }
</style>
