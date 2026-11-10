import { useCallback, useEffect, useRef, useState } from "react";

import { servedProvider } from "../../agents/providers";
import { raiseError } from "../../components/alerts/raiseError";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../../components/announce/StatusAnnouncer";
import { ActionableError } from "../../errors/actionableError";
import { CodedError } from "../../errors/codedError";
import { errorPresentation } from "../../errors/errorPresentation";
import { InlineReference } from "../../i18n/InlineReference";
import { isolate } from "../../i18n/isolate";
import { Message, useT } from "../../i18n/react";
import type { AccountMcpInventory, Agent, McpServerAuthOutcome } from "../../protocol";
import { useDaemon, useProviders } from "../../store";

/** The sign-in to or sign-out of a server that the dialog has asked for and not yet seen end. */
export interface McpServerRun {
  action: "sign_in" | "sign_out";
  /** The answer is in and has succeeded; the inventory is being read again for the new state. */
  settling: boolean;
  /** A cancel of the sign-in is out. */
  cancelling: boolean;
}

/** What the dialog knows of an account's MCP inventory: the last answer (an inventory, or the request's own failure),
 * whether a reading is under way, how many answers have arrived, so a failure that repeats in the same words is
 * announced again, and the sign-ins and sign-outs under way by server name. */
export interface McpInventoryState {
  answer?: { inventory: AccountMcpInventory } | { error: CodedError };
  reading: boolean;
  /** The current reading has run past `SLOW_READING_MS` with no answer yet. */
  slow: boolean;
  attempt: number;
  /** The wait before the answer's failure is announced: set when the first answer came so soon after the dialog opened
   * that the announcement would be lost in the dialog's own (see `AFTER_FOCUS_MOVE_DELAY_MS`). */
  announceDelayMs: number | undefined;
  runs: Readonly<Record<string, McpServerRun>>;
  /** The status to announce of the last sign-in or sign-out that succeeded ("Signed in to …"). */
  announcement: string | undefined;
  /** Reads again with the refresh flag, unless a reading is under way. */
  refresh: () => void;
  /** Signs the account in to or out of the server, unless one of the two is already running for it. */
  signIn: (server: string) => void;
  signOut: (server: string) => void;
  /** Ends the sign-in to the server that is running. */
  cancelSignIn: (server: string) => void;
}

/** How long a reading takes to count as slow. Connecting to servers that do not answer
 * can keep the account's CLI waiting for tens of seconds. */
export const SLOW_READING_MS = 15_000;

/** How long the dialog waits for a reading before it shows the reading as timed out. The daemon answers every reading
 * within its own overall deadline (90 s); this ceiling is just above it, for an answer that never comes all the same
 * (a daemon that stopped answering while the connection stayed up). */
export const READING_CEILING_MS = 100_000;

const withoutSettling = (runs: Readonly<Record<string, McpServerRun>>) =>
  Object.values(runs).some((run) => run.settling) ? Object.fromEntries(Object.entries(runs).filter(([, run]) => !run.settling)) : runs;

/** Whether the provider of `agent` can list an account's MCP servers and connectors (`capabilities.mcp_inventory`). */
export function useReadsMcpInventory(agent: Agent): boolean {
  return servedProvider(useProviders(), agent)?.capabilities.mcp_inventory === true;
}

/**
 * The MCP inventory of one account, read when `enabled` turns on (the dialog opened) and again, with the refresh flag,
 * by `refresh`. The last answer stays while a later reading runs, which for Claude Code can take over a minute; one
 * reading at a time, a press while one runs doing nothing (the daemon would join it anyway). An answer that lands after
 * the dialog has gone is dropped.
 *
 * It also runs the sign-ins and sign-outs of the account's servers, one per server. A failure of one, or a refusal
 * (that of the cancel included), is raised as an error alert, even after the dialog has gone; a success reads the
 * inventory again, behind the reading under way when one is, and the server's run goes on, settling, until that
 * reading has answered, so its control does not flicker back to the old state.
 */
export function useAccountMcpInventory(agent: Agent, account: string | null, enabled: boolean): McpInventoryState {
  const { request } = useDaemon();
  const [answer, setAnswer] = useState<McpInventoryState["answer"]>();
  const [reading, setReading] = useState(false);
  const [slow, setSlow] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [announceDelayMs, setAnnounceDelayMs] = useState<number>();
  const [runs, setRuns] = useState<Record<string, McpServerRun>>({});
  const [announcement, setAnnouncement] = useState<string>();
  const busy = useRef(false);
  const rereadQueued = useRef(false);
  const running = useRef(new Set<string>());
  const mounted = useRef(true);
  const openedAt = useRef<number>(undefined);
  const latestAsk = useRef(0);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  // A reading's timers outlive an unmount rather than being cleared by it: StrictMode unmounts and mounts again while
  // the first reading runs, and that reading is not started again (`busy`), so clearing them would lose them. What
  // they run checks `mounted` itself, as an answer does, so a timer that fires after the dialog has gone does nothing.
  const after = (ms: number, run: () => void) => {
    const timer = setTimeout(run, ms);
    return () => clearTimeout(timer);
  };
  const read = useCallback((refresh: boolean) => {
    if (busy.current) return;
    busy.current = true;
    setReading(true);
    // An answer that comes after the ceiling still replaces the timed-out one, unless another reading has started
    // since, whether or not that one has answered.
    const ask = (refreshing: boolean) => {
      const asked = ++latestAsk.current;
      let overdue = false;
      const stopSlow = after(SLOW_READING_MS, () => {
        if (mounted.current) setSlow(true);
      });
      const stopCeiling = after(READING_CEILING_MS, () => {
        overdue = true;
        settle({ inventory: { agent, account, read_at: Date.now(), connectors: [], servers: [], unavailable: { reason: "timed_out", detail: null } } });
      });
      const answered = (next: NonNullable<McpInventoryState["answer"]>) => {
        stopSlow();
        stopCeiling();
        if (!overdue) settle(next);
        else if (mounted.current && asked === latestAsk.current) setAnswer(next);
      };
      request({ type: "read_account_mcp_inventory", agent, account, refresh: refreshing }).then(
        (reply) => answered({ inventory: reply.inventory }),
        (err: unknown) => answered({ error: errorPresentation(err) }),
      );
    };
    const settle = (next: NonNullable<McpInventoryState["answer"]>) => {
      if (!mounted.current) {
        busy.current = false;
        return;
      }
      const opened = openedAt.current;
      openedAt.current = undefined;
      setAnnounceDelayMs(opened !== undefined && Date.now() - opened < AFTER_FOCUS_MOVE_DELAY_MS ? AFTER_FOCUS_MOVE_DELAY_MS : undefined);
      setSlow(false);
      setAnswer(next);
      setAttempt((value) => value + 1);
      if (rereadQueued.current) {
        rereadQueued.current = false;
        ask(true);
        return;
      }
      busy.current = false;
      setReading(false);
      setRuns(withoutSettling);
    };
    ask(refresh);
  }, [request, agent, account]);
  const t = useT();
  const setRun = (server: string, patch: Partial<McpServerRun> | null) => setRuns((held) => {
    const next = { ...held };
    if (patch && held[server]) next[server] = { ...held[server], ...patch };
    else delete next[server];
    return next;
  });
  const authEnded = (server: string, action: McpServerRun["action"], result: McpServerAuthOutcome | { error: unknown }) => {
    running.current.delete(server);
    if ("status" in result && result.status === "succeeded") {
      if (!mounted.current) return;
      setRun(server, { settling: true });
      setAnnouncement(t(action === "sign_in" ? "settings.accounts.mcp.server.signInSucceeded" : "settings.accounts.mcp.server.signOutSucceeded", { server: isolate(server) }));
      if (busy.current) rereadQueued.current = true;
      else read(true);
      return;
    }
    if (mounted.current) setRun(server, null);
    if ("status" in result && result.status === "cancelled") return;
    // A failure is raised even once the dialog has gone: a sign-in can run for minutes after it closed, and the user
    // started it. A request the daemon refused, or the connection lost on the way, is raised with its own words; the
    // run is over for the dialog, and nothing is read again.
    const title = t(action === "sign_in" ? "settings.accounts.mcp.server.signInFailedTitle" : "settings.accounts.mcp.server.signOutFailedTitle");
    if ("error" in result) {
      raiseError(result.error, { title });
      return;
    }
    const timedOut = result.status === "timed_out" && action === "sign_in";
    const summary = timedOut ? "settings.accounts.mcp.server.signInTimedOut" : action === "sign_in" ? "settings.accounts.mcp.server.signInFailed" : "settings.accounts.mcp.server.signOutFailed";
    const recovery = timedOut ? "mcpSignInTimedOut" : action === "sign_in" ? "mcpSignIn" : "mcpSignOut";
    raiseError(new ActionableError(t(summary, { server: isolate(server) }), recovery, { detail: result.detail ?? undefined }), {
      title,
      content: <Message id={summary} params={{ server: <InlineReference dir="auto" wrap>{server}</InlineReference> }} />,
    });
  };
  const beginAuth = (server: string, action: McpServerRun["action"]) => {
    if (running.current.has(server)) return;
    running.current.add(server);
    setAnnouncement(undefined);
    setRuns((held) => ({ ...held, [server]: { action, settling: false, cancelling: false } }));
    const sent = action === "sign_in"
      ? request({ type: "sign_in_account_mcp_server", agent, account, server })
      : request({ type: "sign_out_account_mcp_server", agent, account, server });
    sent.then((reply) => authEnded(server, action, reply.outcome), (error: unknown) => authEnded(server, action, { error }));
  };
  const cancelSignIn = (server: string) => {
    setRun(server, { cancelling: true });
    request({ type: "cancel_account_mcp_sign_in", agent, account, server }).catch((error: unknown) => {
      if (mounted.current) setRun(server, { cancelling: false });
      // The sign-in ended as the cancel went out, and its own answer is on its way.
      if (error instanceof CodedError && error.code === "mcp_sign_in_not_running") return;
      // Raised even once the dialog has gone, as the sign-in's own failure is: the sign-in may still be running.
      raiseError(error, { title: t("settings.accounts.mcp.server.cancelFailedTitle") });
    });
  };
  useEffect(() => {
    if (!enabled) return;
    openedAt.current = Date.now();
    read(false);
  }, [enabled, read]);
  return {
    answer, reading, slow, attempt, announceDelayMs, runs, announcement,
    refresh: () => read(true),
    signIn: (server) => beginAuth(server, "sign_in"),
    signOut: (server) => beginAuth(server, "sign_out"),
    cancelSignIn,
  };
}
