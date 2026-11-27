import { ArrowUpRight, FileCode2 } from "lucide-react";
import React, { Suspense, useEffect } from "react";

import { trackSuspenseFallback } from "../../components/hooks/lazyReadiness";
import { AgentIcon } from "../../components/icons/AgentIcon";
import { StatusIcon } from "../../components/status/StatusIcon";
import { createFixtureDaemon } from "../../fixtures/fixtureDaemon";
import { consoleOf, minutesAgo, processTreeOf, projectOf, sessionOf, snapshotState } from "../../fixtures/builders";
import { scheduledTaskOf } from "../../fixtures/scheduledTaskBuilders";
import { quotaFailure, quotaOf, quotaWindow } from "../../fixtures/quotas";
import { useT } from "../../i18n/react";
import { AccountNameLine, AccountQuotaSummary, LoginStateBadge } from "../../quota/AccountQuotaSummary";
import { Button } from "../../primitives";
import { ProcessCommand, ProcessTreeDetails } from "../../process/ProcessTreeDetails";
import { MemoryRow } from "../../dialogs/memory/MemoryRow";
import type { MemoryIndexEntry, ScheduledTask } from "../../protocol";
import { TaskRow } from "../../scheduledTasks/ScheduledTasksSidebar";
import { DaemonProvider } from "../../store";
import { RenderedDiff, useRendererScope } from "../../viewer/code/codeRenderer";
import { ready } from "./main";

const workspace = consoleOf("feature-console", "Workspace");
const project = projectOf("feature-api", workspace.id, "api");
const daemon = createFixtureDaemon({ state: snapshotState({ consoles: [workspace], projects: [project] }) });
const accounts = [
  { provider: "Claude Code", name: "Work", quota: quotaOf("claude", "work", "max", [
    quotaWindow("session", 38, 117), quotaWindow("weekly", 72, 3 * 24 * 60),
  ]) },
  { provider: "Codex", name: "Personal", quota: quotaOf("codex", "personal", null, [], {
    unavailable: quotaFailure("quota_not_signed_in", { agent: "Codex" }, "Not signed in"),
  }) },
];
const session = sessionOf("feature-orders", workspace.id, project.id, "Orders API", "working");
const devServer = processTreeOf("feature-dev-server", session, "pnpm dev", 18, {
  cwd: project.path,
  memory_bytes: 146_000_000,
  ports: [3000],
  http_ports: [3000],
});
const tasks: ScheduledTask[] = [
  scheduledTaskOf("feature-task-checks", workspace.id, {
    project_id: project.id,
    launch: { title: "Nightly API checks", compatibility: {} },
    schedule: { kind: "daily", time: { hour: 2, minute: 0 } },
  }),
  scheduledTaskOf("feature-task-report", workspace.id, {
    launch: { title: "Weekly progress report", compatibility: {} },
    schedule: { kind: "weekly", days: ["friday"], time: { hour: 17, minute: 0 } },
  }),
];
const memories: MemoryIndexEntry[] = [
  { id: "a1b2c3d4", summary: "API tests run with pnpm test:api", local: false, created_at: minutesAgo(600), updated_at: minutesAgo(5), updated_by: { kind: "agent", agent: "codex" }, version: 2 },
  { id: "e5f6g7h8", summary: "Staging deploys need review", local: false, created_at: minutesAgo(3000), updated_at: minutesAgo(60 * 3), updated_by: { kind: "agent", agent: "claude" }, version: 1 },
];
const noop = (): void => undefined;

const patch = `diff --git a/src/orders.ts b/src/orders.ts
--- a/src/orders.ts
+++ b/src/orders.ts
@@ -1,3 +1,3 @@
 function submit(order) {
-  return create(order);
+  return createOnce(order);
 }
`;

export function FeatureView({ feature }: { feature: string }): React.ReactElement {
  const t = useT();
  useEffect(() => {
    if (feature === "reviewChanges") return;
    const first = requestAnimationFrame(() => requestAnimationFrame(ready));
    return () => cancelAnimationFrame(first);
  }, [feature]);
  let content: React.ReactNode;
  switch (feature) {
    case "managedAccounts":
      content = <ul className="feature-surface preview-list accounts-view">{accounts.map(({ provider, name, quota }) => (
        <li key={name} className="account-row">
          <AgentIcon agent={quota.agent} className="account-mark" />
          <div className="preview-identity"><strong dir="auto">{name}</strong><span dir="auto">{provider}</span></div>
          <LoginStateBadge quota={quota} separated={false} />
        </li>
      ))}</ul>;
      break;
    case "usageMonitor":
      content = <section className="feature-surface usage-view">
        <AccountNameLine name={<span className="provider-name"><AgentIcon agent={accounts[0].quota.agent} />{accounts[0].provider}</span>} quota={accounts[0].quota} />
        <AccountQuotaSummary quota={accounts[0].quota} single />
      </section>;
      break;
    case "scheduledTasks":
      content = <ul className="feature-surface preview-list task-view">{tasks.map((task) => (
        <li key={task.id}><TaskRow task={task} selected={task.id === tasks[0].id} running={false} onSelect={noop} onRunNow={noop} onSetEnabled={noop} onDelete={noop} /></li>
      ))}</ul>;
      break;
    case "sharedMemory":
      content = <ul className="feature-surface preview-list memory-view">{memories.map((line) => (
        <MemoryRow key={line.id} line={line} flagged={false} onView={noop} onEdit={noop} onDelete={noop} />
      ))}</ul>;
      break;
    case "sessionIntervention":
      content = <ul className="feature-surface preview-list session-view">
        <li className="session-row waiting-row">
          <StatusIcon status="waiting_user" decorative />
          <div className="preview-identity"><strong dir="auto">Checkout review</strong><span>{t("session.status.waitingUser")}</span></div>
          <ArrowUpRight size={18} className="session-open" />
        </li>
        <li className="session-row">
          <StatusIcon status="working" decorative />
          <div className="preview-identity"><strong dir="auto">{session.title}</strong><span>{t("session.status.working")}</span></div>
        </li>
      </ul>;
      break;
    case "processMonitoring":
      content = <section className="feature-surface process-view">
        <ProcessCommand tree={devServer} />
        <ProcessTreeDetails tree={devServer} links />
        <div className="process-actions">
          <Button size="xs" variant="secondary">{t("processes.keep")}</Button>
          <Button size="xs" variant="danger-soft">{t("processes.stop")}</Button>
        </div>
      </section>;
      break;
    case "reviewChanges": content = <Changes />; break;
    default: throw new Error(`Unknown feature view: ${feature}`);
  }
  return <DaemonProvider value={daemon}><main className="feature-canvas" data-feature={feature}>{content}</main></DaemonProvider>;
}

function Changes(): React.ReactElement {
  const t = useT();
  useRendererScope();
  return <section className="feature-surface changes-view"><header className="changes-heading hairline-b"><FileCode2 size={17} /><code dir="ltr">src/orders.ts</code></header><div className="diff-body" dir="ltr"><Suspense fallback={trackSuspenseFallback(null)}><RenderedDiff name="orders.ts" patch={patch} layout="unified" theme="dark" wrap={false} onDrawn={ready} labels={{ unmodified: (count) => t("viewer.expand.unmodified", { count }), unknown: t("viewer.expand.unknown"), above: t("viewer.expand.above"), below: t("viewer.expand.below"), between: t("viewer.expand.between"), hiddenAll: (count) => t("viewer.expand.hiddenAll", { count }), all: t("viewer.expand.all"), loading: t("viewer.expand.loading"), failedHere: t("viewer.expand.failedHere") }} /></Suspense></div></section>;
}
