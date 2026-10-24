import type { HistoryMessage, Session, SessionContext } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, contextOf, minutesAgo, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Terminal";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Website");

/** The archived session with the context its agent last reported, which the history header shows as the context badge beside its heading. */
function archivedWith(context?: SessionContext): Session {
  return sessionOf("s-history", console_.id, project.id, "Speed up the pricing page", "archived", { started_at: minutesAgo(60 * 24), ended_at: minutesAgo(60 * 19), ...(context && { context }) });
}
const archived = archivedWith();
function stateWith(context?: SessionContext) {
  return snapshotState({
    consoles: [console_],
    projects: [project],
    sessions: [sessionOf("s-console", console_.id, undefined, "Hub", "idle"), archivedWith(context)],
  });
}

// The three tones of the header badge's ring, and the agent that reported a size but no window.
const warningContext = contextOf(65);
const dangerContext = contextOf(91);
const accentContext = contextOf(30);
const windowlessContext: SessionContext = { tokens: 130_000 };

// A failure the application has no words for, so the pane shows its generic sentence and the raw
// message behind Details.
const historyFailure = { code: "internal_error", params: { detail: "failed to read the session record: input/output error" }, message: "failed to read the session record: input/output error" };

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
      "I will start with the image, then look at the [cache headers](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control).",
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
const preferences = { panelStacks: { project: { open: [], shares: {} } }, sidebarConsole: console_.id, sidebarFocus: `project:${project.id}` } satisfies Scenario["preferences"];
const openHistory: NonNullable<Scenario["steps"]> = [(ui) => ui.press(ui.session(archived.title))];

const base = { group: GROUP, category: "normal", state: stateWith(warningContext), preferences } satisfies Partial<Scenario>;

export const sessionHistoryScenarios: Scenario[] = [
  {
    ...base,
    id: "history-loaded",
    title: "Archived session history",
    description:
      "The pane shows user messages and agent replies together in place of a terminal. Octoboard briefs, notices and reports are hidden, while the agent's replies to them remain. User messages have a neutral bubble and agent replies have a softly accent-tinted bubble; both keep the standard short shadow and the same body font. Reply headings stay at the body size in semibold; lists, inline code and tables keep the conversation scale, while fenced code keeps its monospace font and code palette. Scroll for the code blocks and table. The GitHub document preference does not change the reply styling. User messages are labelled You, with a Chinese message at the end. The long message wraps and the long path breaks inside the column. The header holds the context badge, 130K with a warning ring for 65% used, followed by the labelled archive and creation times on two lines; no reply switch is present. Hover the badge for the Context window card. The floating bar says the session is archived and offers Reopen; the last message scrolls clear of it.",
    preferences: { ...preferences, markdownStyle: "github" },
    sessionHistory: { [archived.id]: MESSAGES },
    steps: openHistory,
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
    title: "Archived session with no conversation messages",
    description: "The record contains only Octoboard briefs, notices and reports, so the pane shows the normal empty state without displaying them. The header keeps the archive and creation times visible. The context badge beside the heading, 60K, has an accent ring for the 30% used.",
    state: stateWith(accentContext),
    sessionHistory: { [archived.id]: MESSAGES.filter((message) => message.speaker === "octoboard") },
    steps: openHistory,
  },
  {
    ...base,
    id: "history-loading",
    title: "Archived session history loading",
    description: "The daemon has not answered yet: a spinner and \"Loading history…\" fill the pane. The archive and creation times stay visible in the header above the loading state. The floating bar already offers Reopen. The agent reported no window, so the badge beside the heading shows only the compact size, 130K, with no ring.",
    state: stateWith(windowlessContext),
    sessionHistory: { [archived.id]: "pending" },
    steps: openHistory,
  },
  {
    ...base,
    id: "history-unavailable",
    title: "Archived session history unavailable",
    description: "The agent's record of the session is missing or unreadable: a silent, neutral empty panel with a muted glyph, the heading and the reason, and under it the remedy of reopening the session to see its terminal. The archive and creation times remain visible in the header above the unavailable state. It is no failure, so there is no danger glyph, no Try again, since reading it again would not help, and no Details, since there is no raw text.",
    state: stateWith(dangerContext),
    sessionHistory: { [archived.id]: "unavailable" },
    steps: openHistory,
  },
  {
    ...base,
    category: "exceptional",
    id: "history-failed",
    title: "Archived session history request failed",
    description: "The pane takes the empty-state shape with a danger glyph, the heading and the application's generic sentence for an unexpected failure under it. The archive and creation times stay visible in the header above the failure. Try again, the primary button, requests history again, and the secondary Details holds the daemon's raw message; the floating bar still offers Reopen.",
    sessionHistory: { [archived.id]: historyFailure },
    steps: openHistory,
  },
];
