import { describing, type Matcher } from "../interact";
import type { Scenario } from "../scenario";
import { consoleOf, pageOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Window layout";

// `railState` has five consoles covering everything the rail shows per console: a raised hand (Main,
// which has two waiting sessions there, and the one with the long name, which has one, so the waiting
// count reads 3 across both), work in progress (Operations), a session merely running (Billing), and
// nothing going on (Docs). Main and Website are also borrowed by `layout-waiting-overflow`, which
// gives them sessions of its own.
const main = consoleOf("c-main", "Main");
const operations = consoleOf("c-ops", "Operations");
const billing = consoleOf("c-billing", "Billing");
const docs = consoleOf("c-docs", "Docs");
const lab = consoleOf("c-lab", "A console with a name that is far too long for the rail");
const website = projectOf("p-web", main.id, "Website");
const deploys = projectOf("p-deploys", operations.id, "Deploys");
const invoices = projectOf("p-invoices", billing.id, "Invoices");
const manual = projectOf("p-manual", docs.id, "User manual");
const experiments = projectOf("p-experiments", lab.id, "Experiments");
const hub = sessionOf("s-hub", main.id, undefined, "Hub", "waiting_user", { colour: "teal" });
const railState = snapshotState({
  consoles: [main, operations, billing, docs, lab],
  projects: [website, deploys, invoices, manual, experiments],
  sessions: [
    hub,
    sessionOf("s-web-1", main.id, website.id, "Fix the summary layout", "working"),
    sessionOf("s-web-2", main.id, website.id, "Update the dependencies", "idle"),
    sessionOf("s-web-3", main.id, website.id, "Asks which region to use", "waiting_user"),
    sessionOf("s-ops-hub", operations.id, undefined, "Operations hub", "idle", { colour: "azure" }),
    sessionOf("s-ops-1", operations.id, deploys.id, "Roll back the last deploy", "working"),
    sessionOf("s-bill-1", billing.id, invoices.id, "Reconcile March", "idle"),
    sessionOf("s-docs-1", docs.id, manual.id, "Old draft", "archived"),
    sessionOf("s-lab-1", lab.id, experiments.id, "Asks which branch to use", "waiting_user"),
  ],
  pages: { [hub.id]: [pageOf("pg-hub", hub.id, "<h1>Report</h1><p>Two sessions need you.</p>", 5)] },
});
// More consoles than the rail holds in any window, so its column of them scrolls whatever the
// window's height: forty tiles are taller than a browser window the gallery is opened in.
const crowd = Array.from({ length: 40 }, (_, i) => consoleOf(`c-crowd-${i}`, `Console ${i + 1}`));
const crowdState = snapshotState({
  consoles: crowd,
  projects: [projectOf("p-crowd", crowd[0].id, "Website")],
  sessions: [
    sessionOf("s-crowd-hub", crowd[0].id, undefined, "Hub", "idle", { colour: "teal" }),
    sessionOf("s-crowd-1", crowd[0].id, "p-crowd", "Fix the summary layout", "working"),
  ],
});
/** The rail's button for a console: its accessible name is the console's whole name, alone or
 * followed by punctuation and what is going on in it, whichever language writes that. A name
 * followed by a letter or a space is another control's ("Operations hub" is a session, not the
 * console "Operations"); one followed by punctuation of its own is not told apart. */
const avatar = (name: string): Matcher =>
  describing(`avatar(${name})`, (label) => label === name || (label.startsWith(name) && /^\p{P}/u.test(label.slice(name.length))));

export const layoutScenarios: Scenario[] = [
  {
    id: "layout-rail-consoles",
    group: GROUP,
    title: "Rail with five consoles",
    description:
      "Back and Forward start enabled after visits to Hub, Fix the summary layout, Operations and Roll back the last deploy, then Back. Five avatars cover waiting, working, running and inactive consoles, including a long name. Hover them for full names and activity tooltips, or right-click for actions. Hide the docked sidebar, then hover avatars or the sidebar toggle for previews; keyboard focus still opens avatar tooltips. Select Hub, hide the report and hover the window's end edge for its preview. Drag either pane to its maximum. At Narrow, open the sessions drawer, select Hub, then open the report drawer.",
    state: railState,
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.wait(500),
      (ui) => ui.hoverTitledControl(avatar(lab.name)),
      (ui) => ui.press(avatar(billing.name)),
      (ui) => ui.wait(500),
      (ui) => ui.hoverTitledControl(avatar(billing.name)),
      (ui) => ui.press(avatar(main.name)),
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.press(avatar(operations.name)),
      (ui) => ui.press(ui.session("Roll back the last deploy")),
      (ui) => ui.press(ui.t("titleBar.back")),
      (ui) => ui.wait(300),
    ],
  },
  {
    id: "layout-rail-crowded",
    group: GROUP,
    title: "More consoles than the rail holds",
    description:
      "More consoles than the column holds, which scrolls with no scrollbar to say so: the avatars fade at the bottom edge, where more are waiting. Scroll it and both edges fade until the end, where that edge stops. Rail with five consoles shows the same column with everything in view, and no fade.",
    state: crowdState,
  },
  {
    id: "layout-waiting-overflow",
    group: GROUP,
    title: "More than 99 raised hands",
    description:
      "The rail's waiting count at its limit: a hundred and five sessions waiting in one console, so the count shows its overflow label and the glyph is cut out around the wider pill.",
    state: snapshotState({
      consoles: [main],
      projects: [website],
      sessions: Array.from({ length: 105 }, (_, i) =>
        sessionOf(`s-${i}`, main.id, website.id, `Needs an answer ${i + 1}`, "waiting_user"),
      ),
    }),
  },
];
