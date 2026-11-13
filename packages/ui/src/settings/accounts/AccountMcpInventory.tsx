import { Plug, RefreshCw, Server } from "lucide-react";
import React, { createContext, useContext, useId, useLayoutEffect, useRef } from "react";

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
import { Button, Spinner, Table, TableBody, TableCell, TableColumn, TableHeader, TableRow } from "../../primitives";
import type { Agent, ConnectorState, LocalMcpServer, McpInventoryUnavailable, PlatformConnector } from "../../protocol";
import { providerConnectorPlatform, providerMcpSessionSignIn } from "../../providers/registry";
import type { McpInventoryState } from "./useAccountMcpInventory";

/** The aside at the end of the account's name line, once the first answer is in (until then the body's reading line is
 * the only progress shown): when the inventory shown was read (not while the last answer is a failure), or "Refreshing"
 * while a later reading runs, then the Refresh button, whose icon rotates meanwhile. */
export function McpInventoryAside({ state, refreshButton }: { state: McpInventoryState; refreshButton: React.Ref<HTMLButtonElement> }): React.ReactElement | null {
  const t = useT();
  if (!state.answer) return null;
  const inventory = "inventory" in state.answer ? state.answer.inventory : undefined;
  return (
    <span className="ms-auto flex shrink-0 items-center gap-1 text-xs font-normal text-app-muted">
      {state.reading
        ? <span>{t("settings.accounts.mcp.refreshing")}</span>
        : inventory && !inventory.unavailable && <UpdatedAge at={inventory.read_at} copy={{ now: "settings.accounts.mcp.updatedNow", age: "settings.accounts.mcp.updated" }} />}
      <TitledControl title={t("settings.accounts.mcp.refresh")}>
        <Button ref={refreshButton} iconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} label={t("settings.accounts.mcp.refresh")} focusOnPress={false} pending={state.reading} dimWhilePending={false} pendingIndicator={<RefreshCw aria-hidden="true" className="size-4 motion-safe:animate-spin-slow" />} onPress={state.refresh}>
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
 * connector settings, since a connector belongs to the platform account the account is signed in with; an agent with no
 * connector platform and no connector to show (Grok Build) has no such group.
 *
 * When a failure goes (Try again answered with the inventory), focus that fell to the page with it, or was already
 * there, goes to the Refresh button (`refreshButton`, the aside's). When an answer replaces what held focus
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
        <span>{state.slow ? <Message id="settings.accounts.mcp.readingSlow" params={{ agent: <ProviderName agent={agent} /> }} /> : t("settings.accounts.mcp.reading")}</span>
      </div>
    );
  }
  if ("error" in answer) {
    return <FailureAlert title={t("settings.accounts.mcp.failedTitle")} message={answer.error} onRetry={state.refresh} pending={state.reading} announce attempt={state.attempt} announceDelayMs={state.announceDelayMs} />;
  }
  const { inventory } = answer;
  if (inventory.unavailable) return <UnavailableAlert agent={agent} unavailable={inventory.unavailable} state={state} />;
  return (
    <div className="flex flex-col gap-5">
      {(providerConnectorPlatform(agent) || inventory.connectors.length > 0) && <ConnectorGroup agent={agent} connectors={inventory.connectors} />}
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

/** A group's heading, standing under the dialog's title. */
function GroupHeading({ id, children }: { id: string; children: React.ReactNode }): React.ReactElement {
  return <h3 id={id} className="text-sm font-semibold text-foreground">{children}</h3>;
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
          <EntryTable labelledBy={headingId} stateColumns={[t("settings.accounts.mcp.column.status")]}>
            {connectors.map((connector) => {
              const state = CONNECTOR_STATE[connector.state];
              return (
                <EntryRow
                  key={connector.id}
                  id={connector.id}
                  name={connector.name}
                  stateCells={[<DotStatusBadge key="state" color={state.color}>{t(state.key)}</DotStatusBadge>]}
                  facts={connector.plugins.length > 0 ? [<FromPlugins key="plugins" plugins={connector.plugins} />] : []}
                />
              );
            })}
          </EntryTable>
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
        : (
          <EntryTable labelledBy={headingId} stateColumns={[t("settings.accounts.mcp.column.signIn"), t("settings.accounts.mcp.column.runtime")]} actions={servers.some((server) => server.oauth_sign_in)}>
            {servers.map((server) => <ServerEntry key={`${server.plugin ?? ""}:${server.name}`} agent={agent} server={server} state={state} />)}
          </EntryTable>
        )}
    </section>
  );
}

/** One local server's row: its name, its sign-in state and its runtime state in a column each ("Unknown" where the agent
 * does not know), then
 * what it is (its URL, the plugin bringing it, how many tools it offers). A failed server's Failed badge is followed by a
 * mark holding what the agent said, the raw text alone, since the badge already says it failed. A server the agent signs in to with
 * OAuth through its own login command (`oauth_sign_in`) has Sign in or Sign out in the row's actions cell; while one runs the
 * button is pending and a sign-in says it waits for the browser, with Cancel. A Claude Code server that needs sign-in
 * says how to authorize it, since Octoboard has no way of signing it in. Cancel goes when the sign-in ends; focus on
 * it goes to the row's Sign in or Sign out button. */
function ServerEntry({ agent, server, state }: { agent: Agent; server: LocalMcpServer; state: McpInventoryState }): React.ReactElement {
  const t = useT();
  const run = state.runs[server.name];
  const waiting = run?.action === "sign_in" && !run.settling;
  const unknown = <span className="text-xs text-app-muted">{t("settings.accounts.mcp.state.unknownShort")}</span>;
  const signIn = {
    signed_in: <DotStatusBadge color="success">{t("settings.accounts.mcp.server.signedIn")}</DotStatusBadge>,
    needs_sign_in: <DotStatusBadge color="warning">{t("settings.accounts.mcp.server.needsSignIn")}</DotStatusBadge>,
    not_needed: <span className="text-xs text-app-muted">{t("settings.accounts.mcp.server.signInNotNeeded")}</span>,
    unknown,
  }[server.sign_in];
  const runtime = {
    running: <DotStatusBadge color="success">{t("settings.accounts.mcp.server.running")}</DotStatusBadge>,
    failed: (
      <span className="flex items-center gap-1">
        <DotStatusBadge color="danger">{t("settings.accounts.mcp.server.failed")}</DotStatusBadge>
        {server.runtime_message && (
          <StatusMark tone="danger" label={t("settings.accounts.mcp.server.failedLabel")}>
            <DiagnosticBlock>{server.runtime_message}</DiagnosticBlock>
          </StatusMark>
        )}
      </span>
    ),
    unknown,
  }[server.runtime];
  const facts: React.ReactNode[] = [];
  if (server.origin) facts.push(<span key="origin" dir="ltr" className="wrap-anywhere">{server.origin}</span>);
  if (server.plugin) facts.push(<FromPlugins key="plugin" plugins={[server.plugin]} />);
  if (server.tool_count !== null) facts.push(t("settings.accounts.mcp.toolCount", { count: server.tool_count }));
  const sessionCommand = server.sign_in === "needs_sign_in" ? providerMcpSessionSignIn(agent) : undefined;
  return (
    <EntryRow id={`${server.plugin ?? ""}:${server.name}`} name={server.name} stateCells={[signIn, runtime]} actions={server.oauth_sign_in ? <ServerAuthActions server={server} state={state} /> : undefined} facts={facts}>
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
    </EntryRow>
  );
}

/** A server's Sign in or Sign out, with Cancel while a sign-in waits for the browser. It holds its own focus hand-off:
 * the table renders a row's cells apart from the component that builds the row, so the hand-off lives with the
 * buttons it moves focus between. */
function ServerAuthActions({ server, state }: { server: LocalMcpServer; state: McpInventoryState }): React.ReactElement {
  const t = useT();
  const cancelId = useId();
  const authButton = useRef<HTMLButtonElement>(null);
  const handoffFrom = useHandoffAfterRemoval(() => authButton.current);
  const run = state.runs[server.name];
  const waiting = run?.action === "sign_in" && !run.settling;
  const signOut = run ? run.action === "sign_out" : server.sign_in === "signed_in";
  const name = isolate(server.name);
  return (
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
}

/** Whether `EntryTable` has the actions column, for its rows to add their cell. */
const ActionsColumnContext = createContext(false);

/**
 * A group's entries in HeroUI's `Table`: Name (the rows' header), one column per state (`stateColumns`, each a
 * row's state in its own words, so the states line up whatever the names' lengths), Details and, when a row has a
 * button (`actions`), Actions. A dialog too narrow for the columns scrolls the table sideways in its scroll container
 * rather than squeezing the cells.
 */
function EntryTable({ labelledBy, stateColumns, actions = false, children }: { labelledBy: string; stateColumns: string[]; actions?: boolean; children: React.ReactNode }): React.ReactElement {
  const t = useT();
  return (
    <Table labelledBy={labelledBy} minWidth={actions ? "40rem" : "34rem"}>
      <TableHeader>
        <TableColumn rowHeader>{t("settings.accounts.mcp.column.name")}</TableColumn>
        {stateColumns.map((column) => <TableColumn key={column}>{column}</TableColumn>)}
        <TableColumn>{t("settings.accounts.mcp.column.details")}</TableColumn>
        {actions ? <TableColumn>{t("settings.accounts.mcp.column.actions")}</TableColumn> : null}
      </TableHeader>
      <TableBody>
        <ActionsColumnContext.Provider value={actions}>{children}</ActionsColumnContext.Provider>
      </TableBody>
    </Table>
  );
}

/** A table row: the name, one cell per state column (`stateCells`), what it is (its facts, one to a line, muted, then
 * any further lines in `children`) and, in a table with the actions column, its buttons. */
function EntryRow({ id, name, stateCells, actions, facts, children }: { id: string; name: string; stateCells: React.ReactNode[]; actions?: React.ReactNode; facts: React.ReactNode[]; children?: React.ReactNode }): React.ReactElement {
  const actionsColumn = useContext(ActionsColumnContext);
  return (
    <TableRow id={id} textValue={name}>
      <TableCell className="align-top">
        <span dir="auto" className="block min-w-24 leading-6 wrap-anywhere">{name}</span>
      </TableCell>
      {stateCells.map((cell, index) => (
        <TableCell key={index} className="align-top whitespace-nowrap">
          <div className="flex min-h-6 items-center">{cell}</div>
        </TableCell>
      ))}
      <TableCell className="align-top">
        {(facts.length > 0 || children) && (
          <div className="flex min-h-6 flex-col justify-center gap-1 text-xs text-app-muted">
            {facts.map((fact, index) => <span key={index} className="min-w-0 wrap-anywhere">{fact}</span>)}
            {children}
          </div>
        )}
      </TableCell>
      {actionsColumn ? (
        <TableCell className="align-top">
          {actions && <div className="flex min-h-6 items-center gap-1 whitespace-nowrap">{actions}</div>}
        </TableCell>
      ) : null}
    </TableRow>
  );
}
