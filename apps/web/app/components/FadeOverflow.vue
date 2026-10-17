<script setup lang="ts">
// The website is a Vue application: keep the app's one-line fade and marquee behaviour here,
// without importing the desktop React UI. The mask affects only the text, never the control fill.
const element = ref<HTMLElement>();
const mask = ref<string>();
let observer: ResizeObserver | undefined;
let scope: HTMLElement | undefined;
let motion: MediaQueryList | undefined;
let frame: number | undefined;
let hovered = false;
let position = 0;
let last: number | undefined;
let phase: "initial" | "rest" | "forward" | "far" | "back" = "initial";
let pausedAt: number | undefined;

function extent(): number {
  const node = element.value;
  return node ? Math.max(0, node.scrollWidth - node.clientWidth) : 0;
}

function paint(): void {
  const node = element.value;
  if (!node) return;
  const rtl = getComputedStyle(node).direction === "rtl";
  node.scrollLeft = rtl ? -position : position;
  const offset = Math.abs(node.scrollLeft);
  const start = offset > 1;
  const end = extent() - offset > 1;
  const fade = Math.floor(Math.min(24, node.clientWidth / (Number(start) + Number(end) + 1)));
  mask.value = !fade || (!start && !end) ? undefined :
    `linear-gradient(to ${rtl ? "left" : "right"}, ${start ? `transparent 0, #000 ${fade}px` : "#000 0"}, ${end ? `#000 calc(100% - ${fade}px), transparent 100%` : "#000 100%"})`;
}

function cancel(): void {
  if (frame !== undefined) cancelAnimationFrame(frame);
  frame = undefined;
  last = undefined;
}

function tick(now: number): void {
  frame = undefined;
  const distance = extent();
  const elapsed = last === undefined ? 0 : (now - last) / 1000;
  last = now;
  if (motion?.matches || distance <= 1) {
    position = 0;
    paint();
    last = undefined;
    return;
  }
  if (phase === "initial" || phase === "rest" || phase === "far") {
    pausedAt ??= now;
    if (now - pausedAt >= (phase === "initial" ? 600 : 3000)) {
      phase = phase === "far" ? "back" : "forward";
      pausedAt = undefined;
    }
  } else {
    const target = phase === "forward" ? distance : 0;
    const step = Math.min(Math.abs(target - position), (phase === "forward" ? 60 : distance * 5) * elapsed);
    position += Math.sign(target - position) * step;
    if (Math.abs(position - target) < 0.5) {
      position = target;
      phase = phase === "forward" ? "far" : "rest";
      pausedAt = undefined;
    }
    paint();
  }
  if (hovered || position > 0) frame = requestAnimationFrame(tick);
  else last = undefined;
}

function measure(): void {
  position = Math.min(position, extent());
  paint();
  if (hovered && extent() > 1 && !motion?.matches && frame === undefined) {
    last = undefined;
    frame = requestAnimationFrame(tick);
  }
}

function enter(): void {
  hovered = true;
  phase = "initial";
  pausedAt = undefined;
  measure();
}

function leave(): void {
  hovered = false;
  motionChanged();
}

function motionChanged(): void {
  cancel();
  position = 0;
  phase = "initial";
  pausedAt = undefined;
  measure();
}

onMounted(() => {
  const node = element.value;
  if (!node) return;
  scope = node.closest<HTMLElement>("[data-marquee-scope]") ?? node;
  motion = window.matchMedia("(prefers-reduced-motion: reduce)");
  motion.addEventListener("change", motionChanged);
  scope.addEventListener("pointerenter", enter);
  scope.addEventListener("pointerleave", leave);
  observer = new ResizeObserver(measure);
  observer.observe(node);
  measure();
});
onUpdated(measure);
onBeforeUnmount(() => {
  cancel();
  observer?.disconnect();
  scope?.removeEventListener("pointerenter", enter);
  scope?.removeEventListener("pointerleave", leave);
  motion?.removeEventListener("change", motionChanged);
});
</script>

<template>
  <span ref="element" class="fade-overflow" dir="auto" :style="{ maskImage: mask, WebkitMaskImage: mask }"><slot /></span>
</template>

<style scoped>
.fade-overflow {
  display: block;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-align: match-parent;
}
</style>
