import { ArrowUpRight, Globe, Play } from "lucide-react";
import React, { Suspense, useEffect } from "react";

import { trackSuspenseFallback } from "../../components/hooks/lazyReadiness";
import { AgentIcon } from "../../components/icons/AgentIcon";
import { PasteButton } from "../../components/controls/PasteButton";
import { TextInput } from "../../components/form/TextInput";
import { ExternalLink } from "../../components/text/ExternalLink";
import { StatusIcon } from "../../components/status/StatusIcon";
import { createFixtureDaemon } from "../../fixtures/fixtureDaemon";
import { snapshotState } from "../../fixtures/builders";
import { quotaOf, quotaWindow } from "../../fixtures/quotas";
import { useCurrentLanguage, useT } from "../../i18n/react";
import { AccountQuotaSummary, LoginStateBadge } from "../../quota/AccountQuotaSummary";
import { Button } from "../../primitives";
import { DaemonProvider } from "../../store";
import { RenderedDiff, useRendererScope } from "../../viewer/code/codeRenderer";
import { ready } from "./main";

const noop = () => {};
const daemon = createFixtureDaemon({ state: snapshotState({}) });
const accounts = [
  { name: "Claude Code", quota: quotaOf("claude", "work", "max", [quotaWindow("session", 38, 117)]) },
  { name: "Codex", quota: quotaOf("codex", "personal", "plus", []) },
];
const processPort = 3000;

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
  const language = useCurrentLanguage();
  const t = useT();
  useEffect(() => {
    if (feature === "reviewChanges") return;
    const first = requestAnimationFrame(() => requestAnimationFrame(ready));
    return () => cancelAnimationFrame(first);
  }, [feature]);
  let content: React.ReactNode;
  switch (feature) {
    case "managedAccounts":
      content = <ul className="feature-surface preview-list">{accounts.map(({ name, quota }) => (
        <li key={name} className="preview-row">
          <AgentIcon agent={quota.agent} className="size-5 shrink-0" />
          <strong>{name}</strong>
          <LoginStateBadge quota={quota} />
        </li>
      ))}</ul>;
      break;
    case "usageMonitor":
      content = <section className="feature-surface compact-view">
        <strong>Claude Code</strong>
        <AccountQuotaSummary quota={accounts[0].quota} single />
      </section>;
      break;
    case "sessionIntervention":
      content = <ul className="feature-surface preview-list">
        <li className="preview-row waiting-row">
          <StatusIcon status="waiting_user" decorative />
          <span>{t("session.status.waitingUser")}</span>
          <ArrowUpRight size={19} className="end-glyph" />
        </li>
        <li className="preview-row">
          <StatusIcon status="working" decorative />
          <span>{t("session.status.working")}</span>
        </li>
      </ul>;
      break;
    case "processMonitoring": {
      const memory = new Intl.NumberFormat(language, {
        style: "unit", unit: "megabyte", unitDisplay: "short",
      }).format(146);
      content = <section className="feature-surface compact-view process-view">
        <div className="preview-line">
          <Play size={17} className="process-running" /><code dir="ltr">pnpm dev</code>
        </div>
        <div className="preview-line process-facts"><bdi>{memory}</bdi><span>·</span><bdi>:{processPort}</bdi></div>
        <div className="preview-line">
          <Globe size={17} />
          <ExternalLink href={`http://localhost:${processPort}/`} dir="ltr"><code>localhost:{processPort}</code></ExternalLink>
        </div>
        <div className="preview-line process-actions">
          <Button size="sm" variant="secondary">{t("processes.keep")}</Button>
          <Button size="sm" variant="danger-soft">{t("processes.stop")}</Button>
        </div>
      </section>;
      break;
    }
    case "openAgentApp":
      content = <section className="feature-surface compact-view">
        <div className="preview-line preview-muted">
          <StatusIcon status="interrupted" decorative />{t("session.status.interrupted")}
        </div>
        <div className="preview-line">
          <AgentIcon agent="codex" className="size-5 shrink-0" />
          <strong>{t("provider.codex.openInApp")}</strong>
          <ArrowUpRight size={19} className="end-glyph" />
        </div>
      </section>;
      break;
    case "remoteDevices":
      content = <div className="remote-details-view card-fills">
        <TextInput
          label={t("remoteDaemons.form.details")}
          value="octoboard+ssh://alex@server:22"
          dir="ltr"
          onChange={noop}
          trailing={<PasteButton variant="secondary" besideField focusOnPress={false} failedText={t("remoteDaemons.form.details.pasteFailedTitle")} onPaste={noop} />}
        />
      </div>;
      break;
    case "nativeProjectSessions":
      content = <ul className="feature-surface preview-list">
        <li className="preview-row"><AgentIcon agent="claude" className="size-5 shrink-0" /><code dir="ltr">web/CLAUDE.md</code></li>
        <li className="preview-row"><AgentIcon agent="codex" className="size-5 shrink-0" /><code dir="ltr">api/AGENTS.md</code></li>
      </ul>;
      break;
    case "reviewChanges": content = <Changes />; break;
    default: throw new Error(`Unknown feature view: ${feature}`);
  }
  return <DaemonProvider value={daemon}><main className="feature-canvas" data-feature={feature}>{content}</main></DaemonProvider>;
}

function Changes(): React.ReactElement {
  const t = useT();
  useRendererScope();
  return <section className="feature-surface changes-view"><header className="changes-heading"><code dir="ltr">src/orders.ts</code></header><div className="diff-body" dir="ltr"><Suspense fallback={trackSuspenseFallback(null)}><RenderedDiff name="orders.ts" patch={patch} layout="unified" theme="dark" wrap={false} onDrawn={ready} labels={{ unmodified: (count) => t("viewer.expand.unmodified", { count }), unknown: t("viewer.expand.unknown"), above: t("viewer.expand.above"), below: t("viewer.expand.below"), between: t("viewer.expand.between"), hiddenAll: (count) => t("viewer.expand.hiddenAll", { count }), all: t("viewer.expand.all"), loading: t("viewer.expand.loading"), failedHere: t("viewer.expand.failedHere") }} /></Suspense></div></section>;
}
