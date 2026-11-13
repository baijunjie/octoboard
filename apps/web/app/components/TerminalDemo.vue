<script setup lang="ts">
import { defaultServerPort as port, installCommand, linuxCommandParts } from "../../seo/downloads";

// public/demo-workspace.webp, the picture in the browser window, is the site's own product preview
// (public/preview/website-preview.html?scene=overview&lang=en, as built by `pnpm --filter @octoboard/web build`)
// captured with Playwright at a 1280x800 viewport and scale 1, resized to 960x600 with Pillow (Lanczos) and
// encoded with `cwebp -q 55 -m 6` (about 52 KB). Retake it the same way when the preview changes.
//
// Two terminal windows and a browser window, played step by step. The commands and their output are lines the real
// tools print (install.sh's are held to the script by apps/web/test/install-script.test.ts; a warning it prints on
// stderr is left out), so they stay in English and read left to right whatever the page language; only the step
// labels are translated.
type Line = {
  window: 0 | 1;
  kind: "cmd" | "out" | "prompt";
  text: string;
  prompt?: string;
};
type Step =
  | { kind: "show"; win: 1 | 2 }
  | { kind: "type"; line: number }
  | { kind: "print"; lines: number[] }
  | { kind: "cursor"; line: number }
  | { kind: "wait"; n: number };

const { t } = useSiteCopy();
const config = useRuntimeConfig();
const windows = [
  { title: "you@server: ~", prompt: "you@server:~$", caption: "downloadDemoServerCaption" },
  { title: "you@laptop: ~", prompt: "you@laptop:~$", caption: "downloadDemoLocalCaption" },
] as const;
const address = `127.0.0.1:${port}/?key=3f9c…`;
// The forward `octoboardd address` prints for a user `you`, and the one the laptop window types: the same with the
// server named and without `-N`, so the session opens the shell the demo shows.
const socket = "/home/you/.octoboard/browser.sock";
const printedForward = `ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:${port}:${socket} you@<server>`;
const forward = `ssh -o ExitOnForwardFailure=yes -L 127.0.0.1:${port}:${socket} you@server`;
const lines: Line[] = [
  { window: 0, kind: "cmd", text: installCommand(config.public.siteUrl) },
  { window: 0, kind: "out", text: "Downloading octoboardd 0.1.0 for x86_64..." },
  { window: 0, kind: "out", text: "Installed octoboardd 0.1.0 to /home/you/.local/bin/octoboardd." },
  {
    window: 0,
    kind: "out",
    text: `Start it with \`octoboardd start\` (port ${port}; \`--port N\` picks another), then run \`octoboardd address\`: it prints the address to open and the SSH forward to open it through.`,
  },
  { window: 0, kind: "cmd", text: linuxCommandParts.start },
  { window: 0, kind: "out", text: `octoboardd listening on 127.0.0.1:${port} with key 3f9c…` },
  { window: 0, kind: "out", text: `Octoboard started: pid 4242, http://127.0.0.1:${port}` },
  { window: 0, kind: "cmd", text: linuxCommandParts.address },
  { window: 0, kind: "out", text: `http://127.0.0.1:${port}/?key=3f9c…` },
  {
    window: 0,
    kind: "out",
    text: `forward it to your own machine with \`${printedForward}\`, then open the address there`,
  },
  { window: 0, kind: "prompt", text: "" },
  { window: 1, kind: "cmd", text: forward },
  // The shell the SSH session opens on the server.
  { window: 1, kind: "prompt", text: "", prompt: "you@server:~$" },
];
const steps: Step[] = [
  { kind: "wait", n: 10 },
  { kind: "type", line: 0 },
  { kind: "wait", n: 14 },
  { kind: "print", lines: [1] },
  { kind: "wait", n: 36 },
  { kind: "print", lines: [2, 3] },
  { kind: "wait", n: 16 },
  { kind: "type", line: 4 },
  { kind: "wait", n: 12 },
  { kind: "print", lines: [5, 6] },
  { kind: "wait", n: 18 },
  { kind: "type", line: 7 },
  { kind: "wait", n: 10 },
  { kind: "print", lines: [8, 9, 10] },
  { kind: "cursor", line: 10 },
  { kind: "wait", n: 30 },
  { kind: "show", win: 1 },
  { kind: "wait", n: 14 },
  { kind: "type", line: 11 },
  { kind: "wait", n: 20 },
  { kind: "print", lines: [12] },
  { kind: "cursor", line: 12 },
  { kind: "wait", n: 24 },
  { kind: "show", win: 2 },
];
const windowLines = windows.map((_, w) =>
  lines.map((line, index) => ({ line, index })).filter(({ line }) => line.window === w),
);

// Rendered complete, so that without scripting or with reduced motion the final state is what shows.
const typed = ref(lines.map((line) => (line.kind === "cmd" ? line.text.length : 0)));
const shown = ref(lines.map(() => true));
const visible = ref([true, true, true]);
const cursor = ref(-1);
const playing = ref(false);
const reduced = ref(false);
const root = ref<HTMLElement | null>(null);
let timer: ReturnType<typeof setInterval> | undefined;
let observer: IntersectionObserver | undefined;
let inView = false;
let step = 0;
let waited = 0;
let chars = 0;
let lead = 0;
let delay = 0;

// The clock runs only while the demo plays, is on screen and its tab is visible; the position is kept in between.
function sync() {
  const run = playing.value && inView && !document.hidden;
  if (run && timer === undefined) timer = setInterval(tick, 30);
  else if (!run && timer !== undefined) {
    clearInterval(timer);
    timer = undefined;
  }
}
function advance() {
  step++;
  waited = 0;
  chars = 0;
  lead = 7;
  delay = 0;
}
// A human pace: uneven gaps, longer after a space or a symbol.
function gap(char: string | undefined) {
  const base = 1 + Math.floor(Math.random() * 3);
  return /[\s\-/.:|]/.test(char ?? "") && Math.random() < 0.5 ? base + 2 : base;
}
function tick() {
  const current = steps[step];
  if (!current) {
    playing.value = false;
    sync();
    return;
  }
  if (current.kind === "wait") {
    if (++waited >= current.n) advance();
  } else if (current.kind === "show") {
    visible.value[current.win] = true;
    advance();
  } else if (current.kind === "cursor") {
    cursor.value = current.line;
    advance();
  } else if (current.kind === "print") {
    for (const index of current.lines) shown.value[index] = true;
    cursor.value = -1;
    advance();
  } else {
    const text = lines[current.line]?.text ?? "";
    cursor.value = current.line;
    if (chars === 0 && lead > 0) {
      lead--;
      return;
    }
    if (delay > 0) {
      delay--;
      return;
    }
    chars++;
    typed.value[current.line] = chars;
    delay = gap(text[chars - 1]);
    if (chars >= text.length) advance();
  }
}
function play() {
  typed.value = lines.map(() => 0);
  shown.value = lines.map(() => false);
  visible.value = [true, false, false];
  cursor.value = -1;
  step = 0;
  waited = 0;
  chars = 0;
  lead = 7;
  delay = 0;
  playing.value = true;
  sync();
}
onMounted(() => {
  reduced.value = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduced.value) return;
  if ("IntersectionObserver" in window && root.value) {
    observer = new IntersectionObserver(([entry]) => {
      inView = entry?.isIntersecting ?? true;
      sync();
    });
    observer.observe(root.value);
  } else {
    inView = true;
  }
  document.addEventListener("visibilitychange", sync);
  play();
});
onBeforeUnmount(() => {
  playing.value = false;
  sync();
  observer?.disconnect();
  document.removeEventListener("visibilitychange", sync);
});
</script>

<template>
  <div ref="root" class="demo">
    <p class="sr-only">{{ t("downloadDemoSummary") }}</p>
    <div class="demo-stage" dir="ltr">
      <div
        v-for="(win, w) in windows"
        :key="win.title"
        class="demo-window"
        :class="[`is-terminal-${w}`, { 'is-away': !visible[w] }]"
      >
        <div class="demo-bar">
          <span class="demo-dots" aria-hidden="true"><i /><i /><i /></span>
          <span class="demo-title" :class="{ 'has-replay': w === 0 && !reduced }" aria-hidden="true">{{ win.title }}</span>
          <button
            v-if="w === 0 && !reduced"
            class="demo-replay"
            type="button"
            :aria-label="t('downloadDemoReplayLabel')"
            :title="t('downloadDemoReplayLabel')"
            @click="play"
          >
            <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
              <path d="M3 8a5 5 0 1 0 1.6-3.7M3 2.5v3h3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </button>
        </div>
        <p class="demo-step" aria-hidden="true"><span class="demo-step-number">{{ String(w + 1).padStart(2, "0") }}</span>{{ t(win.caption) }}</p>
        <div class="demo-terminal" aria-hidden="true">
          <div v-for="{ line, index: i } in windowLines[w]" :key="i" class="demo-line" :class="`is-${line.kind}`">
            <template v-if="line.kind !== 'out'">
              <span class="demo-prompt" :class="{ 'is-waiting': line.kind === 'cmd' ? typed[i] === 0 && cursor !== i : !shown[i] }">{{ line.prompt ?? win.prompt }}</span>
              <span>{{ line.text.slice(0, typed[i]) }}</span><span v-if="cursor === i" class="demo-cursor" /><span class="demo-rest">{{ line.text.slice(typed[i]) }}</span>
            </template>
            <span v-else :class="{ 'demo-rest': !shown[i] }">{{ line.text }}</span>
          </div>
        </div>
      </div>
      <div class="demo-window is-browser" :class="{ 'is-away': !visible[2] }" aria-hidden="true">
        <div class="demo-bar">
          <span class="demo-dots"><i /><i /><i /></span>
          <span class="demo-address">{{ address }}</span>
        </div>
        <img
          :src="`${config.app.baseURL}demo-workspace.webp`"
          alt=""
          width="960"
          height="600"
          loading="lazy"
          decoding="async"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.demo-stage {
  display: grid;
  max-width: 100%;
  padding-bottom: 8px;
}
.demo-window {
  position: relative;
  min-width: 0;
  overflow: hidden;
  border: 1px solid #2c2c2e;
  border-radius: 10px;
  background: #0d0d0f;
  box-shadow: 0 24px 48px -16px rgba(0, 0, 0, 0.8);
  transition:
    opacity 400ms var(--ease),
    transform 400ms var(--ease);
}
.demo-window.is-away {
  opacity: 0;
  transform: translateY(14px) scale(0.98);
}
.is-terminal-1 {
  z-index: 1;
  width: 94%;
  margin: -20px 0 0 6%;
}
.is-browser {
  z-index: 2;
  width: 82%;
  margin: -30px 0 0 18%;
}
.demo-bar {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 36px;
  padding: 0 12px;
  background: #1f1f22;
  border-bottom: 1px solid #2c2c2e;
}
.demo-dots {
  display: flex;
  gap: 7px;
}
.demo-dots i {
  width: 11px;
  height: 11px;
  border-radius: 50%;
  background: #ff5f57;
}
.demo-dots i:nth-child(2) {
  background: #febc2e;
}
.demo-dots i:nth-child(3) {
  background: #28c840;
}
.demo-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: #9a9a9f;
  font-size: 12px;
  text-align: center;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.demo-title.has-replay {
  padding-inline-end: 54px;
}
.demo-replay {
  position: absolute;
  inset-inline-end: 0;
  top: 0;
  display: grid;
  width: 44px;
  height: 36px;
  place-items: center;
  border: 0;
  background: none;
  color: #9a9a9f;
  cursor: pointer;
  transition: color 200ms;
}
.demo-replay:hover {
  color: var(--text);
}
.demo-replay:focus-visible {
  outline: 2px solid var(--red);
  outline-offset: -4px;
}
.demo-step {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 14px;
  border-bottom: 1px solid #1c1c1f;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.5;
  text-align: start;
}
.demo-step-number {
  flex-shrink: 0;
  color: #929292;
  font-weight: 400;
  font-variant-numeric: tabular-nums;
}
.demo-terminal {
  padding: 14px 16px 34px;
  color: #e4e4e6;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 13px;
  line-height: 1.65;
  text-align: left;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.demo-prompt {
  margin-inline-end: 0.7em;
  color: #7fd58f;
}
.demo-prompt.is-waiting,
.demo-rest {
  visibility: hidden;
}
.is-out {
  color: #a8a8ad;
}
.demo-cursor {
  position: relative;
}
.demo-cursor::after {
  content: "";
  position: absolute;
  top: 0.2em;
  left: 0;
  width: 0.6em;
  height: 1.15em;
  background: #e4e4e6;
  animation: demo-blink 1.06s steps(1) infinite;
}
.is-browser img {
  display: block;
  width: 100%;
  height: auto;
  background: #000;
}
.demo-address {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  padding: 4px 12px;
  border-radius: 6px;
  background: #0d0d0f;
  color: #b8b8bd;
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
@keyframes demo-blink {
  50% {
    opacity: 0;
  }
}
@media (max-width: 600px) {
  .is-terminal-1 {
    width: 96%;
    margin: -16px 0 0 4%;
  }
  .is-browser {
    width: 90%;
    margin: -26px 0 0 10%;
  }
  .demo-terminal {
    padding: 12px 12px 30px;
    font-size: 12px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .demo-window {
    transition: none;
  }
  .demo-cursor::after {
    animation: none;
  }
}
</style>
