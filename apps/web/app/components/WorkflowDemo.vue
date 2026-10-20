<script setup lang="ts">
import {
  workflowNodes,
  workflowProjects as projects,
} from "../../seo/page-outline";

defineProps<{ paused?: boolean }>();
const { t } = useSiteCopy();
const { locale } = useI18n();
const config = useRuntimeConfig();
const id = useId();
const stage = ref<HTMLElement | null>(null);
const center = ref<HTMLElement | null>(null);
const nodeElements = ref<HTMLElement[]>([]);
const running = useVisibleMotion(stage);
const selected = ref(0);
const measured = ref(false);
const positions = [
  [29, 13],
  [71, 13],
  [13, 37],
  [87, 37],
  [13, 67],
  [87, 67],
  [29, 90],
  [71, 90],
] as const;
const nodes = workflowNodes.map((node, index) => ({
  ...node,
  x: positions[index]![0],
  y: positions[index]![1],
}));
function curve(cx: number, cy: number, x: number, y: number) {
  const dx = x - cx;
  const dy = y - cy;
  return `M${cx} ${cy}C${cx + dx * 0.15 - dy * 0.1} ${cy + dy * 0.55 + dx * 0.1} ${x - dx * 0.4} ${y - dy * 0.05} ${x} ${y}`;
}
const diagram = ref({
  width: 1000,
  height: 640,
  paths: nodes.map((node) => curve(500, 320, node.x * 10, node.y * 6.4)),
});
const activeProject = computed(() => nodes[selected.value]!.project.key);
let observer: ResizeObserver | undefined;

function measure() {
  if (!stage.value || !center.value) return;
  const bounds = stage.value.getBoundingClientRect();
  if (!bounds.width || !bounds.height) return;
  const origin = center.value.getBoundingClientRect();
  const cx = origin.left + origin.width / 2 - bounds.left;
  const cy = origin.top + origin.height / 2 - bounds.top;
  const stacked = matchMedia("(max-width: 760px)").matches;
  const paths = nodes.map((_, index) => {
    const element = nodeElements.value.find(
      (node) => Number(node.dataset.index) === index,
    );
    if (!element) return "";
    const rect = element.getBoundingClientRect();
    const x = rect.left + rect.width / 2 - bounds.left;
    const y = rect.top + rect.height / 2 - bounds.top;
    return stacked
      ? `M${cx} ${cy}C${cx} ${y} ${cx} ${y} ${x} ${y}`
      : curve(cx, cy, x, y);
  });
  diagram.value = { width: bounds.width, height: bounds.height, paths };
  measured.value = true;
}
watch(locale, () => nextTick(measure), { flush: "post" });
onMounted(() => {
  measure();
  observer = new ResizeObserver(measure);
  if (stage.value) observer.observe(stage.value);
  for (const element of nodeElements.value) observer.observe(element);
});
onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <div
    class="workflow-board"
    :class="{ running: running && !paused }"
    role="group"
    :aria-label="t('demoLabel')"
  >
    <div class="workflow-heading">
      <span class="workflow-caption"
      ><SiteIcon name="tree-structure" />{{ t("demoWorkspace") }}</span
      >
      <span>{{ t("demoIllustration") }}</span>
    </div>
    <div class="workflow-request">
      <span class="workflow-index" aria-hidden="true">01</span>
      <div>
        <span class="workflow-caption">{{ t("demoYou") }}</span>
        <p>{{ t("demoRequest") }}</p>
      </div>
    </div>
    <div ref="stage" class="constellation" dir="ltr">
      <svg
        class="network-layer"
        :class="{ measured }"
        :viewBox="`0 0 ${diagram.width} ${diagram.height}`"
        aria-hidden="true"
      >
        <g
          v-for="(path, index) in diagram.paths"
          :key="index"
          :class="{ 'selected-link': selected === index }"
        >
          <path :d="path" class="network-track" />
          <path
            :d="path"
            class="network-dispatch"
            pathLength="1"
            :style="{ animationDelay: `${index * -0.7}s` }"
          />
          <path
            :d="path"
            class="network-report"
            pathLength="1"
            :style="{ animationDelay: `${index * -0.7 - 2.8}s` }"
          />
        </g>
      </svg>
      <div ref="center" class="command-core" dir="auto">
        <div class="core-orbit" aria-hidden="true" />
        <span class="core-index" aria-hidden="true">02</span>
        <img
          :src="`${config.app.baseURL}logo-640.png`"
          alt=""
          width="150"
          height="150"
          loading="lazy"
        />
        <strong>{{ t("demoCoordinator") }}</strong>
        <p>{{ t("demoCoordinatorText") }}</p>
      </div>
      <div class="agent-nodes">
        <!-- These controls belong to the Vue marketing site, outside the React HeroUI application. -->
        <button
          v-for="(node, index) in nodes"
          :id="`${id}-node-${index}`"
          :key="node.path"
          ref="nodeElements"
          :data-index="index"
          type="button"
          class="agent-node"
          :class="{ selected: selected === index }"
          :style="{ '--node-x': `${node.x}%`, '--node-y': `${node.y}%` }"
          :aria-pressed="selected === index"
          :aria-controls="`${id}-detail-${node.project.key}`"
          @click="selected = index"
        >
          <span class="node-port" aria-hidden="true" />
          <span class="node-brand"
          ><img
            :src="`${config.app.baseURL}agents/${node.project.icon}.svg`"
            alt=""
            width="20"
            height="20"
          /><strong>{{ node.project.agent }}</strong
          ><span class="node-arrow" aria-hidden="true">↗</span></span
          >
          <span class="node-project" dir="auto">{{
            t(`demo${node.project.key}`)
          }}</span>
          <span class="node-path" dir="ltr">{{ node.path }}</span>
        </button>
      </div>
    </div>
    <div class="project-details">
      <article
        v-for="project in projects"
        v-show="activeProject === project.key"
        :id="`${id}-detail-${project.key}`"
        :key="project.key"
        class="project-detail"
        :aria-labelledby="`${id}-detail-heading-${project.key}`"
      >
        <div class="detail-context">
          <h3 :id="`${id}-detail-heading-${project.key}`">
            {{ t(`demo${project.key}`) }}
          </h3>
          <span class="detail-path" dir="ltr">{{ nodes[selected]!.path }}</span>
          <span>{{ t("demoContext") }}</span>
          <p>{{ t("demoRules") }}</p>
        </div>
        <div class="detail-task">
          <span class="detail-label">{{ project.agent }}</span>
          <p>{{ t(`demo${project.key}Task`) }}</p>
        </div>
        <div class="detail-report">
          <span class="detail-label"
          ><SiteIcon name="chat-circle-dots" />{{
            t("demoReportLabel")
          }}</span
          >
          <p>{{ t(`demo${project.key}Report`) }}</p>
        </div>
      </article>
    </div>
    <div class="workflow-summary">
      <span class="workflow-index" aria-hidden="true">03</span>
      <div>
        <h3>{{ t("demoReportsTitle") }}</h3>
        <p>{{ t("demoReportsDescription") }}</p>
      </div>
      <SiteIcon name="plugs-connected" />
      <div class="summary-delivery">
        <h3>{{ t("demoDeliveryTitle") }}</h3>
        <p>{{ t("demoDeliveryDescription") }}</p>
      </div>
    </div>
    <div class="workflow-legend">
      <span
      ><i class="dispatch-swatch" aria-hidden="true" />{{
        t("demoDispatch")
      }}</span
      >
      <span
      ><i class="report-swatch" aria-hidden="true" />{{
        t("demoReportLabel")
      }}</span
      >
      <WorkflowStatus status="idle" :animating="false" />
      <WorkflowStatus status="background" :animating="running && !paused" />
      <WorkflowStatus status="working" :animating="running && !paused" />
    </div>
    <p class="workflow-footer">
      <SiteIcon name="chat-circle-dots" />{{ t("demoDirect") }}
    </p>
  </div>
</template>

<style scoped>
.workflow-board {
  --network-background: #101112;
  position: relative;
  background: var(--network-background);
  border: 1px solid #ffffff20;
  overflow: hidden;
}
.workflow-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 20px 28px;
  border-bottom: 1px solid #ffffff16;
  color: #a7a7ab;
  font-size: 12px;
  line-height: 1.5;
}
.workflow-caption {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  color: #bfc0c3;
  font-size: 12px;
  letter-spacing: 0.04em;
}
.workflow-caption .site-icon {
  width: 16px;
  height: 16px;
}
.workflow-request {
  display: flex;
  gap: 24px;
  align-items: center;
  padding: 34px 40px 0;
}
.workflow-index {
  color: #8b8c91;
  font-family: var(--font-mono, monospace);
  font-size: 42px;
  line-height: 1;
  font-weight: 300;
  letter-spacing: -0.08em;
  flex-shrink: 0;
}
.workflow-request p {
  color: #f0f0f2;
  font-size: clamp(18px, 2.1vw, 25px);
  line-height: 1.5;
  margin-top: 8px;
}
.constellation {
  position: relative;
  min-height: 640px;
  background: radial-gradient(ellipse at center, #ffffff05, transparent 55%);
}
.constellation::before {
  content: "";
  position: absolute;
  width: 42%;
  aspect-ratio: 1;
  border-radius: 50%;
  border: 1px dashed #ffffff0b;
  left: 29%;
  top: 50%;
  transform: translateY(-50%);
  pointer-events: none;
}
.network-layer {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}
.network-track {
  fill: none;
  stroke: #ffffff26;
  stroke-width: 1;
}
.selected-link .network-track {
  stroke: #ff4b3e66;
}
.network-dispatch,
.network-report {
  fill: none;
  stroke-width: 1.5;
  stroke-dasharray: 0.065 1;
  stroke-dashoffset: 1.065;
  animation: network-signal 7s linear infinite;
  animation-play-state: paused;
}
.network-dispatch {
  stroke: #e1e1e8;
}
.network-report {
  stroke: #ff675b;
  animation-direction: reverse;
  stroke-width: 1;
}
.running .network-dispatch,
.running .network-report {
  animation-play-state: running;
}
.command-core {
  position: absolute;
  z-index: 1;
  width: 240px;
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  text-align: center;
  padding: 12px 12px 24px;
  background: radial-gradient(ellipse, #101112 55%, #10111200 72%);
}
.command-core img {
  display: block;
  width: 150px;
  height: 150px;
  margin: 0 auto 4px;
  position: relative;
}
.command-core strong {
  color: #f5f5f5;
  font-size: 16px;
  line-height: 1.6;
  position: relative;
}
.command-core p {
  font-size: 12px;
  line-height: 1.8;
  color: #afb0b5;
  margin-top: 9px;
  position: relative;
}
.core-orbit {
  position: absolute;
  width: 200px;
  height: 200px;
  top: -6px;
  left: 20px;
  border: 1px solid #ff4b3e25;
  border-radius: 50%;
}
.core-orbit::before,
.core-orbit::after {
  content: "";
  position: absolute;
  inset: -12px;
  border: 1px solid #ffffff0a;
  border-radius: 50%;
}
.core-orbit::after {
  inset: -26px;
  border-style: dashed;
}
.core-index {
  position: absolute;
  inset-inline-end: 10px;
  top: 2px;
  color: #96969b;
  font: 24px/1.2 monospace;
}
.agent-node {
  position: absolute;
  z-index: 2;
  left: var(--node-x);
  top: var(--node-y);
  transform: translate(-50%, -50%);
  width: 190px;
  padding: 15px 18px;
  color: #e1e1e5;
  border: 1px solid #63636a;
  border-radius: 5px;
  background: linear-gradient(120deg, #1b1c1f, #141517);
  text-align: start;
  cursor: pointer;
  transition:
    border-color 180ms,
    background 180ms;
}
.agent-node:hover {
  border-color: #a0a1a6;
  background: #202124;
}
.agent-node.selected {
  border-color: #ff786c;
  background: linear-gradient(120deg, #272021, #171719);
}
.agent-node:focus {
  outline: 2px solid #f1f1f1;
  outline-offset: 5px;
}
.node-port {
  position: absolute;
  width: 5px;
  height: 5px;
  background: #9d9fa6;
  border-radius: 50%;
  left: -3px;
  top: calc(50% - 2px);
  box-shadow: 0 0 0 4px #101112;
}
.selected .node-port {
  background: #ff786c;
}
.node-brand {
  display: flex;
  align-items: center;
  gap: 8px;
}
.node-brand img {
  flex-shrink: 0;
}
.node-brand strong {
  font-size: 13px;
  line-height: 1.5;
  font-weight: 500;
}
.node-arrow {
  color: #b8b8c0;
  margin-inline-start: auto;
  font-size: 16px;
}
.node-project {
  display: block;
  color: #eeeef0;
  font-size: 14px;
  line-height: 1.6;
  margin-top: 12px;
}
.node-path {
  display: block;
  color: #9d9fa6;
  font-family: var(--font-mono, monospace);
  font-size: 11px;
  line-height: 1.7;
  overflow-wrap: anywhere;
}
.project-details {
  margin: 10px 28px 0;
  padding: 24px 0;
  border-block: 1px solid #ffffff20;
}
.project-detail {
  display: grid;
  grid-template-columns: 0.85fr 1fr 1.2fr;
  gap: 30px;
}
.detail-context h3 {
  font-size: 17px;
  font-weight: 500;
  line-height: 1.5;
}
.detail-context > span {
  display: block;
  margin-top: 6px;
  color: #aaaab0;
  font-size: 12px;
  line-height: 1.6;
}
.detail-context .detail-path {
  color: #d1d1d5;
  font-family: var(--font-mono, monospace);
}
.detail-context p {
  color: #aaaab0;
  font-size: 12px;
  line-height: 1.8;
  margin-top: 12px;
}
.detail-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  line-height: 1.8;
  color: #c0c0c6;
}
.detail-label .site-icon {
  width: 16px;
  height: 16px;
  color: #ff867b;
}
.detail-task p,
.detail-report p {
  margin-top: 12px;
  color: #e0e0e5;
  font-size: 14px;
  line-height: 1.8;
}
.detail-report {
  border-inline-start: 1px solid #ff4b3e40;
  padding-inline-start: 24px;
}
.workflow-summary {
  display: flex;
  align-items: flex-start;
  gap: 24px;
  padding: 30px 28px;
}
.workflow-summary > div {
  flex: 1;
  min-width: 0;
}
.workflow-summary h3 {
  color: #e1e1e5;
  font-size: 15px;
  line-height: 1.6;
  font-weight: 500;
}
.workflow-summary p {
  color: #a8a8ae;
  font-size: 12px;
  line-height: 1.8;
  margin-top: 8px;
}
.workflow-summary > .site-icon {
  color: #aaaab0;
  margin-top: 5px;
  flex-shrink: 0;
}
.workflow-footer {
  display: flex;
  gap: 12px;
  align-items: center;
  color: #babac0;
  font-size: 12px;
  line-height: 1.7;
  padding: 18px 28px;
  background: #ffffff04;
  border-top: 1px solid #ffffff0c;
}
.workflow-footer .site-icon {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
}
.workflow-legend {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 16px 24px;
  padding: 0 28px 24px;
  color: #b9b9bf;
  font-size: 12px;
  line-height: 1.6;
}
.workflow-legend > span {
  display: flex;
  align-items: center;
  gap: 8px;
}
.workflow-legend i {
  display: inline-block;
  width: 24px;
  height: 1px;
  background: #e1e1e8;
  position: relative;
}
.workflow-legend i::after {
  content: "";
  position: absolute;
  right: 0;
  top: -2px;
  width: 5px;
  height: 5px;
  border-top: 1px solid currentColor;
  border-right: 1px solid currentColor;
  transform: rotate(45deg);
}
.workflow-legend .report-swatch {
  background: #ff867b;
  color: #ff867b;
  transform: rotate(180deg);
}
@keyframes network-signal {
  to {
    stroke-dashoffset: -1.065;
  }
}
@media (max-width: 1050px) and (min-width: 761px) {
  .agent-node {
    width: 160px;
    padding: 12px;
  }
  .command-core {
    width: 210px;
  }
  .core-orbit {
    left: 5px;
  }
  .constellation {
    min-height: 600px;
  }
}
@media (max-width: 760px) {
  .network-layer:not(.measured) {
    display: none;
  }
  .workflow-heading {
    padding: 16px 20px;
    flex-wrap: wrap;
    gap: 8px;
  }
  .workflow-request {
    padding: 28px 20px 0;
    gap: 20px;
  }
  .constellation {
    min-height: 0;
    padding: 24px 20px 16px;
  }
  .constellation::before {
    width: 260px;
    left: calc(50% - 130px);
    top: 140px;
  }
  .command-core {
    position: relative;
    left: auto;
    top: auto;
    transform: none;
    width: 240px;
    margin: 0 auto 38px;
  }
  .agent-nodes {
    position: relative;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 24px 32px;
  }
  .agent-node {
    position: relative;
    width: auto;
    left: auto;
    top: auto;
    transform: none;
    padding: 14px;
  }
  .agent-node:nth-child(odd) .node-port {
    left: auto;
    right: -3px;
  }
  .node-brand {
    gap: 6px;
    flex-wrap: wrap;
  }
  .node-brand strong {
    font-size: 12px;
  }
  .node-arrow {
    display: none;
  }
  .project-details {
    margin-inline: 20px;
  }
  .project-detail {
    grid-template-columns: 1fr;
    gap: 24px;
  }
  .detail-context p {
    margin-top: 8px;
  }
  .detail-report {
    padding-inline-start: 16px;
  }
  .workflow-summary {
    display: grid;
    grid-template-columns: 44px 1fr;
    gap: 22px 20px;
    padding: 28px 20px;
  }
  .workflow-summary > .site-icon {
    margin-inline: auto;
  }
  .workflow-footer {
    padding: 18px 20px;
  }
}
@media (max-width: 380px) {
  .agent-nodes {
    gap: 20px 24px;
  }
  .agent-node {
    padding: 12px 10px;
  }
  .node-brand img {
    width: 18px;
    height: 18px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .network-dispatch,
  .network-report {
    animation: none;
    display: none;
  }
  .agent-node {
    transition: none;
  }
}
</style>
