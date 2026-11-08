import { Plug, RefreshCw, Server } from "lucide-react";
import React, { useId, useLayoutEffect, useRef } from "react";

import { AlertButton } from "../../components/alerts/AlertButton";
import { DiagnosticBlock } from "../../components/alerts/DiagnosticBlock";
import { ErrorAlert } from "../../components/alerts/ErrorAlert";
import { FailureAlert } from "../../components/alerts/FailureAlert";
import { AttendedStatusAnnouncer } from "../../components/announce/AttendedStatusAnnouncer";
import { StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { ROW_ICON_BUTTON_CLASS } from "../../components/controls/RowControls";
import { focusHandoffTarget, useHandoffAfterRemoval } from "../../components/focus/HandoffOnUnmount";
import { TitledControl } from "../../components/hint/TitledControl";
import { EmptyPanel } from "../../components/panel/EmptyPanel";
import { DotStatusBadge } from "../../components/status/DotStatusBadge";
import { StatusMark } from "../../components/status/StatusMark";
import { ExternalLink } from "../../components/text/ExternalLink";
import { ProviderName } from "../../components/text/ProviderName";
import { UpdatedAge } from "../../components/text/UpdatedAge";
import { InlineReference } from "../../i18n/InlineReference";
import { isolate } from "../../i18n/isolate";
import { joinNodes } from "../../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../../i18n/react";
import { Button, Spinner } from "../../primitives";
import type { Agent, ConnectorState, LocalMcpServer, McpInventoryUnavailable, PlatformConnector } from "../../protocol";
import { providerConnectorPlatform, providerMcpSessionSignIn } from "../../providers/registry";
import type { McpInventoryState } from "./useAccountMcpInventory";

/** The section's heading aside: when the inventory shown was read (not while the last answer is a failure), then the
 * Refresh button, pending while a reading runs. */
export function McpInventoryAside({ state, refreshButton }: { state: McpInventoryState; refreshButton: React.Ref<HTMLButtonElement> }): React.ReactElement {
  const t = useT();
  const inventory = state.answer && "inventory" in state.answer ? state.answer.inventory : undefined;
  return (
    <span className="ms-auto flex shrink-0 items-center gap-1 text-xs font-normal text-app-muted">
      {inventory && !inventory.unavailable && <UpdatedAge at={inventory.read_at} copy={{ now: "settings.accounts.mcp.updatedNow", age: "settings.accounts.mcp.updated" }} />}
      <TitledControl title={t("settings.accounts.mcp.refresh")}>
        <Button ref={refreshButton} iconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} label={t("settings.accounts.mcp.refresh")} focusOnPress={false} pending={state.reading} onPress={state.refresh}>
          <RefreshCw aria-hidden="true" className="size-4" />
        </Button>
      </TitledControl>
    </span>
  );
}

/**
 * The account's platform connectors and local MCP servers, as the agent reports them: a reading line until the first
 * answer, the failure in an Alert when the inventory could not be read, otherwise the two groups. Each entry shows its
 * name and state; a failed server's mark holds what the agent said. The platform group leads to the platform's own
 * connector settings, since a connector belongs to the platform account the account is signed in with.
 *
 * When a failure goes (Try again answered with the inventory), focus that fell to the page with it, or was already
 * there, goes to the section's Refresh button (`refreshButton`, the aside's). When an answer replaces what held focus
 * by something else — one failure's Alert by another's, the lists by a failure's Alert after a sign-in's rereading —
 * focus goes to the new Alert's Try again, else to Refresh. A sign-in or sign-out that succeeds is announced.
 */
export function McpInventory({ agent, state, refreshButton }: { agent: Agent; state: McpInventoryState; refreshButton: React.RefObject<HTMLButtonElement | null> }): React.ReactElement {
  const t = useT();
  const { answer } = state;
  const failed = answer !== undefined && ("error" in answer || Boolean(answer.inventory.unavailable));
  const failedBefore = useRef(failed);
  const bodyId = useId();
  const body = useRef<HTMLDivElement>(null);
  const handoffFrom = useHandoffAfterRemoval(() => {
    const tryAgain = [...(body.current?.querySelectorAll("button") ?? [])].find((button) => button.textContent === t("error.tryAgain"));
    return tryAgain ?? refreshButton.current;
  });
  useLayoutEffect(() => {
    if (failedBefore.current && !failed) focusHandoffTarget(refreshButton.current);
    failedBefore.current = failed;
  });
  // What the body shows, as a key: a change of it is a new subtree, whose removal hands on the focus it held.
  const shown = !answer ? "reading" : "error" in answer ? "failure" : answer.inventory.unavailable ? `unavailable:${answer.inventory.unavailable.reason}` : "inventory";
  const scope = `${bodyId}:${shown}`;
  return (
    <>
      <AttendedStatusAnnouncer text={state.announcement} />
      <div key={shown} ref={body} data-mcp-body={scope} className="contents">
        {handoffFrom(`[data-mcp-body="${CSS.escape(scope)}"]`)}
        <McpInventoryBody agent={agent} state={state} />
      </div>
    </>
  );
}

function McpInventoryBody({ agent, state }: { agent: Agent; state: McpInventoryState }): React.ReactElement {
  const t = useT();
  const { answer } = state;
  if (!answer) {
    return (
      <div className="flex items-center gap-2 text-sm text-app-muted">
        <Spinner size="sm" color="current" />
        <span>{t("settings.accounts.mcp.reading")}</span>
      </div>
    );
  }
  if ("error" in answer) {
    return <FailureAlert title={t("settings.accounts.mcp.failedTitle")} message={answer.error} onRetry={state.refresh} pending={state.reading} announce attempt={state.attempt} announceDelayMs={state.announceDelayMs} />;
  }
  const { inventory } = answer;
  if (inventory.unavailable) return <UnavailableAlert agent={agent} unavailable={inventory.unavailable} state={state} />;
  return (
    <div className="flex flex-col gap-3">
      <ConnectorGroup agent={agent} connectors={inventory.connectors} />
      <ServerGroup agent={agent} servers={inventory.servers} state={state} />
    </div>
  );
}

/** Why the inventory could not be read, in the application's words per reason, with what the agent or the daemon
 * said behind Details. A reason no retry can change (the provider lists nothing) has no Try again. */
function UnavailableAlert({ agent, unavailable, state }: { agent: Agent; unavailable: McpInventoryUnavailable; state: McpInventoryState }): React.ReactElement {
  const t = useT();
  const agentName = <ProviderName agent={agent} />;
  const { reason, detail } = unavailable;
  const retry = reason !== "unsupported_agent";
  return (
    <ErrorAlert
      announce
      announceKey={state.attempt}
      announceDelayMs={state.announceDelayMs}
      title={<Message id={`settings.accounts.mcp.unavailable.${reason}.title`} params={{ agent: agentName }} />}
      remedy={retry ? <Message id={`settings.accounts.mcp.unavailable.${reason}.remedy`} params={{ agent: agentName }} /> : undefined}
      details={detail ? <DiagnosticBlock>{detail}</DiagnosticBlock> : undefined}
      actions={retry && (
        <AlertButton kind="action" size="sm" focusOnPress={false} pending={state.reading} onPress={state.refresh}>
          {t("error.tryAgain")}
        </AlertButton>
      )}
    >
      {reason === "failed" ? undefined : <Message id={`settings.accounts.mcp.unavailable.${reason}.summary`} params={{ agent: agentName }} />}
    </ErrorAlert>
  );
}

/** A group's heading, standing under the section's own. */
function GroupHeading({ id, children }: { id: string; children: React.ReactNode }): React.ReactElement {
  return <h4 id={id} className="text-xs font-medium text-app-muted">{children}</h4>;
}

const CONNECTOR_STATE: Record<ConnectorState, { key: "settings.accounts.mcp.connector.connected" | "settings.accounts.mcp.connector.notConnected" | "settings.accounts.mcp.state.unknown"; color: "success" | "default" }> = {
  connected: { key: "settings.accounts.mcp.connector.connected", color: "success" },
  not_connected: { key: "settings.accounts.mcp.connector.notConnected", color: "default" },
  unknown: { key: "settings.accounts.mcp.state.unknown", color: "default" },
};

/** The plugins an entry comes from, each name a reference in the sentence. */
function FromPlugins({ plugins }: { plugins: string[] }): React.ReactElement {
  const language = useCurrentLanguage();
  const names = joinNodes(language, plugins, (plugin, key) => <InlineReference key={key} dir="auto" wrap size="xs">{plugin}</InlineReference>, "long");
  return <Message id="settings.accounts.mcp.fromPlugins" params={{ count: plugins.length, plugins: names }} />;
}

function ConnectorGroup({ agent, connectors }: { agent: Agent; connectors: PlatformConnector[] }): React.ReactElement {
  const t = useT();
  const platform = providerConnectorPlatform(agent);
  const headingId = `mcp-connectors-${agent}`;
  return (
    <section aria-labelledby={headingId} className="flex min-w-0 flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
        <GroupHeading id={headingId}>{t("settings.accounts.mcp.connectors")}</GroupHeading>
        {platform && (
          <ExternalLink href={platform.settingsUrl} className="ms-auto">
            {t("settings.accounts.mcp.manageOn", { platform: platform.name })}
          </ExternalLink>
        )}
      </div>
      {connectors.length === 0
        ? <EmptyPanel compact icon={Plug} message={t("settings.accounts.mcp.noConnectors")} />
        : (
          <ul className="flex flex-col gap-1.5">
            {connectors.map((connector) => {
              const state = CONNECTOR_STATE[connector.state];
              return (
                <Entry
                  key={connector.id}
                  name={connector.name}
                  states={<DotStatusBadge color={state.color}>{t(state.key)}</DotStatusBadge>}
                  facts={connector.plugins.length > 0 ? [<FromPlugins key="plugins" plugins={connector.plugins} />] : []}
                />
              );
            })}
          </ul>
        )}
      {platform && <p className="text-xs text-app-muted">{t("settings.accounts.mcp.platformAccount", { platform: platform.name })}</p>}
    </section>
  );
}

function ServerGroup({ agent, servers, state }: { agent: Agent; servers: LocalMcpServer[]; state: McpInventoryState }): React.ReactElement {
  const t = useT();
  const headingId = `mcp-servers-${agent}`;
  return (
    <section aria-labelledby={headingId} className="flex min-w-0 flex-col gap-1.5">
      <GroupHeading id={headingId}>{t("settings.accounts.mcp.servers")}</GroupHeading>
      {servers.length === 0
        ? <EmptyPanel compact icon={Server} message={t("settings.accounts.mcp.noServers")} />
        : <ul className="flex flex-col gap-1.5">{servers.map((server) => <ServerEntry key={`${server.plugin ?? ""}:${server.name}`} agent={agent} server={server} state={state} />)}</ul>}
    </section>
  );
}

/** One local server: its name with its sign-in and runtime states (a state the agent does not know shows nothing),
 * then what it is (its URL, the plugin bringing it, how many tools it offers). A server the agent signs in to with
 * OAuth through its own login command (`oauth_sign_in`) has Sign in or Sign out beside its states; while one runs the
 * button is pending and a sign-in says it waits for the browser, with Cancel. A Claude Code server that needs sign-in
 * says how to authorize it, since Octoboard has no way of signing it in. Cancel goes when the sign-in ends; focus on
 * it goes to the row's Sign in or Sign out button. */
function ServerEntry({ agent, server, state }: { agent: Agent; server: LocalMcpServer; state: McpInventoryState }): React.ReactElement {
  const t = useT();
  const cancelId = useId();
  const authButton = useRef<HTMLButtonElement>(null);
  const handoffFrom = useHandoffAfterRemoval(() => authButton.current);
  const run = state.runs[server.name];
  const waiting = run?.action === "sign_in" && !run.settling;
  const signOut = run ? run.action === "sign_out" : server.sign_in === "signed_in";
  const name = isolate(server.name);
  const states = (
    <>
      {server.sign_in === "signed_in" && <DotStatusBadge color="success">{t("settings.accounts.mcp.server.signedIn")}</DotStatusBadge>}
      {server.sign_in === "needs_sign_in" && <DotStatusBadge color="warning">{t("settings.accounts.mcp.server.needsSignIn")}</DotStatusBadge>}
      {server.runtime === "running" && <DotStatusBadge color="success">{t("settings.accounts.mcp.server.running")}</DotStatusBadge>}
      {server.runtime === "failed" && (
        <StatusMark tone="danger" text={t("settings.accounts.mcp.server.failed")} label={t("settings.accounts.mcp.server.failedLabel")} detail={server.runtime_message ?? undefined} />
      )}
    </>
  );
  const facts: React.ReactNode[] = [];
  if (server.origin) facts.push(<span key="origin" dir="ltr" className="wrap-anywhere">{server.origin}</span>);
  if (server.plugin) facts.push(<FromPlugins key="plugin" plugins={[server.plugin]} />);
  if (server.tool_count !== null) facts.push(t("settings.accounts.mcp.toolCount", { count: server.tool_count }));
  const sessionCommand = server.sign_in === "needs_sign_in" ? providerMcpSessionSignIn(agent) : undefined;
  const actions = server.oauth_sign_in && (
    <>
      {waiting && handoffFrom(`#${CSS.escape(cancelId)}`)}
      {waiting && (
        <Button id={cancelId} size="xs" variant="tertiary" focusOnPress={false} pending={run.cancelling} label={t("settings.accounts.mcp.server.cancelSignInLabel", { server: name })} onPress={() => state.cancelSignIn(server.name)}>
          {t("common.cancel")}
        </Button>
      )}
      <Button
        ref={authButton}
        size="xs"
        variant="secondary"
        focusOnPress={false}
        pending={run !== undefined}
        label={t(signOut ? "settings.accounts.mcp.server.signOutLabel" : "settings.accounts.mcp.server.signInLabel", { server: name })}
        onPress={() => (signOut ? state.signOut(server.name) : state.signIn(server.name))}
      >
        {t(signOut ? "settings.accounts.mcp.server.signOut" : "settings.accounts.mcp.server.signIn")}
      </Button>
    </>
  );
  return (
    <Entry name={server.name} states={states} actions={actions || undefined} facts={facts}>
      {waiting && (
        <p className="flex items-center gap-2 text-xs text-app-muted">
          <Spinner size="sm" color="current" />
          <span aria-hidden="true">{t("settings.accounts.mcp.server.waiting")}</span>
          <StatusAnnouncer text={t("settings.accounts.mcp.server.waiting")} element="span" />
        </p>
      )}
      {sessionCommand && (
        <p className="text-xs text-app-muted">
          <Message id="settings.accounts.mcp.server.signInFromSession" params={{ command: <InlineReference monospace size="xs">{sessionCommand}</InlineReference> }} />
        </p>
      )}
    </Entry>
  );
}

/** A list entry: the name and its state badges on one line, wrapping, with its actions at the trailing end; what it is
 * under it, muted, its facts apart. */
function Entry({ name, states, actions, facts, children }: { name: string; states: React.ReactNode; actions?: React.ReactNode; facts: React.ReactNode[]; children?: React.ReactNode }): React.ReactElement {
  return (
    <li className="flex min-w-0 flex-col gap-0.5">
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm">
        <span dir="auto" className="min-w-0 text-foreground wrap-anywhere">{name}</span>
        {states}
        {actions && <span className="ms-auto flex items-center gap-1">{actions}</span>}
      </div>
      {facts.length > 0 && (
        <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-xs text-app-muted">
          {facts.map((fact, index) => <span key={index} className="min-w-0">{fact}</span>)}
        </div>
      )}
      {children}
    </li>
  );
}
