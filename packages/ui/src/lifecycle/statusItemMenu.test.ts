import { describe, expect, it } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Account, AccountQuota, Agent, QuotaWindow, Session, SessionStatus } from "../protocol";
import { iconKeysOf } from "./statusItemIcons";
import { buildStatusItemMenu, MAX_PER_SECTION } from "./statusItemMenu";
import type { StatusItemQuotaSource } from "./statusItemQuota";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;

const consoles = [consoleOf("c", "Main")];
const projects = [projectOf("p", "c", "Alpha")];

const NOW = Date.UTC(2026, 9, 19, 12);
const MINUTE = 60_000;

const NO_QUOTA: StatusItemQuotaSource = { language: "en", now: NOW, accounts: [], agentAvailability: new Map(), accountQuotas: [] };

const menuOf = (sessions: Session[], quota: StatusItemQuotaSource = NO_QUOTA) =>
  buildStatusItemMenu(t, {
    quota,
    consoles,
    projects,
    sessions,
    consoleMap: new Map(consoles.map((c) => [c.id, c])),
    projectMap: new Map(projects.map((p) => [p.id, p])),
  });

const inProject = (id: string, status: SessionStatus, title = id, terminal_title?: string) =>
  sessionOf(id, "c", "p", title, status, { terminal_title });

describe("buildStatusItemMenu", () => {
  it("says so when no session is live", () => {
    expect(menuOf([inProject("a", "archived"), inProject("b", "interrupted")]).sections).toEqual([
      { heading: "No sessions running", items: [] },
    ]);
  });

  it("lists waiting, then working, then idle sessions, each under a heading with its count", () => {
    const { sections } = menuOf([inProject("i", "idle"), inProject("w", "working"), inProject("h", "waiting_user")]);
    expect(sections.map((s) => [s.heading, s.items.map((item) => item.session)])).toEqual([
      ["Waiting for you (1)", ["h"]],
      ["Working (1)", ["w"]],
      ["Awaiting instructions (1)", ["i"]],
    ]);
  });

  it("raises the icon's hand only while a live session waits for the user", () => {
    expect(menuOf([inProject("w", "working"), inProject("i", "idle")]).attention).toBe(false);
    expect(menuOf([inProject("a", "archived"), inProject("b", "interrupted")]).attention).toBe(false);
    expect(menuOf([inProject("w", "working"), inProject("h", "waiting_user")]).attention).toBe(true);
    expect(menuOf([]).attention).toBe(false);
  });

  it("leaves the hand down for a waiting session that sits in no console the source holds", () => {
    const orphan = sessionOf("o", "gone", "p", "o", "waiting_user");
    expect(menuOf([orphan]).attention).toBe(false);
  });

  it("lists a session running in the background with the working ones", () => {
    const background = sessionOf("b", "c", "p", "b", "idle", { background_tasks: [{ id: "t1" }] });
    const { sections } = menuOf([inProject("i", "idle"), inProject("w", "working"), background]);
    expect(sections.map((s) => [s.heading, s.items.map((item) => item.session)])).toEqual([
      ["Working (2)", ["w", "b"]],
      ["Awaiting instructions (1)", ["i"]],
    ]);
  });

  it("gives each heading its status glyph and each session its agent's mark", () => {
    const { sections } = menuOf([inProject("h", "waiting_user"), sessionOf("w", "c", "p", "w", "working", { agent: "codex" })]);
    expect(sections.map((s) => [s.icon, s.items.map((item) => item.icon)])).toEqual([
      ["status:waiting_user", ["agent:claude"]],
      ["status:working", ["agent:codex"]],
    ]);
  });

  it("names a session with its location unless it is titled after it", () => {
    const { sections } = menuOf([inProject("a", "idle", "Fix bug"), inProject("b", "idle", "Alpha")]);
    expect(sections[0].items.map((item) => item.label).sort()).toEqual(["Alpha", `${isolate("Fix bug")} — ${isolate("Alpha")}`]);
  });

  it("names the first ten of a longer section and sums up the rest in a line that cannot be chosen", () => {
    const many = Array.from({ length: MAX_PER_SECTION + 3 }, (_, i) => inProject(`s${i}`, "working"));
    const [section] = menuOf(many).sections;
    expect(section.heading).toBe(`Working (${many.length})`);
    expect(section.items).toHaveLength(MAX_PER_SECTION + 1);
    expect(section.items.at(-1)).toEqual({ label: "3 more…" });
  });
});

describe("buildStatusItemMenu quota submenus", () => {
  const available = (...agents: Agent[]): StatusItemQuotaSource["agentAvailability"] =>
    new Map(agents.map((agent) => [agent, { agent, availability: "available", default_config_dir: null } as never]));
  const window = (patch: Partial<QuotaWindow>): QuotaWindow => ({
    kind: "session",
    label: null,
    used_percent: 18,
    resets_at: NOW + 117 * MINUTE,
    window_minutes: 300,
    ...patch,
  });
  const quotaOf = (agent: Agent, account: string | null, patch: Partial<AccountQuota> = {}): AccountQuota => ({
    agent,
    account,
    fetched_at: NOW - MINUTE,
    plan: "max",
    windows: [window({}), window({ kind: "weekly", used_percent: 5, resets_at: null })],
    unavailable: null,
    ...patch,
  });
  const source = (patch: Partial<StatusItemQuotaSource>): StatusItemQuotaSource => ({ ...NO_QUOTA, ...patch });
  /** Each line as the text it shows: a header's title, a window's name and detail, a note's text. */
  const labels = (menu: ReturnType<typeof menuOf>, index = 0) =>
    menu.submenus[index].items.map((item) => {
      switch (item.kind) {
        case "header":
          return item.title;
        case "window":
          return `${item.name} | ${item.detail}`;
        case "note":
          return item.text;
        case "separator":
          return "---";
      }
    });

  it("has no submenu for an agent that is not available", () => {
    expect(menuOf([], source({ agentAvailability: new Map([["codex", { agent: "codex", availability: "unavailable" } as never]]) })).submenus).toEqual([]);
    expect(menuOf([]).submenus).toEqual([]);
  });

  it("has one submenu per available agent, in the agents' usual order", () => {
    const menu = menuOf([], source({ agentAvailability: available("grok", "claude") }));
    expect(menu.submenus.map((submenu) => submenu.title)).toEqual(["Claude quota", "Grok quota"]);
  });

  it("names each submenu's mark by the key a session line of that agent uses", () => {
    const menu = menuOf([], source({ agentAvailability: available("grok", "claude") }));
    expect(menu.submenus.map((submenu) => submenu.icon)).toEqual(["agent:claude", "agent:grok"]);
  });

  it("draws the submenus' marks", () => {
    const menu = menuOf([], source({ agentAvailability: available("grok", "claude") }));
    expect([...iconKeysOf(menu)]).toEqual(expect.arrayContaining(["agent:claude", "agent:grok"]));
  });

  it("lists an account's plan, then a row per window with its fraction, agent and whole line", () => {
    const menu = menuOf([], source({ agentAvailability: available("claude"), accountQuotas: [quotaOf("claude", null)] }));
    expect(menu.submenus[0].items).toEqual([
      { kind: "header", title: "Default · Max" },
      {
        kind: "window",
        name: "5-hour",
        detail: "82% remaining · resets in 1h 57m",
        label: "5-hour 82% remaining · resets in 1h 57m",
        fraction: 0.82,
        agent: "claude",
      },
      { kind: "window", name: "Weekly", detail: "95% remaining", label: "Weekly 95% remaining", fraction: 0.95, agent: "claude" },
    ]);
  });

  it("orders native quota details by account-wide priority while preserving model order and the reading", () => {
    const reading = quotaOf("claude", null, { windows: [
      window({ kind: "weekly_model", label: "Fable" }),
      window({ kind: "credit_period", window_minutes: null }),
      window({ kind: "weekly_model", label: "Sonnet" }),
      window({ kind: "weekly" }),
      window({ kind: "session" }),
    ] });
    const original = structuredClone(reading.windows);
    const menu = menuOf([], source({ agentAvailability: available("claude"), accountQuotas: [reading] }));
    expect(menu.submenus[0].items.flatMap((item) => item.kind === "window" ? [item.name] : [])).toEqual([
      "5-hour", "Weekly", "Credits", `${isolate("Fable")} weekly`, `${isolate("Sonnet")} weekly`,
    ]);
    expect(reading.windows).toEqual(original);
  });

  it("groups the owned accounts after the default one, each set off by a separator", () => {
    const owned: Account[] = [{ id: "a1", agent: "codex", name: "Work", config_dir: "/w" }];
    const menu = menuOf([], source({
      accounts: owned,
      agentAvailability: available("codex"),
      accountQuotas: [quotaOf("codex", null, { plan: null, windows: [window({})] }), quotaOf("codex", "a1", { plan: "pro_lite", windows: [window({ used_percent: 100 })] })],
    }));
    expect(labels(menu)).toEqual([
      "Default",
      "5-hour | 82% remaining · resets in 1h 57m",
      "---",
      `${isolate("Work")} · Pro Lite`,
      "5-hour | 0% remaining · resets in 1h 57m",
    ]);
  });

  it("says so in one line when an account's quota is unavailable or not read yet", () => {
    const unavailable = quotaOf("claude", null, { plan: null, windows: [], unavailable: { code: "quota_timed_out", params: {}, message: "" } });
    expect(labels(menuOf([], source({ agentAvailability: available("claude"), accountQuotas: [unavailable] })))).toEqual(["Default", "Quota unavailable"]);
    expect(labels(menuOf([], source({ agentAvailability: available("claude") })))).toEqual(["Default", "Reading quota…"]);
    // An entry with no windows and no failure to explain still gets the line, rather than a bare header.
    const empty = quotaOf("claude", null, { plan: null, windows: [] });
    expect(labels(menuOf([], source({ agentAvailability: available("claude"), accountQuotas: [empty] })))).toEqual(["Default", "Quota unavailable"]);
  });

  it("says only that an account is not logged in, which is a state and not a failed reading", () => {
    for (const code of ["quota_not_signed_in", "quota_config_dir_missing"]) {
      const unauthenticated = quotaOf("claude", null, { plan: null, windows: [], unavailable: { code, params: {}, message: "" } });
      expect(labels(menuOf([], source({ agentAvailability: available("claude"), accountQuotas: [unauthenticated] })))).toEqual(["Default", "Not logged in"]);
    }
  });

  it("keeps the last windows of an out-of-date reading under a line saying so", () => {
    const stale = quotaOf("claude", null, { fetched_at: NOW - 12 * MINUTE, unavailable: { code: "quota_timed_out", params: {}, message: "" } });
    const lines = labels(menuOf([], source({ agentAvailability: available("claude"), accountQuotas: [stale] })));
    expect(lines.slice(0, 3)).toEqual(["Default · Max", "Out of date: last read 12 minutes ago", "5-hour | 82% remaining · resets in 1h 57m"]);
  });

  it("counts the countdown from the moment given and says so once the reset has passed", () => {
    const quota = quotaOf("claude", null, { windows: [window({ resets_at: NOW - MINUTE })] });
    const lines = labels(menuOf([], source({ agentAvailability: available("claude"), accountQuotas: [quota] })));
    expect(lines[1]).toBe("5-hour | 82% remaining · resets now");
  });
});
