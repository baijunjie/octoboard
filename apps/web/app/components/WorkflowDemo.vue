<script setup lang="ts">
const { t } = useSiteCopy();
const { locale } = useI18n();
const config = useRuntimeConfig();
const projects = [
  { key: "Frontend", agent: "Codex", icon: "codex", path: "apps/web" },
  {
    key: "Backend",
    agent: "Claude Code",
    icon: "claude",
    path: "services/api",
  },
  { key: "Docs", agent: "Grok Build", icon: "grok", path: "docs" },
] as const;
const diagramId = useId();
const markerId = `workflow-arrow-${diagramId}`;
const body = ref<HTMLElement | null>(null);
const request = ref<HTMLElement | null>(null);
const coordinator = ref<HTMLElement | null>(null);
const summary = ref<HTMLElement | null>(null);
const delivery = ref<HTMLElement | null>(null);
const cards = ref<HTMLElement[]>([]);
const reports = ref<HTMLElement[]>([]);
const receipts = ref<HTMLElement[]>([]);
const intakeSignal = ref<SVGPathElement | null>(null);
const dispatchSignals = ref<SVGPathElement[]>([]);
const reportSignals = ref<SVGPathElement[]>([]);
const deliverySignal = ref<SVGPathElement | null>(null);
const projectStatus = ref<"working" | "idle">("idle");
const coordinatorStatus = ref<"working" | "idle">("idle");
const animating = ref(false);
const canReplay = ref(false);
const diagram = ref({
  width: 1,
  height: 1,
  intake: "",
  dispatch: [] as string[],
  reports: [] as string[],
  delivery: "",
});
let dimensions: ResizeObserver | undefined;
let visibility: IntersectionObserver | undefined;
let direction: MutationObserver | undefined;
let motion: MediaQueryList | undefined;
let inView = false;
let disposed = false;
let sequenceVersion = 0;
let animations: Animation[] = [];

function measureDiagram() {
  if (
    !body.value ||
    !request.value ||
    !coordinator.value ||
    !summary.value ||
    !delivery.value
  )
    return false;
  const root = body.value.getBoundingClientRect();
  if (!root.width || !root.height) return false;
  const relative = (element: HTMLElement) => {
    const rect = element.getBoundingClientRect();
    return {
      left: rect.left - root.left,
      right: rect.right - root.left,
      top: rect.top - root.top,
      bottom: rect.bottom - root.top,
      cx: rect.left - root.left + rect.width / 2,
      cy: rect.top - root.top + rect.height / 2,
    };
  };
  const intake = relative(request.value);
  const owner = relative(coordinator.value);
  const collector = relative(summary.value);
  const result = relative(delivery.value);
  const lanes = projects.flatMap((project) => {
    const card = cards.value.find(
      (element) => element.dataset.project === project.key,
    );
    const report = reports.value.find(
      (element) => element.dataset.project === project.key,
    );
    const receipt = receipts.value.find(
      (element) => element.dataset.project === project.key,
    );
    return card && report && receipt
      ? [
          {
            card: relative(card),
            report: relative(report),
            receipt: relative(receipt),
          },
        ]
      : [];
  });
  if (lanes.length !== projects.length) return false;
  const rtl = getComputedStyle(body.value).direction === "rtl";
  const stacked = matchMedia("(max-width: 900px)").matches;
  const branchY =
    (owner.bottom + Math.min(...lanes.map((lane) => lane.card.top))) / 2;
  const gatherY =
    (Math.max(...lanes.map((lane) => lane.card.bottom)) + collector.top) / 2;
  // Mobile uses two straight outside rails: dispatch enters cards, reports travel to the same coordinator below.
  const dispatchRail = rtl
    ? Math.max(...lanes.map((lane) => lane.card.right)) + 12
    : Math.min(...lanes.map((lane) => lane.card.left)) - 12;
  const reportRail = rtl
    ? Math.min(...lanes.map((lane) => lane.card.left)) - 12
    : Math.max(...lanes.map((lane) => lane.card.right)) + 12;
  const nextDiagram = {
    width: root.width,
    height: root.height,
    intake: `M${intake.cx} ${intake.bottom}V${owner.top}`,
    dispatch: lanes.map(({ card }) =>
      stacked
        ? `M${owner.cx} ${owner.bottom}V${branchY}H${dispatchRail}V${card.top + 32}H${rtl ? card.right : card.left}`
        : `M${owner.cx} ${owner.bottom}V${branchY}H${card.cx}V${card.top}`,
    ),
    reports: lanes.map(({ report, receipt }) =>
      stacked
        ? `M${rtl ? report.left : report.right} ${report.cy}H${reportRail}V${gatherY}H${receipt.cx}V${receipt.cy}`
        : `M${report.cx} ${report.bottom}V${gatherY}H${receipt.cx}V${receipt.cy}`,
    ),
    delivery: `M${collector.cx} ${collector.bottom}V${result.top}`,
  };
  if (JSON.stringify(nextDiagram) === JSON.stringify(diagram.value))
    return false;
  diagram.value = nextDiagram;
  return true;
}

function cancelSequence() {
  for (const animation of animations) animation.cancel();
  animations = [];
  animating.value = false;
  projectStatus.value = "idle";
  coordinatorStatus.value = "idle";
}
function refreshGeometry() {
  if (!measureDiagram()) return;
  sequenceVersion++;
  cancelSequence();
}
function signal(
  element: SVGPathElement | null,
  delay: number,
  duration: number,
) {
  if (!element) return;
  const animation = element.animate(
    [
      { strokeDashoffset: "0.08", opacity: 0 },
      { strokeDashoffset: "0", opacity: 1, offset: 0.08 },
      { strokeDashoffset: "-0.92", opacity: 1, offset: 0.92 },
      { strokeDashoffset: "-1", opacity: 0 },
    ],
    { delay, duration, easing: "cubic-bezier(0.45, 0, 0.55, 1)" },
  );
  animations.push(animation);
  return animation;
}
function emphasize(
  element: HTMLElement | null,
  delay: number,
  duration: number,
) {
  if (!element) return;
  const animation = element.animate(
    [
      { boxShadow: "inset 0 0 0 1px transparent" },
      { boxShadow: "inset 0 0 0 1px #ff4b3e", offset: 0.25 },
      { boxShadow: "inset 0 0 0 1px #ff4b3e", offset: 0.7 },
      { boxShadow: "inset 0 0 0 1px transparent" },
    ],
    { delay, duration, easing: "cubic-bezier(0.45, 0, 0.55, 1)" },
  );
  animations.push(animation);
  return animation;
}
function whenFinished(
  animation: Animation | undefined,
  version: number,
  action: () => void,
) {
  if (animation)
    animation.onfinish = () => {
      if (!disposed && version === sequenceVersion) action();
    };
}
function replay() {
  const version = ++sequenceVersion;
  cancelSequence();
  measureDiagram();
  void nextTick(() => {
    if (
      disposed ||
      version !== sequenceVersion ||
      !inView ||
      document.hidden ||
      !canReplay.value
    )
      return;
    animating.value = true;
    coordinatorStatus.value = "working";
    signal(intakeSignal.value, 0, 450);
    for (const element of dispatchSignals.value) {
      whenFinished(signal(element, 450, 1000), version, () => {
        projectStatus.value = "working";
      });
    }
    for (const element of cards.value) {
      whenFinished(emphasize(element, 1450, 1800), version, () => {
        projectStatus.value = "idle";
      });
    }
    for (const element of reports.value) emphasize(element, 3250, 1800);
    for (const element of reportSignals.value) signal(element, 5050, 1000);
    emphasize(coordinator.value, 6050, 1600);
    whenFinished(emphasize(summary.value, 6050, 1600), version, () => {
      coordinatorStatus.value = "idle";
    });
    signal(deliverySignal.value, 7650, 450);
    whenFinished(emphasize(delivery.value, 8100, 1800), version, () => {
      animating.value = false;
    });
  });
}
function updateMotion() {
  canReplay.value =
    !motion?.matches && typeof body.value?.animate === "function";
  if (canReplay.value) replay();
  else {
    sequenceVersion++;
    cancelSequence();
  }
}
function onVisibilityChange() {
  for (const animation of animations) {
    if (document.hidden && animation.playState === "running") animation.pause();
    else if (
      !document.hidden &&
      inView &&
      canReplay.value &&
      animation.playState === "paused"
    )
      animation.play();
  }
  animating.value =
    !document.hidden &&
    inView &&
    animations.some((animation) => animation.playState === "running");
}
watch(locale, () => nextTick(refreshGeometry), { flush: "post" });
onMounted(() => {
  motion = matchMedia("(prefers-reduced-motion: reduce)");
  motion.addEventListener("change", updateMotion);
  document.addEventListener("visibilitychange", onVisibilityChange);
  measureDiagram();
  updateMotion();
  if ("ResizeObserver" in window) {
    dimensions = new ResizeObserver(refreshGeometry);
    for (const element of [
      body.value,
      request.value,
      coordinator.value,
      summary.value,
      delivery.value,
      ...cards.value,
      ...reports.value,
      ...receipts.value,
    ]) {
      if (element) dimensions.observe(element);
    }
  }
  if ("MutationObserver" in window) {
    direction = new MutationObserver(refreshGeometry);
    direction.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["dir"],
    });
  }
  if ("IntersectionObserver" in window && body.value) {
    visibility = new IntersectionObserver(
      ([entry]) => {
        const visible = entry?.isIntersecting ?? false;
        if (visible === inView) return;
        inView = visible;
        if (inView) replay();
        else {
          animating.value = false;
          for (const animation of animations)
            if (animation.playState === "running") animation.pause();
        }
      },
      { threshold: 0.1 },
    );
    visibility.observe(body.value);
  }
});
onBeforeUnmount(() => {
  disposed = true;
  sequenceVersion++;
  cancelSequence();
  dimensions?.disconnect();
  visibility?.disconnect();
  direction?.disconnect();
  motion?.removeEventListener("change", updateMotion);
  document.removeEventListener("visibilitychange", onVisibilityChange);
});
</script>

<template>
  <div class="workflow-board" role="group" :aria-label="t('demoLabel')">
    <div class="workflow-board-header">
      <span><SiteIcon name="tree-structure" />{{ t("demoWorkspace") }}</span>
      <span>{{ t("demoIllustration") }}</span>
    </div>
    <div ref="body" class="workflow-body">
      <div ref="request" class="request-block">
        <span class="eyebrow">{{ t("demoYou") }}</span>
        <p>{{ t("demoRequest") }}</p>
      </div>
      <div class="intake-space" aria-hidden="true" />
      <div ref="coordinator" class="coordinator-block">
        <div class="coordinator-heading">
          <SiteIcon name="tree-structure" /><span>{{
            t("demoCoordinator")
          }}</span>
        </div>
        <p>{{ t("demoCoordinatorText") }}</p>
        <WorkflowStatus :status="coordinatorStatus" :animating="animating" />
      </div>
      <div class="branch-space" aria-hidden="true" />
      <div class="project-grid">
        <article
          v-for="project in projects"
          :key="project.key"
          ref="cards"
          :data-project="project.key"
          class="project-card"
          :aria-labelledby="`${diagramId}-${project.key}`"
        >
          <header class="project-heading">
            <h3 :id="`${diagramId}-${project.key}`">
              {{ t(`demo${project.key}`) }}
            </h3>
            <span class="code-path" dir="ltr">{{ project.path }}</span>
          </header>
          <div class="agent-identity">
            <img
              :src="`${config.app.baseURL}agents/${project.icon}.svg`"
              alt=""
              width="28"
              height="28"
            />
            <div class="agent-copy">
              <strong>{{ project.agent }}</strong
              ><span>{{ t("demoContext") }}</span>
            </div>
            <WorkflowStatus :status="projectStatus" :animating="animating" />
          </div>
          <p class="session-rules">{{ t("demoRules") }}</p>
          <p class="project-task">{{ t(`demo${project.key}Task`) }}</p>
          <div ref="reports" :data-project="project.key" class="project-report">
            <span class="report-label"
              ><SiteIcon name="chat-circle-dots" />{{
                t("demoReportLabel")
              }}</span
            >
            <p>{{ t(`demo${project.key}Report`) }}</p>
          </div>
        </article>
      </div>
      <div class="branch-space" aria-hidden="true" />
      <div ref="summary" class="summary-block">
        <div class="receipt-ports" aria-hidden="true">
          <span
            v-for="project in projects"
            :key="project.key"
            ref="receipts"
            :data-project="project.key"
          />
        </div>
        <div class="coordinator-heading">
          <SiteIcon name="tree-structure" /><span>{{
            t("demoReportsTitle")
          }}</span>
        </div>
        <p>{{ t("demoReportsDescription") }}</p>
      </div>
      <div class="intake-space" aria-hidden="true" />
      <div ref="delivery" class="delivery-block">
        <SiteIcon name="plugs-connected" />
        <div>
          <h3>{{ t("demoDeliveryTitle") }}</h3>
          <p>{{ t("demoDeliveryDescription") }}</p>
        </div>
      </div>
      <svg
        class="connector-layer"
        :viewBox="`0 0 ${diagram.width} ${diagram.height}`"
        aria-hidden="true"
      >
        <defs>
          <marker
            :id="markerId"
            viewBox="0 0 8 8"
            refX="8"
            refY="4"
            markerWidth="6"
            markerHeight="6"
            orient="auto"
          >
            <path d="M1 1L7 4L1 7" fill="none" stroke="currentColor" />
          </marker>
        </defs>
        <path
          :d="diagram.intake"
          class="connector-track"
          :marker-end="`url(#${markerId})`"
        />
        <path
          v-for="(path, index) in diagram.dispatch"
          :key="`dispatch-${index}`"
          :d="path"
          class="connector-track"
          :marker-end="`url(#${markerId})`"
        />
        <path
          v-for="(path, index) in diagram.reports"
          :key="`report-${index}`"
          :d="path"
          class="connector-track report-track"
          :marker-end="`url(#${markerId})`"
        />
        <path
          :d="diagram.delivery"
          class="connector-track"
          :marker-end="`url(#${markerId})`"
        />
        <path
          ref="intakeSignal"
          :d="diagram.intake"
          class="connector-signal"
          pathLength="1"
        />
        <path
          v-for="(path, index) in diagram.dispatch"
          :key="`dispatch-signal-${index}`"
          ref="dispatchSignals"
          :d="path"
          class="connector-signal"
          pathLength="1"
        />
        <path
          v-for="(path, index) in diagram.reports"
          :key="`report-signal-${index}`"
          ref="reportSignals"
          :d="path"
          class="connector-signal report-signal"
          pathLength="1"
        />
        <path
          ref="deliverySignal"
          :d="diagram.delivery"
          class="connector-signal"
          pathLength="1"
        />
      </svg>
    </div>
    <div class="workflow-footer">
      <p><SiteIcon name="chat-circle-dots" />{{ t("demoDirect") }}</p>
      <button v-if="canReplay" type="button" @click="replay">
        {{ t("demoReplay") }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.workflow-board {
  border: 1px solid #515151;
  background: #181818;
}
.workflow-board-header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 16px 24px;
  background: #272727;
  color: #ababab;
  font-size: 12px;
  line-height: 16px;
}
.workflow-board-header > span:first-child {
  display: flex;
  align-items: center;
  gap: 8px;
  color: #f5f5f5;
}
.workflow-board-header .site-icon {
  width: 16px;
  height: 16px;
}
.workflow-body {
  position: relative;
  padding: 32px;
}
.request-block,
.coordinator-block,
.summary-block,
.delivery-block {
  width: min(100%, 680px);
  margin-inline: auto;
}
.request-block {
  text-align: center;
  padding: 16px;
}
.request-block .eyebrow {
  justify-content: center;
}
.request-block p {
  margin-top: 16px;
  font-size: 20px;
  line-height: 28px;
}
.coordinator-block,
.summary-block {
  position: relative;
  padding: 24px;
  border: 1px solid #797979;
  background: #272727;
  text-align: center;
}
.coordinator-heading {
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 16px;
  color: #f5f5f5;
  font-size: 16px;
  line-height: 24px;
}
.coordinator-heading .site-icon {
  color: var(--red);
  flex-shrink: 0;
}
.coordinator-block > p,
.summary-block > p {
  margin-top: 16px;
  color: #d0d0d0;
  font-size: 16px;
  line-height: 24px;
}
.coordinator-block .workflow-status {
  justify-content: center;
  margin-top: 16px;
}
.branch-space {
  height: 48px;
}
.intake-space {
  height: 32px;
}
.project-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  align-items: stretch;
  gap: 24px;
}
.project-card {
  display: flex;
  flex-direction: column;
  min-width: 0;
  padding: 24px;
  border: 1px solid #515151;
  background: #1f1f1f;
}
.project-heading {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px 16px;
  margin-bottom: 24px;
}
.project-heading h3 {
  font-size: 20px;
  line-height: 28px;
}
.code-path {
  font-size: 12px;
  line-height: 16px;
  color: #b4b4b4;
  overflow-wrap: anywhere;
}
.agent-identity {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
  align-items: center;
  margin-bottom: 16px;
}
.agent-copy {
  flex: 1;
  min-width: 96px;
}
.agent-identity .workflow-status {
  margin-inline-start: auto;
  max-width: 100%;
}
.agent-identity img {
  flex-shrink: 0;
}
.agent-identity strong {
  display: block;
  font-size: 16px;
  line-height: 24px;
  font-weight: 500;
}
.agent-copy > span {
  display: block;
  color: #ababab;
  font-size: 12px;
  line-height: 16px;
}
.session-rules {
  color: #ababab;
  font-size: 12px;
  line-height: 16px;
  margin-top: 16px;
}
.project-task {
  font-size: 18px;
  line-height: 28px;
  color: #f5f5f5;
  margin-top: 24px;
  margin-bottom: 24px;
}
.project-report {
  margin-top: auto;
  padding: 16px;
  border: 1px solid #666;
  background: #131209;
}
.report-label {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  line-height: 16px;
  color: #c8c8c8;
}
.report-label .site-icon {
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  color: var(--red);
}
.project-report > p {
  margin-top: 16px;
  font-size: 14px;
  line-height: 20px;
  color: #deded4;
}
.workflow-body p,
.coordinator-heading,
.agent-identity {
  overflow-wrap: anywhere;
}
.receipt-ports {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  position: absolute;
  inset-inline: 20%;
  top: -3px;
  height: 6px;
}
.receipt-ports > span {
  width: 6px;
  height: 6px;
  background: #d0d0d0;
  margin-inline: auto;
}
.delivery-block {
  display: flex;
  align-items: flex-start;
  gap: 16px;
  padding: 24px;
  border: 1px solid #797979;
  background: #272727;
}
.delivery-block > .site-icon {
  color: #72d6a0;
  flex-shrink: 0;
}
.delivery-block h3 {
  font-size: 20px;
  line-height: 28px;
}
.delivery-block p {
  margin-top: 8px;
  color: #c5c5c5;
  font-size: 14px;
  line-height: 20px;
}
.connector-layer {
  position: absolute;
  inset: 0;
  z-index: 2;
  width: 100%;
  height: 100%;
  overflow: visible;
  pointer-events: none;
  color: #ababab;
}
.connector-track {
  fill: none;
  stroke: #797979;
  stroke-width: 1;
  stroke-linejoin: round;
}
.report-track {
  stroke: #9a645f;
}
.connector-signal {
  fill: none;
  stroke: #f5f5f5;
  stroke-width: 2;
  stroke-dasharray: 0.08 2;
  stroke-dashoffset: 0.08;
  opacity: 0;
}
.report-signal {
  stroke: var(--red);
}
.workflow-footer {
  display: flex;
  flex-wrap: wrap;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
  padding: 24px 24px;
  border-top: 1px solid #515151;
}
.workflow-footer p {
  display: flex;
  align-items: center;
  gap: 16px;
  color: #c5c5c5;
  font-size: 14px;
  line-height: 20px;
}
.workflow-footer p .site-icon {
  width: 20px;
  height: 20px;
  flex-shrink: 0;
}
.workflow-footer button {
  padding: 8px 16px;
  border: 1px solid #797979;
  color: #f5f5f5;
  background: #272727;
  font-size: 12px;
  line-height: 16px;
  transition: background 300ms cubic-bezier(0.32, 0.72, 0, 1);
}
.workflow-footer button:hover {
  background: #313131;
}
.workflow-footer button:active {
  background: #272727;
  transform: translateY(1px);
}
@media (max-width: 1100px) {
  .workflow-body {
    padding: 24px;
  }
  .project-grid {
    gap: 16px;
  }
  .project-card {
    padding: 16px;
  }
  .project-report {
    padding: 16px;
  }
}
@media (max-width: 900px) {
  .project-grid {
    grid-template-columns: minmax(0, 1fr);
    gap: 24px;
    margin-inline: 16px;
  }
  .project-card {
    padding: 24px;
  }
  .project-report {
    padding: 16px;
  }
  .project-heading {
    margin-bottom: 24px;
  }
}
@media (max-width: 480px) {
  .workflow-body {
    padding: 16px;
  }
  .workflow-board-header,
  .workflow-footer {
    padding: 16px;
  }
  .coordinator-block,
  .summary-block,
  .delivery-block {
    padding: 24px 16px;
  }
  .project-card {
    padding: 16px;
  }
  .project-report {
    padding: 16px;
  }
  .request-block p,
  .project-heading h3,
  .delivery-block h3 {
    font-size: 18px;
    line-height: 28px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .connector-signal {
    display: none;
  }
}
</style>
