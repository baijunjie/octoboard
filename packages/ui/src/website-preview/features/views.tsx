import React, { Suspense, useEffect, useRef } from "react";

import { AgentIcon } from "../../components/AgentIcon";
import { PanelHeader } from "../../components/PanelHeader";
import { SessionAgentIcon } from "../../components/SessionAgentIcon";
import { StatusIcon } from "../../components/StatusIcon";
import { FileTree } from "../../browser/FileTree";
import { buildTree, rowKey, type DirListing } from "../../browser/tree";
import { createFixtureDaemon } from "../../gallery/fixtureDaemon";
import { consoleOf, sessionOf, snapshotState } from "../../gallery/fixtures/builders";
import { quotaOf, quotaWindow } from "../../gallery/fixtures/quotas";
import { useCurrentLanguage, useT } from "../../i18n/react";
import { AccountNameLine, AccountQuotaSummary, LoginStateBadge } from "../../quota/AccountQuotaSummary";
import { AccountUsageBar } from "../../quota/AccountUsageBar";
import { statusLabel, sessionShownStatusKind } from "../../sessionLabel";
import { liveSessions } from "../../sidebar/order";
import { DaemonProvider } from "../../store";
import { RenderedDiff, useRendererScope } from "../../viewer/codeRenderer";
import { ChangeStatusBadge } from "../../viewer/ChangeStatusBadge";
import { previewCopy as english } from "../locales/en";
import { previewCopy as chinese } from "../locales/zh-Hans";
import { ready } from "./main";

const noop = () => {};
const workspace = consoleOf("feature-workspace", "Bistro");
const daemon = createFixtureDaemon({
  id: "feature-view", group: "Sidebar sessions", category: "normal", title: "Feature view", description: "Static product components.",
  state: snapshotState({ consoles: [workspace] }),
});
const readings = [
  quotaOf("claude", "work", "max", [quotaWindow("session", 38, 117), quotaWindow("weekly", 56, 3900)]),
  quotaOf("codex", "personal", "plus", [quotaWindow("session", 24, 221), quotaWindow("weekly", 43, 6600)]),
  quotaOf("grok", "build", "SuperGrok", [quotaWindow("credit_period", 16, 3400)]),
];

const listing = (entries: [string, "file" | "directory"][]): DirListing => ({
  state: "loaded", complete: true, refreshing: false,
  entries: entries.map(([name, kind]) => ({ name, kind, size: kind === "file" ? 128 : null, target: null, version: null })),
});
const expanded = new Set([rowKey(".claude"), rowKey(".claude/skills")]);
const files = buildTree(new Map([
  ["", listing([[".claude", "directory"], ["CLAUDE.md", "file"], [".mcp.json", "file"]])],
  [".claude", listing([["skills", "directory"]])],
  [".claude/skills", listing([["code-review", "directory"], ["testing", "directory"]])],
]), new Set([".claude", ".claude/skills"]), undefined);

const patch = `diff --git a/src/orders.ts b/src/orders.ts
--- a/src/orders.ts
+++ b/src/orders.ts
@@ -1,5 +1,7 @@
 export async function submit(order) {
-  return await create(order);
+  const previous = await find(order.key);
+  if (previous) return previous;
+  return await createOnce(order);
 }
\u0020
 export const retrySafe = true;
`;

export function FeatureView({ feature }: { feature: string }): React.ReactElement {
  const language = useCurrentLanguage();
  const t = useT();
  const copy = language === "zh-Hans" ? chinese : english;
  useEffect(() => {
    if (feature === "reviewChanges") return;
    const first = requestAnimationFrame(() => requestAnimationFrame(ready));
    return () => cancelAnimationFrame(first);
  }, [feature]);
  let content: React.ReactNode;
  switch (feature) {
    case "managedAccounts":
      content = <section className="feature-surface accounts-view">
        <PanelHeader title={t("rail.quota.accounts")} />
        <div className="account-list">{readings.map((quota, index) => <div key={quota.agent} className="account-item">
          <AgentIcon agent={quota.agent} className="size-5" />
          <div className="account-details"><AccountNameLine name={["Claude Code", "Codex", "Grok Build"][index]} quota={quota} showUpdated={false} /><div className="account-subline"><LoginStateBadge quota={quota} /><AccountUsageBar agent={quota.agent} quota={quota} now={Date.now()} className="w-20" /></div></div>
        </div>)}</div>
      </section>;
      break;
    case "usageMonitor":
      content = <section className="feature-surface usage-view"><PanelHeader title={t("rail.quota")} /><div className="usage-list">{readings.slice(0, 2).map((quota, index) => <div className="usage-item" key={quota.agent}><AccountNameLine name={["Claude Code", "Codex"][index]} quota={quota} showUpdated={false} /><AccountQuotaSummary quota={quota} single /></div>)}</div></section>;
      break;
    case "sessionStatus": {
      const sessions = liveSessions([
        sessionOf("build", workspace.id, "server", copy.serverTask, "working", { agent: "codex" }),
        sessionOf("review", workspace.id, "server", copy.review, "idle"),
        sessionOf("checks", workspace.id, "server", copy.orderChecks, "waiting_user", { agent: "grok" }),
      ]);
      content = <section className="feature-surface status-view">{sessions.map((session) => {
        const status = sessionShownStatusKind(session);
        return <div className="session-item" key={session.id}><SessionAgentIcon session={session} accounts={[]} defaultNames={{}} touchPopover={false} /><div className="session-copy"><div className="session-title">{session.title}</div><div className="session-state"><StatusIcon status={status} decorative /><span>{statusLabel(t, status)}</span></div></div></div>;
      })}</section>;
      break;
    }
    case "existingWorkflow": content = <WorkflowFiles />; break;
    case "reviewChanges": content = <Changes />; break;
    default: throw new Error(`Unknown feature view: ${feature}`);
  }
  return <DaemonProvider value={daemon}><main className="feature-canvas" data-feature={feature}>{content}</main></DaemonProvider>;
}

function WorkflowFiles(): React.ReactElement {
  const t = useT();
  const tree = useRef<HTMLDivElement>(null);
  return <section className="feature-surface files-view"><PanelHeader title={t("panel.files.name")} /><FileTree nodes={files.nodes} expanded={expanded} label={t("panel.files.name")} onExpandedChange={noop} onOpenFile={noop} onRetry={noop} treeRef={tree} /></section>;
}

function Changes(): React.ReactElement {
  const t = useT();
  useRendererScope();
  return <section className="feature-surface changes-view"><header className="changes-heading"><code dir="ltr">src/orders.ts</code><ChangeStatusBadge status="modified">{t("viewer.change.modified")}</ChangeStatusBadge></header><div className="diff-body" dir="ltr"><Suspense fallback={null}><RenderedDiff name="orders.ts" patch={patch} layout="unified" theme="dark" wrap={false} onDrawn={ready} labels={{ unmodified: (count) => t("viewer.expand.unmodified", { count }), unknown: t("viewer.expand.unknown"), above: t("viewer.expand.above"), below: t("viewer.expand.below"), between: t("viewer.expand.between"), hiddenAll: (count) => t("viewer.expand.hiddenAll", { count }), all: t("viewer.expand.all"), loading: t("viewer.expand.loading"), failedHere: t("viewer.expand.failedHere") }} /></Suspense></div></section>;
}
