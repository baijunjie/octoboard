<script setup lang="ts">
const { t } = useSiteCopy();
const { locale } = useI18n();
const config = useRuntimeConfig();
const scenarios = [
  { id: "overview", label: "previewScenarioOverview" },
  { id: "report", label: "previewScenarioReport" },
  { id: "changes", label: "previewScenarioChanges" },
] as const;
const selected = ref("overview");
const host = ref<HTMLElement | null>(null);
const frame = ref<HTMLIFrameElement | null>(null);
const width = ref(1280);
const visible = ref(false);
const ready = ref(false);
const failed = ref(false);
const frameFocused = ref(false);
const compact = computed(() => width.value <= 800);
const scale = computed(() =>
  compact.value ? 1 : Math.min(1, width.value / 1440),
);
const source = computed(() => {
  const language = locale.value === "zh" ? "zh-Hans" : locale.value;
  return `${config.app.baseURL}preview/website-preview.html?scene=${selected.value}&lang=${language}`;
});
let readinessTimer: ReturnType<typeof setTimeout> | undefined;
const attempt = ref(0);
function stopWaiting() {
  clearTimeout(readinessTimer);
}
watch([visible, source, attempt], () => {
  stopWaiting();
  ready.value = false;
  failed.value = false;
  if (visible.value)
    readinessTimer = setTimeout(() => {
      failed.value = true;
    }, 20000);
});
function chooseScenario(scene: string) {
  if (selected.value === scene && failed.value) attempt.value++;
  selected.value = scene;
}
let observer: IntersectionObserver | undefined;
let dimensions: ResizeObserver | undefined;
let focusTimer: ReturnType<typeof setTimeout> | undefined;
function syncFrameFocus() {
  clearTimeout(focusTimer);
  // Focus inside an iframe does not propagate CSS :focus-within to its parent.
  focusTimer = setTimeout(() => {
    frameFocused.value = document.activeElement === frame.value;
  }, 0);
}
function receive(event: MessageEvent) {
  if (event.source !== frame.value?.contentWindow) return;
  if (
    event.data?.type === "octoboard-preview-scroll" &&
    typeof event.data.deltaY === "number" &&
    Number.isFinite(event.data.deltaY)
  ) {
    window.scrollBy({ top: event.data.deltaY, behavior: "instant" });
    return;
  }
  if (event.data?.type === "octoboard-preview-ready") {
    stopWaiting();
    ready.value = true;
    failed.value = false;
  }
  if (event.data?.type === "octoboard-preview-error") {
    stopWaiting();
    failed.value = true;
  }
}
onMounted(() => {
  if (!host.value) return;
  dimensions = new ResizeObserver(([entry]) => {
    if (entry) width.value = entry.contentRect.width;
  });
  dimensions.observe(host.value);
  observer = new IntersectionObserver(
    ([entry]) => {
      if (entry?.isIntersecting) {
        visible.value = true;
        observer?.disconnect();
      }
    },
    { rootMargin: "300px" },
  );
  observer.observe(host.value);
  window.addEventListener("message", receive);
  window.addEventListener("blur", syncFrameFocus);
  window.addEventListener("focus", syncFrameFocus);
  document.addEventListener("focusin", syncFrameFocus);
});
onBeforeUnmount(() => {
  stopWaiting();
  dimensions?.disconnect();
  observer?.disconnect();
  window.removeEventListener("message", receive);
  clearTimeout(focusTimer);
  window.removeEventListener("blur", syncFrameFocus);
  window.removeEventListener("focus", syncFrameFocus);
  document.removeEventListener("focusin", syncFrameFocus);
});
</script>

<template>
  <div class="product-preview">
    <div class="preview-toolbar">
      <div
        class="preview-scenarios"
        role="group"
        :aria-label="t('previewEyebrow')"
      >
        <button
          v-for="(scenario, index) in scenarios"
          :key="scenario.id"
          type="button"
          :aria-pressed="selected === scenario.id"
          @click="chooseScenario(scenario.id)"
        >
          <span aria-hidden="true">0{{ index + 1 }}</span
          >{{ t(scenario.label) }}
        </button>
      </div>
      <a :href="source" target="_blank" rel="noopener" class="text-link">
        {{ t("previewOpen") }}<span class="link-arrow" aria-hidden="true" />
      </a>
    </div>
    <div
      ref="host"
      class="preview-window"
      :class="{
        'is-ready': ready,
        'is-compact': compact,
        'is-focused': frameFocused,
      }"
      :style="{
        '--preview-scale': scale,
        height: `${Math.round((compact ? 640 : 900) * scale)}px`,
      }"
    >
      <div v-if="!ready" class="preview-loading" role="status">
        {{ t(failed ? "errorDescription" : "previewLoading") }}
      </div>
      <div class="preview-crop" :inert="!ready" :aria-hidden="!ready">
        <iframe
          v-if="visible"
          :key="`${source}-${attempt}`"
          ref="frame"
          :src="source"
          :title="t('previewCaption')"
          sandbox="allow-scripts allow-same-origin"
          width="1440"
          height="900"
        />
      </div>
      <div class="preview-corner corner-start" aria-hidden="true" />
      <div class="preview-corner corner-end" aria-hidden="true" />
    </div>
    <p class="preview-caption">
      <span class="red-square" aria-hidden="true" />{{ t("previewCaption") }}
    </p>
  </div>
</template>

<style scoped>
.preview-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  margin-bottom: 32px;
}
.preview-scenarios {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.preview-scenarios button {
  display: flex;
  gap: 12px;
  align-items: center;
  border: 1px solid transparent;
  padding: 14px 20px;
  color: #b6b6b6;
  background: transparent;
  transition:
    background 220ms,
    color 220ms,
    border-color 220ms;
}
.preview-scenarios button > span {
  font-size: 15px;
  font-variant-numeric: tabular-nums;
  color: #aaa;
}
.preview-scenarios button:hover {
  background: #151515;
  color: #fff;
}
.preview-scenarios button[aria-pressed="true"] {
  border-color: #555;
  background: #191919;
  color: #fff;
}
.preview-scenarios button[aria-pressed="true"] > span {
  color: var(--red);
}
.preview-window {
  position: relative;
  overflow: hidden;
  border-top: 1px solid #454545;
  border-radius: 12px 12px 0 0;
  background: #131313;
  mask-image: linear-gradient(#000 72%, transparent 100%);
}
.preview-window:hover,
.preview-window.is-focused,
.preview-window:focus-within {
  mask-image: none;
}
.preview-crop {
  position: absolute;
  inset: 0;
  overflow: hidden;
  opacity: 0;
  transition: opacity 500ms;
  pointer-events: none;
}
.is-ready .preview-crop {
  opacity: 1;
  pointer-events: auto;
}
iframe {
  position: absolute;
  left: 0;
  top: 0;
  width: 1440px;
  height: 900px;
  max-width: none;
  border: 0;
  transform: scale(var(--preview-scale));
  transform-origin: top left;
  background: #171717;
}
.is-compact iframe {
  width: 100%;
  height: 100%;
  transform: none;
}
.preview-loading {
  display: grid;
  place-items: center;
  position: absolute;
  inset: 0;
  color: #c4c4c4;
  padding: 32px;
}
.preview-caption {
  position: relative;
  margin-top: 16px;
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 12px;
  color: #aaa;
  font-size: 13px;
}
.preview-corner {
  pointer-events: none;
  position: absolute;
  top: 0;
  width: 24px;
  height: 24px;
  border-top: 1px solid #a5a5a5;
}
.corner-start {
  left: 0;
  border-left: 1px solid #a5a5a5;
}
.corner-end {
  right: 0;
  border-right: 1px solid #a5a5a5;
}
@media (max-width: 800px) {
  .preview-toolbar {
    align-items: flex-start;
    flex-direction: column;
    gap: 16px;
  }
  .preview-scenarios {
    gap: 4px;
  }
  .preview-scenarios button {
    padding: 12px;
    gap: 8px;
    font-size: 14px;
  }
  .preview-scenarios button > span {
    display: none;
  }
}
@media (hover: none) {
  .preview-window {
    mask-image: none;
  }
}
</style>
