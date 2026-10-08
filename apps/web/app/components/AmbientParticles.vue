<script setup lang="ts">
const canvas = ref<HTMLCanvasElement | null>(null);
let dispose: (() => void) | undefined;

onMounted(() => {
  const surface = canvas.value;
  const hero = surface?.parentElement;
  const context = surface?.getContext("2d");
  if (
    !surface ||
    !hero ||
    !context ||
    !("IntersectionObserver" in window) ||
    !("ResizeObserver" in window)
  )
    return;

  const motion = matchMedia("(prefers-reduced-motion: reduce)");
  const protectedElements = Array.from(
    hero.querySelectorAll<HTMLElement>(".hero-copy, .hero-art, .hero-bottom"),
  );
  type Bounds = { left: number; top: number; right: number; bottom: number };
  type Particle = {
    x: number;
    y: number;
    size: number;
    distance: number;
    period: number;
    phase: number;
    pulse: number;
    red: boolean;
    vertical: boolean;
  };
  let width = 0;
  let height = 0;
  let compact = false;
  let visible = false;
  let frame = 0;
  let previous = 0;
  let elapsed = 0;
  let bounds: Bounds[] = [];
  let particles: Particle[] = [];

  function freeSpace(x: number, y: number, size: number, boxHeight = size) {
    return !bounds.some(
      (area) =>
        x + size > area.left &&
        x < area.right &&
        y + boxHeight > area.top &&
        y < area.bottom,
    );
  }

  function draw() {
    context!.clearRect(0, 0, width, height);
    for (const particle of particles) {
      const progress = (elapsed / particle.period + particle.phase) % 1;
      const travel = Math.sin(progress * Math.PI * 2) * 0.5;
      const drift = Math.round(travel * particle.distance);
      const x = Math.round(particle.x + (particle.vertical ? 0 : drift));
      const y = Math.round(particle.y + (particle.vertical ? drift : 0));
      if (!freeSpace(x - 2, y - 2, particle.size + 4)) continue;

      const pulse = (elapsed / particle.pulse + particle.phase) % 1;
      const light = (1 - Math.cos(pulse * Math.PI * 2)) / 2;
      context!.fillStyle = particle.red
        ? `rgba(255, 75, 62, ${0.1 + light * 0.08})`
        : `rgba(210, 216, 222, ${0.06 + light * 0.06})`;
      context!.fillRect(x - 2, y - 2, particle.size + 4, particle.size + 4);
      context!.fillStyle = particle.red
        ? `rgba(255, 75, 62, ${0.6 + light * 0.3})`
        : `rgba(210, 216, 222, ${0.4 + light * 0.3})`;
      context!.fillRect(x, y, particle.size, particle.size);
    }
  }

  function resize() {
    const rect = hero!.getBoundingClientRect();
    width = rect.width;
    height = rect.height;
    compact = width < 768;
    const ratio = Math.min(window.devicePixelRatio || 1, compact ? 1 : 1.5);
    surface!.width = Math.ceil(width * ratio);
    surface!.height = Math.ceil(height * ratio);
    context!.setTransform(ratio, 0, 0, ratio, 0, 0);

    // Reserve the full entrance-motion envelope as well as the resting content boxes.
    const contentMargin = 40;
    bounds = protectedElements.map((element) => {
      const area = element.getBoundingClientRect();
      return {
        left: area.left - rect.left - contentMargin,
        top: area.top - rect.top - contentMargin,
        right: area.right - rect.left + contentMargin,
        bottom: area.bottom - rect.top + contentMargin,
      };
    });
    particles = [];
    for (let index = 0; index < (compact ? 28 : 96); index++) {
      const size = !compact && index % 12 === 0 ? 8 : index % 3 === 0 ? 4 : 2;
      const distance = 24 + Math.random() * 24;
      const vertical = index % 2 === 0;
      const reachX = vertical ? 2 : distance / 2 + 2;
      const reachY = vertical ? distance / 2 + 2 : 2;
      for (let attempt = 0; attempt < 120; attempt++) {
        const x =
          reachX + Math.random() * Math.max(0, width - size - reachX * 2);
        const y =
          reachY + Math.random() * Math.max(0, height - size - reachY * 2);
        // Reserve the complete drift and glow envelope so particles never pop across content.
        if (
          !freeSpace(
            x - reachX,
            y - reachY,
            size + reachX * 2,
            size + reachY * 2,
          )
        )
          continue;
        particles.push({
          x,
          y,
          size,
          distance,
          period: 12 + Math.random() * 12,
          phase: Math.random(),
          pulse: 7 + Math.random() * 8,
          red: index % 2 === 0,
          vertical,
        });
        break;
      }
    }
    if (visible && !document.hidden && !motion.matches) draw();
  }

  function tick(timestamp: number) {
    frame = requestAnimationFrame(tick);
    if (!previous) previous = timestamp;
    const delta = timestamp - previous;
    if (delta < 1000 / (compact ? 16 : 24)) return;
    elapsed += Math.min(delta, 100) / 1000;
    previous = timestamp;
    draw();
  }

  function synchronize() {
    if (visible && !document.hidden && !motion.matches) {
      if (!frame) {
        previous = 0;
        frame = requestAnimationFrame(tick);
      }
    } else {
      cancelAnimationFrame(frame);
      frame = 0;
      previous = 0;
      if (motion.matches) context!.clearRect(0, 0, width, height);
    }
  }

  const visibility = new IntersectionObserver(([entry]) => {
    visible = entry?.isIntersecting ?? false;
    synchronize();
  });
  const dimensions = new ResizeObserver(resize);
  resize();
  visibility.observe(hero);
  dimensions.observe(hero);
  for (const element of protectedElements) dimensions.observe(element);
  document.addEventListener("visibilitychange", synchronize);
  motion.addEventListener("change", synchronize);

  dispose = () => {
    cancelAnimationFrame(frame);
    visibility.disconnect();
    dimensions.disconnect();
    document.removeEventListener("visibilitychange", synchronize);
    motion.removeEventListener("change", synchronize);
  };
});

onBeforeUnmount(() => dispose?.());
</script>

<template>
  <canvas ref="canvas" class="ambient-particles" aria-hidden="true" />
</template>

<style scoped>
.ambient-particles {
  position: absolute;
  inset: 0;
  z-index: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}

@media (prefers-reduced-motion: reduce) {
  .ambient-particles {
    display: none;
  }
}
</style>
