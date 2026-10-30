<script setup lang="ts">
const props = defineProps<{ feature: string }>();
const { locale } = useI18n();
const { t } = useSiteCopy();
const config = useRuntimeConfig();
const host = ref<HTMLElement | null>(null);
const frame = ref<HTMLIFrameElement | null>(null);
const visible = ref(false);
const ready = ref(false);
const failed = ref(false);
const width = ref(320);
const height = ref(240);
const canvasWidth = computed(() => width.value < 350 ? 320 : 400);
const canvasHeight = computed(() => width.value < 350 ? 300 : 260);
const scale = computed(() => Math.min(1, width.value / canvasWidth.value, height.value / canvasHeight.value));
const source = computed(() => `${config.app.baseURL}preview/feature-preview.html?feature=${props.feature}&lang=${locale.value === "zh" ? "zh-Hans" : locale.value}`);
let observer: IntersectionObserver | undefined;
let dimensions: ResizeObserver | undefined;
let timer: ReturnType<typeof setTimeout> | undefined;
function receive(event: MessageEvent) {
  if (event.source !== frame.value?.contentWindow || event.origin !== window.location.origin) return;
  if (event.data?.type === "octoboard-feature-ready" && event.data.feature === props.feature) {
    clearTimeout(timer);
    ready.value = true;
  }
  if (event.data?.type === "octoboard-feature-error") failed.value = true;
}
watch(source, () => { ready.value = false; failed.value = false; });
watch([visible, source], () => {
  clearTimeout(timer);
  if (visible.value) timer = setTimeout(() => { if (!ready.value) failed.value = true; }, 20000);
});
onMounted(() => {
  if (!host.value) return;
  dimensions = new ResizeObserver(([entry]) => {
    if (entry) { width.value = entry.contentRect.width; height.value = entry.contentRect.height; }
  });
  dimensions.observe(host.value);
  observer = new IntersectionObserver(([entry]) => {
    if (entry?.isIntersecting) { visible.value = true; observer?.disconnect(); }
  }, { rootMargin: "240px" });
  observer.observe(host.value);
  window.addEventListener("message", receive);
});
onBeforeUnmount(() => { observer?.disconnect(); dimensions?.disconnect(); clearTimeout(timer); window.removeEventListener("message", receive); });
</script>

<template>
  <div ref="host" class="feature-app-view" :class="{ 'is-ready': ready }" aria-hidden="true" inert>
    <div v-if="!ready" class="app-view-placeholder"><FeatureGlyph name="file" :size="24" /><span>{{ t(failed ? 'previewError' : 'previewLoading') }}</span></div>
    <iframe v-if="visible" :key="source" ref="frame" :src="source" :title="t(`${feature}FeatureTitle`)" :width="canvasWidth" :height="canvasHeight" tabindex="-1" aria-hidden="true" inert sandbox="allow-scripts allow-same-origin" :style="{ width: `${canvasWidth}px`, height: `${canvasHeight}px`, transform: `translate(-50%, -50%) scale(${scale})` }" />
  </div>
</template>

<style scoped>
.feature-app-view { position: relative; width: 100%; height: 100%; min-height: 0; pointer-events: none; }
iframe { position: absolute; top: 50%; left: 50%; width: 400px; height: 260px; max-width: none; border: 0; opacity: 0; transform-origin: center; pointer-events: none; }
.is-ready iframe { opacity: 1; }
.app-view-placeholder { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; gap: 10px; color: #98939c; font-size: 12px; }
</style>
