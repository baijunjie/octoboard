<script setup lang="ts">
defineProps<{ paused?: boolean }>();
const config = useRuntimeConfig();
const surface = ref<HTMLElement | null>(null);
const running = useVisibleMotion(surface);
const id = useId();
const source = `${config.app.baseURL}logo-640.png`;
const headShape =
  "M320 65 410 108 485 206 493 315 441 378 415 398 377 428 365 454 320 455 274 454 263 428 224 398 198 378 147 315 155 206 230 108Z";
// Overlapping joints sit beneath the intact head, keeping the source artwork's silhouette.
const arms = [
  {
    points: "0,0 280,0 230,145 192,218 207,332 124,334 0,280",
    origin: "194px 292px",
    angle: "-8deg",
    delay: "-1s",
  },
  {
    points: "640,0 360,0 410,145 448,218 433,332 516,334 640,280",
    origin: "446px 292px",
    angle: "8deg",
    delay: "-3.5s",
  },
  {
    points: "0,280 123,280 209,327 258,380 205,419 0,419",
    origin: "224px 366px",
    angle: "15deg",
    delay: "-2.5s",
  },
  {
    points: "640,280 517,280 431,327 382,380 435,419 640,419",
    origin: "416px 366px",
    angle: "-15deg",
    delay: "-0.5s",
  },
  {
    points: "0,419 170,410 231,365 277,411 221,492 170,509 164,640 0,640",
    origin: "244px 415px",
    angle: "-12deg",
    delay: "-4.5s",
  },
  {
    points: "640,419 470,410 409,365 363,411 419,492 470,509 476,640 640,640",
    origin: "396px 415px",
    angle: "12deg",
    delay: "-2s",
  },
  {
    points: "164,640 164,511 232,491 259,415 320,415 320,640",
    origin: "286px 438px",
    angle: "15deg",
    delay: "-3s",
  },
  {
    points: "476,640 476,511 408,491 381,415 320,415 320,640",
    origin: "354px 438px",
    angle: "-15deg",
    delay: "-5s",
  },
];
</script>

<template>
  <div
    ref="surface"
    class="hero-mascot"
    :class="{ running: running && !paused }"
    aria-hidden="true"
  >
    <div class="mascot-orbit orbit-outer" />
    <div class="mascot-orbit orbit-inner" />
    <div class="mascot-aura" />
    <svg class="mascot-art" viewBox="-32 -32 704 704" fill="none">
      <defs>
        <clipPath
          v-for="(arm, index) in arms"
          :id="`${id}-arm-${index}`"
          :key="index"
        >
          <polygon :points="arm.points" />
        </clipPath>
        <clipPath :id="`${id}-head`">
          <path :d="headShape" />
        </clipPath>
        <mask
          :id="`${id}-outside-head`"
          maskUnits="userSpaceOnUse"
          x="0"
          y="0"
          width="640"
          height="640"
        >
          <rect width="640" height="640" fill="white" />
          <path :d="headShape" fill="black" />
        </mask>
        <radialGradient :id="`${id}-eye`">
          <stop stop-color="#ff6a42" stop-opacity=".8" />
          <stop offset=".45" stop-color="#ff3529" stop-opacity=".35" />
          <stop offset="1" stop-color="#ff3529" stop-opacity="0" />
        </radialGradient>
      </defs>
      <g class="mascot-float">
        <g
          v-for="(arm, index) in arms"
          :key="index"
          class="mascot-arm"
          :style="{
            transformOrigin: arm.origin,
            '--arm-angle': arm.angle,
            animationDelay: arm.delay,
          }"
        >
          <image
            :href="source"
            width="640"
            height="640"
            :clip-path="`url(#${id}-arm-${index})`"
            :mask="`url(#${id}-outside-head)`"
          />
        </g>
        <image
          :href="source"
          width="640"
          height="640"
          :clip-path="`url(#${id}-head)`"
        />
        <g class="mascot-eyes" :fill="`url(#${id}-eye)`">
          <circle cx="249" cy="296" r="38" />
          <circle cx="391" cy="296" r="38" />
        </g>
      </g>
    </svg>
    <span class="orbit-point point-one" />
    <span class="orbit-point point-two" />
  </div>
</template>

<style scoped>
.hero-mascot {
  position: relative;
  isolation: isolate;
  width: 100%;
  aspect-ratio: 1;
}
.mascot-art {
  position: relative;
  z-index: 2;
  width: 100%;
  height: 100%;
  overflow: visible;
}
.mascot-aura {
  position: absolute;
  inset: 17%;
  z-index: -1;
  background: radial-gradient(ellipse, #ff493b12, transparent 68%);
}
.mascot-orbit {
  position: absolute;
  border: 1px solid #ffffff0c;
  border-radius: 50%;
  pointer-events: none;
}
.orbit-outer {
  inset: 3%;
  border-style: dashed;
  transform: rotate(-18deg) scaleY(0.87);
}
.orbit-inner {
  inset: 11%;
  transform: rotate(25deg) scaleY(0.88);
  border-color: #ffffff10;
}
.orbit-point {
  position: absolute;
  width: 4px;
  height: 4px;
  background: #ff4b3e;
  border-radius: 50%;
  box-shadow: 0 0 14px #ff4b3e66;
}
.point-one {
  left: 12%;
  top: 24%;
}
.point-two {
  right: 10%;
  bottom: 25%;
  background: #c5c5c5;
}
.mascot-float {
  animation: mascot-float 5s ease-in-out infinite;
}
.mascot-arm {
  animation: arm-articulation 3.8s ease-in-out infinite alternate;
}
.mascot-eyes {
  animation: eye-breathe 3.6s ease-in-out infinite;
  mix-blend-mode: screen;
}
.mascot-float,
.mascot-arm,
.mascot-eyes {
  animation-play-state: paused;
}
.running .mascot-float,
.running .mascot-arm,
.running .mascot-eyes {
  animation-play-state: running;
}
@keyframes arm-articulation {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(var(--arm-angle));
  }
}
@keyframes mascot-float {
  0%,
  100% {
    transform: translateY(14px) rotate(-2deg);
  }
  50% {
    transform: translateY(-22px) rotate(2deg);
  }
}
@keyframes eye-breathe {
  0%,
  100% {
    opacity: 0.2;
  }
  50% {
    opacity: 0.75;
  }
}
@media (prefers-reduced-motion: reduce) {
  .mascot-float,
  .mascot-arm,
  .mascot-eyes {
    animation: none;
  }
  .mascot-eyes {
    opacity: 0.25;
  }
}
</style>
