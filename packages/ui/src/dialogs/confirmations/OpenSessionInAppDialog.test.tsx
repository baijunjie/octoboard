// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import { mountForTooltip } from "../../components/hint/tooltipTestSupport";
import { CodedError } from "../../errors/codedError";
import { consoleOf, projectOf, sessionOf } from "../../fixtures/builders";
import type { ProcessTree, Session } from "../../protocol";
import type { RequestHandler } from "../../requests";
import { DaemonProvider, type Daemon } from "../../store";
import { testDaemon, withEmptyReplies } from "../../test/testDaemon";
import { OpenSessionInAppDialog } from "./OpenSessionInAppDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const session = sessionOf("s-1", console_.id, project.id, "Fix the parser", "idle");

const TREE: ProcessTree = {
  id: "4242:1", session_id: session.id, command: "npm run dev", cwd: "/r-feature", cwd_missing: false, kept: false, keepable: true,
  started_at: 1_000, process_count: 1, memory_bytes: 1_000_000, ports: [],
};
const SECOND: ProcessTree = { ...TREE, id: "4300:2", command: "tail -f log" };

beforeEach(() => {
  // An earlier test's `click()` leaves react-aria's modality on virtual, where a dialog's opening focus waits a frame.
  act(() => setInteractionModality("pointer"));
});

function mount(
  processes: ProcessTree[],
  { request, onClose = vi.fn(), sessions = [session], inspecting }: { request?: RequestHandler; onClose?: () => void; sessions?: Session[]; inspecting?: Promise<void> } = {},
): { daemon: Daemon; onClose: () => void } {
  const daemon = testDaemon({
    state: { sessions: new Map(sessions.map((s) => [s.id, s])) },
    request: vi.fn(withEmptyReplies(async (body) => {
      if (body.type === "inspect_session_branches") {
        await inspecting;
        return { type: "session_branches_inspected", id: null, items: [], ...(processes.length > 0 ? { processes } : {}) };
      }
      return (request ?? (async () => ({ type: "ack", id: null })))(body);
    })) as never,
    showNotice: vi.fn(),
  });
  mountForTooltip(
    <DaemonProvider value={daemon}>
      <OpenSessionInAppDialog requested={session} label="Open in Claude app" onClose={onClose} />
    </DaemonProvider>,
  );
  return { daemon, onClose };
}

/** The sessions the dialog's last inspection was asked about. */
const inspected = (daemon: Daemon): string[] => {
  const asked = (daemon.request as ReturnType<typeof vi.fn>).mock.calls.map(([body]) => body).filter((body) => body.type === "inspect_session_branches");
  return [...asked[asked.length - 1].sessions].sort();
};

/** What the dialog asked the daemon besides inspecting. */
const sent = (daemon: Daemon) => (daemon.request as ReturnType<typeof vi.fn>).mock.calls.map(([body]) => body).filter((body) => body.type !== "inspect_session_branches");

const dialog = () => document.querySelector("[role=alertdialog]")!;
/** The dialog's text as it reads, without the bidi isolates around substituted values. */
const textOf = () => dialog().textContent!.replace(/[\u2066-\u2069]/g, "");
const boxes = () => [...document.querySelectorAll<HTMLInputElement>('[role=alertdialog] input[type="checkbox"]')];
const confirmButton = () => document.body.querySelector<HTMLButtonElement>("[data-slot=alert-dialog-footer] button:last-child")!;
const cancelButton = () => document.body.querySelector<HTMLButtonElement>("[data-slot=alert-dialog-footer] button:first-child")!;

async function submitDialog(): Promise<void> {
  await act(async () => {});
  await act(async () => {
    document.body.querySelector("[role=alertdialog] form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  });
}

it("is titled and confirmed with the app action's wording, names the session, and says it is archived with its branches kept", async () => {
  mount([TREE]);
  await act(async () => {});

  expect(dialog().querySelector("h2")?.textContent).toBe("Open in Claude app");
  expect(confirmButton().textContent).toBe("Open in Claude app");
  expect(cancelButton().textContent).toBe("Cancel");
  const text = textOf();
  expect(text).toContain("The session Fix the parser will be archived, then opened in the agent's app.");
  expect(text).toContain("Its branches and worktrees are kept. Once the app is done with the conversation, reopen the session from the archive to continue it here.");
});

it("lists the trees under a Background processes heading, each a checked box, as the archive does", async () => {
  mount([TREE, SECOND]);
  await act(async () => {});

  expect([...dialog().querySelectorAll("h3")].map((heading) => heading.textContent)).toEqual(["Background processes"]);
  expect(boxes().map((box) => box.checked)).toEqual([true, true]);
  expect(dialog().textContent).toContain("npm run dev");
  expect(dialog().textContent).toContain("tail -f log");
  expect(dialog().textContent).toContain("2 processes will be stopped.");
});

it("opens with no trees named while every box is left checked, since everything is stopped", async () => {
  const { daemon, onClose } = mount([TREE, SECOND]);
  await act(async () => {});
  expect(sent(daemon)).toEqual([]);

  await submitDialog();

  expect(sent(daemon)).toEqual([{ type: "open_session_in_app", session: session.id }]);
  expect(onClose).toHaveBeenCalledOnce();
});

it("names the unchecked trees as the ones to keep running, and only those", async () => {
  const { daemon } = mount([TREE, SECOND]);
  await act(async () => {});

  act(() => boxes()[1].click());
  expect(dialog().textContent).toContain("1 process will be stopped.");
  expect(dialog().textContent).toContain("The process you keep stays running.");
  await submitDialog();

  expect(sent(daemon)).toEqual([{ type: "open_session_in_app", session: session.id, keep_process_ids: [SECOND.id] }]);
});

it("never names a tree that cannot be kept, which stays checked", async () => {
  const unkeepable: ProcessTree = { ...TREE, id: "4500:4", command: "node shared.js", keepable: false, unkeepable_reason: "agent_console" };
  const { daemon } = mount([unkeepable, TREE]);
  await act(async () => {});

  const [kept, fixed] = boxes();
  act(() => fixed.click());
  expect(fixed.checked).toBe(true);
  act(() => kept.click());
  await submitDialog();

  expect(sent(daemon)).toEqual([{ type: "open_session_in_app", session: session.id, keep_process_ids: [TREE.id] }]);
});

it("sends nothing and closes when cancelled", async () => {
  const { daemon, onClose } = mount([TREE]);
  await act(async () => {});

  await act(async () => cancelButton().click());

  expect(sent(daemon)).toEqual([]);
  expect(onClose).toHaveBeenCalledOnce();
});

it("stays open and raises the failure as an alert titled for the app when the daemon refuses", async () => {
  const refusal = new CodedError("The application could not be opened: not installed", "native_app_unavailable", { detail: "not installed" });
  const { onClose } = mount([TREE], { request: async () => { throw refusal; } });
  await act(async () => {});

  await submitDialog();

  expect(onClose).not.toHaveBeenCalled();
  expect(dialog()).not.toBeNull();
  const [alert] = errorAlerts.getSnapshot();
  expect(alert.title).toBe("Could not open the agent's app");
  expect((alert.error as CodedError).code).toBe("native_app_unavailable");
});

it("names the session by its current title while the store holds it, and by the requested one once the store drops it", () => {
  const { daemon } = mount([TREE]);
  act(() => daemon.store.setState({ sessions: new Map([[session.id, { ...session, title: "Renamed" }]]) }));
  expect(textOf()).toContain("The session Renamed will be archived");

  act(() => daemon.store.setState({ sessions: new Map() }));
  expect(textOf()).toContain("The session Fix the parser will be archived");
});

it("lists the sessions bound to it that are archived with it, running ones included, and keeps the others out", async () => {
  const worker = sessionOf("s-2", console_.id, project.id, "Worker", "working", { bound_to: session.id });
  const nested = sessionOf("s-3", console_.id, project.id, "Nested", "waiting_user", { bound_to: worker.id });
  const archived = sessionOf("s-4", console_.id, project.id, "Already archived", "archived", { bound_to: session.id });
  const elsewhere = sessionOf("s-5", console_.id, project.id, "Elsewhere", "working", { bound_to: "other-owner" });
  const { daemon } = mount([], { sessions: [session, worker, nested, archived, elsewhere] });
  await act(async () => {});

  expect([...dialog().querySelectorAll("h3")].map((heading) => heading.textContent)).toEqual(["Archived with it (2)"]);
  const rows = [...dialog().querySelectorAll("li")].map((row) => row.textContent ?? "");
  expect(rows).toHaveLength(2);
  expect(rows.some((row) => row.includes("Worker"))).toBe(true);
  expect(rows.some((row) => row.includes("Nested"))).toBe(true);
  expect(textOf()).not.toContain("Already archived");
  expect(textOf()).not.toContain("Elsewhere");
  // What goes along is inspected too, for the processes it would stop.
  expect(inspected(daemon)).toEqual([session.id, worker.id, nested.id].sort());
});

it("follows the sessions bound to it live, and asks the daemon to open only the session", async () => {
  const worker = sessionOf("s-2", console_.id, project.id, "Worker", "working", { bound_to: session.id });
  const { daemon, onClose } = mount([], { sessions: [session, worker] });
  await act(async () => {});
  expect(dialog().textContent).toContain("Archived with it (1)");

  const late = sessionOf("s-3", console_.id, project.id, "Late", "idle", { bound_to: session.id });
  act(() => daemon.store.setState({ sessions: new Map([...daemon.store.getState().sessions, [late.id, late]]) }));
  await act(async () => {});
  expect(dialog().textContent).toContain("Archived with it (2)");
  expect(textOf()).toContain("Late");

  await submitDialog();

  expect(sent(daemon)).toEqual([{ type: "open_session_in_app", session: session.id }]);
  expect(onClose).toHaveBeenCalledOnce();
});

it("shows both the bound sessions and the processes, each under its own heading", async () => {
  const worker = sessionOf("s-2", console_.id, project.id, "Worker", "working", { bound_to: session.id });
  mount([TREE], { sessions: [session, worker] });
  await act(async () => {});

  expect([...dialog().querySelectorAll("h3")].map((heading) => heading.textContent)).toEqual(["Archived with it (1)", "Background processes"]);
});

it("shows no section where nothing is archived with it and no process runs", async () => {
  mount([]);
  await act(async () => {});

  expect(dialog().querySelectorAll("h3")).toHaveLength(0);
  expect(dialog().querySelector("li")).toBeNull();
});

it("holds the confirmation while the inspection is still pending, and sends it once it has answered", async () => {
  let answer!: () => void;
  const inspecting = new Promise<void>((done) => { answer = done; });
  const { daemon, onClose } = mount([TREE], { inspecting });

  await submitDialog();
  expect(sent(daemon)).toEqual([]);
  expect(onClose).not.toHaveBeenCalled();

  await act(async () => answer());
  await submitDialog();

  expect(sent(daemon)).toEqual([{ type: "open_session_in_app", session: session.id }]);
  expect(onClose).toHaveBeenCalledOnce();
});
