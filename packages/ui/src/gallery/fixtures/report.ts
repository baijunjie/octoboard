import type { Page } from "../../protocol";
import type { Scenario } from "../scenario";
import { consoleOf, minutesAgo, pageOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Report panel";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const sessions = [
  sessionOf("s-hub", console_.id, undefined, "Hub", "idle"),
  sessionOf("s-1", console_.id, project.id, "Add idempotency keys", "working"),
];

const summary = (title: string) => `<!doctype html><html><head><title>${title}</title></head><body>
<h1>${title}</h1>
<p>Two sessions finished since the last report. Nothing needs your attention.</p>
<table><thead><tr><th>Session</th><th>Result</th></tr></thead>
<tbody><tr><td>Add idempotency keys</td><td>Merged</td></tr><tr><td>Audit the retry path</td><td>Needs review</td></tr></tbody></table>
<ul><li>Tests: 214 passed</li><li>Coverage: 91%</li></ul>
</body></html>`;

const form = `<!doctype html><html><head><title>Choose a release</title></head><body>
<h1>Which release should the hub cut?</h1>
<form>
  <p><label>Version <input type="text" name="version" value="1.4.0"></label></p>
  <p><label><input type="radio" name="channel" value="stable" checked> Stable</label>
     <label><input type="radio" name="channel" value="beta"> Beta</label></p>
  <p><label>Notes<br><textarea name="notes" rows="3" cols="30">Ship it.</textarea></label></p>
  <p><button type="submit">Submit</button></p>
</form>
</body></html>`;

const pages: Page[] = [
  pageOf("pg-1", console_.id, summary("Morning summary"), 60 * 20),
  pageOf("pg-2", console_.id, summary("Midday summary"), 60 * 8),
  pageOf("pg-3", console_.id, form, 12),
];

const withPages = (list: Page[]) =>
  snapshotState({ consoles: [console_], projects: [project], sessions, pages: { [console_.id]: list } });

export const reportScenarios: Scenario[] = [
  {
    id: "report-no-pages",
    group: GROUP,
    title: "No pages yet",
    state: withPages([]),
    steps: [(ui) => ui.press(ui.hub())],
  },
  {
    id: "report-pages",
    group: GROUP,
    title: "Several pages, a form on the newest",
    description: "The newest page is a form, which can be submitted (the fixture accepts it).",
    state: withPages(pages),
    steps: [(ui) => ui.press(ui.hub()), (ui) => ui.wait(500)],
  },
  {
    id: "report-history",
    group: GROUP,
    title: "A history page, read-only",
    description: "Paged back one page: the read-only badge, with the page's controls disabled.",
    state: withPages(pages),
    steps: [(ui) => ui.press(ui.hub()), (ui) => ui.press(ui.t("report.previous")), (ui) => ui.wait(500)],
  },
  {
    id: "report-one-old-page",
    group: GROUP,
    title: "One page, a day old",
    state: withPages([{ ...pages[0], created_at: minutesAgo(60 * 30) }]),
    steps: [(ui) => ui.press(ui.hub()), (ui) => ui.wait(500)],
  },
];
