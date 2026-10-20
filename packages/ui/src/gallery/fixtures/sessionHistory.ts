import { daemonMessage } from "../../daemonMessage";
import type { HistoryMessage } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Terminal";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Website");
const archived = sessionOf("s-history", console_.id, project.id, "Speed up the pricing page", "archived", { ended_at: minutesAgo(60 * 20) });
const state = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [sessionOf("s-console", console_.id, undefined, "Hub", "idle"), archived],
});

const historyFailure = {
  code: "unknown_session",
  params: { session: archived.id },
  message: daemonMessage("en", "unknown_session", { session: archived.id }, "", {
    consoles: new Map([[console_.id, console_]]), projects: new Map([[project.id, project]]), sessions: new Map([[archived.id, archived]]),
  }),
};

/** What the session's record holds: the user's messages, those Octoboard wrote into the session (a
 * brief, a notice, a report) and the agent's final reply to each turn, as the daemon reads them.
 * The user's third message is long enough to wrap and carries a path that cannot break. */
const MESSAGES: HistoryMessage[] = [
  {
    speaker: "octoboard",
    turn: 1,
    at: minutesAgo(60 * 22),
    text: [
      "## Goal",
      "",
      "The pricing page takes more than three seconds to become usable on a cold cache. Bring it under one second on the reference laptop without changing its layout.",
      "",
      "## Context",
      "",
      "- The page is rendered by src/pricing/Page.tsx and its hero image is public/hero.jpg.",
      "- Lighthouse reports 4.1 MB transferred on the first visit.",
      "",
      "## Acceptance",
      "",
      "- Largest Contentful Paint under 1.0 s",
      "- No visual change at 1440 px or 390 px",
    ].join("\n"),
  },
  {
    speaker: "agent",
    turn: 1,
    at: minutesAgo(60 * 22 - 4),
    text: [
      "I read the page and the build output. Two things account for most of the weight:",
      "",
      "## Findings",
      "",
      "1. `public/hero.jpg` is 3.4 MB and is shown at most 1440 px wide.",
      "2. `/assets/` is served with `Cache-Control: no-store`, so nothing is ever reused.",
      "",
      "I will start with the image, then look at the headers.",
    ].join("\n"),
  },
  {
    speaker: "user",
    turn: 2,
    at: minutesAgo(60 * 21),
    text: "Why is the hero image so large? Can we serve the WebP version only when the browser accepts it, and keep the JPEG as a fallback for older clients? Also check the cache headers on /assets/ again, because I saw a 4 MB download on every visit last night even though nothing had changed since the previous deploy.",
  },
  {
    speaker: "agent",
    turn: 2,
    at: minutesAgo(60 * 21 - 6),
    text: [
      "The original was exported at 5120 px wide. I resized it and added a WebP variant:",
      "",
      "- `hero.webp`: 212 KB",
      "- `hero.jpg`: 486 KB, the fallback",
      "",
      "The page now picks the source with a `<picture>` element:",
      "",
      "```tsx",
      "<picture>",
      '  <source srcSet="/assets/hero.webp" type="image/webp" />',
      '  <img src="/assets/hero.jpg" alt="" width={1440} height={720} />',
      "</picture>",
      "```",
      "",
      "## Cache headers",
      "",
      "`/assets/` files are fingerprinted, so they can be cached for a year:",
      "",
      "```nginx",
      "location /assets/ {",
      '  add_header Cache-Control "public, max-age=31536000, immutable";',
      "}",
      "```",
    ].join("\n"),
  },
  {
    speaker: "user",
    turn: 3,
    at: minutesAgo(60 * 20 - 30),
    text: "Looks good. Also make sure /Users/dev/code/website/src/pricing/components/PlanComparisonTable/PlanComparisonTableRowGroup.tsx still renders the same table at 390 px.",
  },
  {
    speaker: "agent",
    turn: 3,
    at: minutesAgo(60 * 20 - 40),
    text: "The table renders the same at 390 px and 1440 px. Largest Contentful Paint is now **0.8 s** on a cold cache.",
  },
  {
    speaker: "octoboard",
    turn: 4,
    at: minutesAgo(60 * 20 - 45),
    text: "Message from Octoboard: your owner asked for a status update before this session is archived.",
  },
  {
    speaker: "agent",
    turn: 4,
    at: minutesAgo(60 * 20 - 46),
    text: "Status: the image and cache changes are applied and verified. Nothing is left to do.",
  },
  {
    speaker: "octoboard",
    turn: 5,
    at: minutesAgo(60 * 20 - 50),
    text: "Report from session s-2c9e — Review the pricing copy\nStatus: Done\n\nThe plan names and prices match the billing service. Two plan descriptions were shortened to fit one line at 390 px.",
  },
  {
    speaker: "agent",
    turn: 5,
    at: minutesAgo(60 * 20 - 52),
    text: [
      "Thanks. Noted the shortened descriptions:",
      "",
      "| Plan | Change |",
      "| --- | --- |",
      "| Team | Description shortened |",
      "| Business | Description shortened |",
    ].join("\n"),
  },
  {
    speaker: "user",
    turn: 6,
    at: minutesAgo(60 * 20 - 55),
    text: "好的，就按这个方案。可以归档了。",
  },
];

/** The sidebar in the project's focus mode, which lists the project's archived sessions, and the one
 * press that selects the archived session from there, leaving its history in the pane. */
const preferences = { sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` } satisfies Scenario["preferences"];
const openHistory: NonNullable<Scenario["steps"]> = [(ui) => ui.press(ui.session(archived.title))];

const base = { group: GROUP, category: "normal", state, preferences } satisfies Partial<Scenario>;

export const sessionHistoryScenarios: Scenario[] = [
  {
    ...base,
    id: "history-loaded",
    title: "Archived session history",
    description:
      "The pane shows the session's history in place of a terminal. The brief, the notice and the report that Octoboard wrote into the session are labelled with the app's name and set apart by a rule and muted text, not hidden; the user's messages are labelled You. The long message wraps, the long path breaks inside the column and a Chinese message ends the list; scroll to reach it. \"Show agent replies\" is off. The floating bar at the bottom says the session is archived and offers Reopen; the last message scrolls clear of it.",
    sessionHistory: { [archived.id]: MESSAGES },
    steps: openHistory,
  },
  {
    ...base,
    id: "history-replies",
    title: "Archived session history with agent replies",
    description:
      "With \"Show agent replies\" on, each turn's final reply follows its opening message as a Markdown document on a code-coloured surface: headings, ordered and bullet lists, inline code, fenced code blocks, bold text and a table. A turn opened by Octoboard gets a reply too. Scroll for the code blocks and the table.",
    sessionHistory: { [archived.id]: MESSAGES },
    steps: [...openHistory, (ui) => ui.press(ui.t("history.showReplies")), (ui) => ui.wait(500)],
  },
  {
    ...base,
    id: "history-reopening",
    title: "Reopening an archived session",
    description: "Reopen was pressed and has not been answered: the \"Resuming session…\" cover with its spinner paints over the history, and the floating bar is gone.",
    sessionHistory: { [archived.id]: MESSAGES },
    sessionResumes: { [archived.id]: "pending" },
    steps: [...openHistory, (ui) => ui.press(ui.t("sidebar.session.reopen"))],
  },
  {
    ...base,
    id: "history-empty",
    title: "Archived session with no messages",
    description: "The record holds no message: an empty panel says so, under the \"Show agent replies\" switch, which stays available.",
    sessionHistory: { [archived.id]: [] },
    steps: openHistory,
  },
  {
    ...base,
    id: "history-loading",
    title: "Archived session history loading",
    description: "The daemon has not answered yet: a spinner and \"Loading history…\" fill the pane. The floating bar already offers Reopen.",
    sessionHistory: { [archived.id]: "pending" },
    steps: openHistory,
  },
  {
    ...base,
    category: "exceptional",
    id: "history-unavailable",
    title: "Archived session history unavailable",
    description: "The agent's record of the session is missing or unreadable: a warning panel says so and points at reopening the session to see its terminal.",
    sessionHistory: { [archived.id]: "unavailable" },
    steps: openHistory,
  },
  {
    ...base,
    category: "exceptional",
    id: "history-failed",
    title: "Archived session history request failed",
    description: "After acknowledgement, the pane retains the daemon's actual failure reason in a stock danger alert. Try again requests history again; the floating bar still offers Reopen.",
    sessionHistory: { [archived.id]: historyFailure },
    steps: [...openHistory, (ui) => ui.press(ui.t("error.acknowledge"))],
  },
  {
    ...base,
    category: "exceptional",
    id: "history-failed-acknowledgement",
    title: "Archived history failure awaiting acknowledgement",
    description: "The failed request first requires acknowledgement with its structured daemon reason. The durable pane remains behind the dialog, with retry and Reopen available after OK.",
    sessionHistory: { [archived.id]: historyFailure },
    steps: openHistory,
  },
];
