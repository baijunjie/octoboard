import { AGENT_OPTIONS } from "../agents";
import { gitBadgeAriaLabel } from "../gitStatusLabel";
import { format, type MessageArgs, type MessageKey } from "../i18n/catalog";
import type { Language } from "../i18n/languages";
import { panelId, type PanelKind } from "../panels/stackState";
import { DORMANT_STATUSES, LIVE_STATUSES, type AccountQuota, type Agent, type AgentAvailability, type Event as DaemonEvent, type GitStatus } from "../protocol";
import { activityLabel, statusLabel } from "../sessionLabel";
import { eventKey, type Binding } from "../shortcuts/binding";
import { ACTIVITIES } from "../sidebar/order";
import type { Daemon, State } from "../store";
import type { TerminalElement } from "../terminal/TerminalController";
import { IS_MAC } from "../useWindowShortcut";
import { STATUS_LABELS, type StatusKey } from "../viewer/statusMarks";

/** Everything a pressable thing is looked up among. A hand-built row is a `role="button"`, a tag
 * in a tag group is a `role="row"`, and a checkbox or radio is its (visually hidden) input: pressing
 * the label HeroUI draws it in does not toggle it. The `summary` of a `details` is a disclosure's
 * control, such as an error alert's "Error details". */
const PRESSABLE =
  "button, [role=button], [role=menuitem], [role=menuitemradio], [role=tab], [role=option], [role=row], a[href], input[type=checkbox], input[type=radio], summary, [role=separator][tabindex]";

/** The name a control is found by: its `aria-label`, otherwise its text (a menu item uses its labelled span), and for a checkbox, radio or switch
 * the text of its label, or of the element its `aria-labelledby` names when the label stands beside
 * it. Deliberately not the browser's accessible-name computation, which folds in the label of a
 * `role="img"` child — such a child inside a control does not enter the name a `Matcher` is
 * handed. */
function nameOf(element: Element): string {
  let label: string | undefined;
  if (element.matches('[role="menuitem"], [role="menuitemradio"]')) {
    // The menu label excludes its trailing keyboard hint and description.
    const labelledBy = element.getAttribute("aria-labelledby");
    label = labelledBy
      ? labelledBy.split(/\s+/).map((id) => element.ownerDocument.getElementById(id)?.textContent ?? "").join(" ")
      : element.querySelector("[data-slot=label]")?.textContent ?? undefined;
  }
  if (element instanceof HTMLInputElement) {
    // A switch's label may hold only its control, with the words in a sibling it is labelled by.
    const labelledBy = element.getAttribute("aria-labelledby");
    label = element.labels?.[0]?.textContent || (labelledBy ? element.ownerDocument.getElementById(labelledBy)?.textContent : undefined) || undefined;
  }
  return withoutIsolates(element.getAttribute("aria-label") ?? label ?? element.textContent ?? "").trim();
}

/** The text of `node` as assistive technology reads it: what sits in an `aria-hidden` subtree is left out,
 * so a label that shows a shorter text and carries the full name apart (`TextInput`'s `labelContent`)
 * reads as that name alone. */
function accessibleText(node: Node): string {
  if (node instanceof Element && node.getAttribute("aria-hidden") === "true") return "";
  if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? "";
  return Array.from(node.childNodes, accessibleText).join("");
}

/** A name without its bidi isolate characters (U+2066 to U+2069). The app puts them round a value
 * substituted into a sentence, and they are invisible, so a step names a control without them. */
function withoutIsolates(text: string): string {
  return text.replace(/[\u2066-\u2069]/g, "");
}

/** The text of a session agent mark's tooltip that has no list: the agent's name, then the account as
 * a badge, whose comma only assistive technology reads (`StatusBadge`). */
function agentTooltipText(agent: string, account: string | undefined): string {
  return account === undefined ? agent : `${agent}, ${account}`;
}

/** What a session agent mark's labelled-list tooltip says, as `Claude Code | Account: Work | Model:
 * Opus 5.5 | Effort: high`: the agent's name, then each label with its value, in the window's language,
 * with the account left out for a session that recorded none. */
function agentDetailsText(t: Ui["t"], agent: string, details: AgentDetails): string {
  const rows = [
    ...(details.account === undefined ? [] : [[t("session.agentTooltip.account"), details.account]]),
    [t("session.agentTooltip.model"), details.model],
    [t("session.agentTooltip.effort"), details.effort],
  ];
  return [agent, ...rows.map(([label, value]) => `${label}: ${value}`)].join(" | ");
}

/** The same text read off the open tooltip: the first line of the box holding the list is the agent, and each `dt` pairs with its
 * `dd`. A tooltip with no list reads as its whole text, which no expectation matches. */
function shownAgentDetails(tooltip: Element | null): string {
  const list = tooltip?.querySelector("dl");
  if (!tooltip || !list) return withoutIsolates(tooltip?.textContent ?? "").trim();
  const agent = list.parentElement?.firstElementChild?.textContent ?? "";
  const rows = Array.from(list.querySelectorAll("dt"), (term) => `${term.textContent}: ${term.nextElementSibling?.textContent ?? ""}`);
  return withoutIsolates([agent, ...rows].join(" | ")).trim();
}

/** The values a session agent mark's list tooltip shows. */
export type AgentDetails = { account?: string; model: string; effort: string };

/** The name a region or panel is given by `aria-labelledby` (the text of the element it names) or `aria-label`, with
 * bidi isolates removed and trimmed; `undefined` where it has neither. */
function labelledNameOf(doc: Document, element: Element): string | undefined {
  const label = element.getAttribute("aria-labelledby");
  const name = label ? doc.getElementById(label)?.textContent : element.getAttribute("aria-label");
  return name == null ? undefined : withoutIsolates(name).trim();
}

/** An exact name or a test of the name. There is no pattern: a pattern is written in one human
 * language and matches nothing in the others, and a name that is not copy (a branch, a file) is a
 * string or a test of its own. */
export type Matcher = string | ((name: string) => boolean);

/** A test of the name that says what it looks for, which is what a failed step reports in place of
 * the test's source. */
export const describing = (description: string, test: (name: string) => boolean): Matcher =>
  Object.assign(test, { toString: () => description });

function matches(matcher: Matcher, name: string): boolean {
  return typeof matcher === "string" ? name === matcher : matcher(name);
}

/** Whether a control can be pressed. */
const isEnabled = (element: Element): boolean => !element.hasAttribute("disabled") && element.getAttribute("aria-disabled") !== "true";

/** What HeroUI marks a tooltip's trigger with. */
const TOOLTIP_TRIGGER = "[data-slot=tooltip-trigger]";

/** The agent terminal in the centre, which the terminal steps act on: the Terminal panel's shell
 * terminals are xterm instances too, and must not be matched in its place. */
const AGENT_TERMINAL = '[data-region="terminal"]';

const POLL_MS = 50;
const TIMEOUT_MS = 5000;

/** The app's global `--tooltip-delay`: a tooltip opens this long after the pointer comes to rest
 * on its trigger. */
const TOOLTIP_DELAY_MS = 500;
/** Left after the delay for the tooltip to mount, be placed and finish fading in. */
const TOOLTIP_SHOW_MS = 500;

/**
 * What a scenario's steps drive the mounted app with: they press controls the way a user would, by
 * name, so a dialog or a view that lives in component state opens through the app's own code. Names
 * are looked up in the language the window is in (`t`), so a step reads the same in every language.
 * A step may also move the fixture daemon itself (`dropConnection`), for a state the app's own
 * controls cannot be driven into.
 */
export interface Ui {
  /** The catalog's message in the window's language, for naming a control. */
  t: <K extends MessageKey>(key: K, ...args: MessageArgs<K>) => string;
  /** The sidebar row of the session titled exactly `title` (so "Hub" is not "Hub 2"), not its
   * actions menu — a console session's row included, since it is an ordinary session row. */
  session: (title: string) => Matcher;
  /** The row of the change to the file `name` in the Git mode's change list, whatever else it says
   * after its status (where a rename came from, that it is selected). `name` is the file's display
   * base name (`wireBaseName`, so an escaped `%25` is written `%`) and `status` the kind of change,
   * which the row says in the window's language. Rows are told apart by name and status only: a file
   * that is both staged and unstaged has two rows named alike, and this takes the first. */
  changeRow: (name: string, status: StatusKey) => Matcher;
  /** Presses the control named `matcher`, waiting for it to appear: the first in document order,
   * except that a control standing inside another of the same name wins over the one around it (a
   * section heading is a `role="row"` band holding the button that folds it, and pressing the band
   * does nothing). Mind a name that exists twice side by side: a tag already picked in the filter is
   * on the sidebar's heading row (earlier in the DOM) and in the filter picker, so pressing it
   * again to unpick it would hit the sidebar row. */
  press: (matcher: Matcher) => Promise<void>;
  /** Presses the menu item named `matcher`, for a name an open menu shares with a control elsewhere on the page (the
   * Settings item of an account's menu, and the rail's Settings button). */
  pressMenuItem: (matcher: Matcher) => Promise<void>;
  /** Presses the control named `matcher` with a finger: the touch pointer events a touch screen sends, then the
   * click that follows. For a control that opens its text in a popover on a touch, pen or virtual press and
   * shows it as a tooltip on mouse hover and keyboard focus, such as a help tip's "?": a real mouse press opens
   * nothing and Enter or Space keep the focus tooltip, so a scenario of the popover needs a touch. (`press`
   * clicks without pointer events, which is a virtual press: it opens the popover too, but is no touch.) It
   * returns once that popover is up, and fails if none opens, which also catches a control with no touch
   * popover. */
  touchPress: (matcher: Matcher) => Promise<void>;
  /** Taps the status glyph at the end of the top bar's breadcrumb with a finger and leaves the popover it
   * opens up, as `touchPress` does. The glyph is the one named `role="img"` in the bar, as
   * `hoverTopBarStatus` finds it; fails while no session is selected or the tap opens no popover. */
  tapTopBarStatus: () => Promise<void>;
  /** `press` with the platform's shortcut modifier (⌘, or Ctrl elsewhere) down for the press alone, as a user
   * pressing a control with the key held does: for what the modifier changes when the page has not seen it held
   * (a new-session "+", whose quick form a press with the modifier runs). */
  pressWithModifier: (matcher: Matcher) => Promise<void>;
  /** Presses the control named `matcher` inside the card of the `kind` panel, for a name two panels
   * share (each has its own close), where `press` would take the first in the stack. */
  pressInPanel: (kind: PanelKind, matcher: Matcher) => Promise<void>;
  pressInDiff: (matcher: Matcher) => Promise<void>;
  /** Presses every control named `matcher` that is there when the first appears, in document order:
   * for controls that keep one name whatever their state, such as the disclosures of a dialog's
   * options, where pressing the name twice would fold the first again. */
  pressEach: (matcher: Matcher) => Promise<void>;
  /** Opens a row’s context menu at a point within the named control. */
  contextMenu: (matcher: Matcher) => Promise<void>;
  /** Opens the terminal's edit menu once the terminal has rendered its output. */
  terminalContextMenu: () => Promise<void>;
  /** Presses the button that submits the open dialog, waiting for it to appear and be enabled: for a
   * dialog whose confirming button is named as a control behind it, which `press` would find first.
   * A disabled button (`isDisabled`, such as Confirm before a `typeToConfirm` word is typed) is
   * waited out by its native `disabled` attribute, which HeroUI sets beside `data-disabled`. */
  confirm: () => Promise<void>;
  /** Moves focus onto the scroll region named `name`, with the keyboard as its source, so that the
   * focus ring is drawn as it is for a user who tabbed there. Some regions are always there (the
   * crash screen's stack, the file viewer's reading surfaces); others only while their content
   * overflows the window, and are not found in a window tall enough to hold it: a dialog's body,
   * named by the dialog's title, and the startup screen's message. A step cannot press Tab itself:
   * a script's key event moves nothing. */
  tabToRegion: (name: Matcher) => Promise<void>;
  /** Scrolls a named grid, tree or list box to a settled offset without moving keyboard focus. */
  scrollCollection: (name: Matcher, top: number) => Promise<void>;
  /** Scrolls the tab panel named `name` (a Settings section, by its tab's name) to a settled offset without moving
   * keyboard focus: the first element inside it that scrolls vertically, the section's own scroller. An offset past
   * the end is clamped, and content that fits is left where it is, so a window wide or tall enough to hold the
   * section needs no other handling. */
  scrollPanel: (name: Matcher, top: number) => Promise<void>;
  /** Moves the pointer onto the indicator inside the first control named `host`, waiting for the
   * control to appear, and leaves it there, for the tooltip that shows only while the pointer is
   * on it. It returns once the app's tooltip delay has passed and the tooltip is up. The indicator
   * is the control's first `role="img"` descendant, which `nameOf` leaves out of the control's
   * name, so `host` names the control as everywhere else. It dispatches no matching `pointerout`,
   * so a second call leaves both indicators hovered. It drives react-aria's hover state
   * (`data-hovered`, `isHovered`) and not CSS `:hover`: what is built with a `hover:` or
   * `group-hover/<name>:` utility shows nothing under it, and the scenario passes all the same. */
  hoverIndicator: (host: Matcher) => Promise<void>;
  /** Moves the pointer onto the status glyph of the sidebar session row named `row`
   * (`session(title)`) and leaves it there, as `hoverIndicator` does. The row's glyph is decorative,
   * so it has no `role="img"` for `hoverIndicator` to find; it is taken as the row's first element
   * child when that is a tooltip trigger, which is how `SessionRow` lays it out. A row whose glyph
   * carries no tooltip (focus mode's card) fails rather than match the agent mark's trigger further
   * along. */
  hoverRowStatus: (row: Matcher) => Promise<void>;
  /** Moves the pointer onto the status glyph of the session titled `title` in the open confirmation
   * dialog's callout list, and leaves it there, as `hoverIndicator` does. Unlike a sidebar row's,
   * that glyph is the named `role="img"` and the list row's first element child. Fails while no
   * dialog lists such a session, and also when the tooltip that opens is not a status's: a glyph
   * drawn without a tooltip is the same `role="img"` first child, so the step would otherwise pass
   * over one that opens nothing. */
  hoverCalloutStatus: (title: string) => Promise<void>;
  /** Moves the pointer onto the agent icon of the session titled `title` in the open confirmation
   * dialog's callout list, and leaves it there, as `hoverIndicator` does: the list row's second
   * element child, after the status glyph. Fails while no dialog lists such a session, and also
   * unless the tooltip open afterwards names `agent` and, as a badge after it, `account`: an
   * icon drawn without a tooltip has no trigger to hover, so it is reported as not found, and a
   * tooltip left up from the status glyph beside it (hovered earlier, and never left) would
   * otherwise be taken for this one's. */
  hoverCalloutAgent: (title: string, agent: string, account?: string) => Promise<void>;
  /** Moves the pointer onto the status glyph at the end of the top bar's breadcrumb and leaves it
   * there, as `hoverIndicator` does. The glyph's tooltip trigger is the one named `role="img"`
   * in the bar; fails while no session is selected. */
  hoverTopBarStatus: () => Promise<void>;
  /** Moves the pointer onto the activity marker of the collapsed project row or the focus mode
   * switch strip chip named `host`, and leaves it there, as `hoverIndicator` does. The marker is
   * decorative and the last tooltip trigger in either, so it is taken as that; the step fails if
   * the tooltip that opens is not one of the three activities' (an expanded row, which draws no
   * marker, has none to hover), rather than leave whatever else was hovered up. The row's git badge
   * is a `role="presentation"` trigger as well, and the two are told apart by nothing but their
   * order in the row: the badge comes before the marker (`Sidebar.tsx`), which `hoverGitBadge`
   * avoids by matching the state glyph instead. */
  hoverActivityMarker: (host: Matcher) => Promise<void>;
  /** Moves the pointer onto the first control named `matcher` and leaves it there, as `hoverIndicator`
   * does, for a control whose tooltip repeats its accessible name, such as a rail avatar's. It is not
   * for a control whose tooltip says something else (an actions button, a pane toggle): the step fails
   * unless the tooltip that opens is the control's name, so hovering one whose tooltip stays shut (a
   * rail avatar while the hidden sidebar can float in, where the pointer floats the sidebar instead)
   * is not passed over. */
  hoverTitledControl: (matcher: Matcher) => Promise<void>;
  /** Moves the pointer onto the first control named `host` and leaves it there, for a control that
   * floats a panel in while the pointer rests on it (the rail's quota entry), and returns once the
   * dialog named `panel` is up. Fails if none opens, which also catches a control whose panel needs a
   * press. The pointer's events are those of a mouse, so the panel is the hovered one and no press
   * pins it. */
  hoverPanel: (host: Matcher, panel: Matcher) => Promise<void>;
  /** Moves the mouse pointer onto the status mark (a `StatusMark`'s button, of any tone) or help tip's "?" named
   * `matcher`, any button that opens a popover hint, and leaves it there, for the popover that opens after the pointer has rested on it, with no focus moved. It returns
   * once that mark's own popover is up and fails if none opens, or if what opens is the press form (`aria-modal` not
   * `"false"`): a press, which `press` makes, is the other way to open it. Inside a floating panel, hover
   * the panel first, as the pointer then goes from one to the other without leaving. */
  hoverStatusMark: (matcher: Matcher) => Promise<void>;
  /** Taps the status mark or help tip named `matcher` with a finger, as a touch screen sends it, and returns once that mark's
   * popover is up in its press form (modal, focus inside). A mouse click opens nothing on a mark, so this is how a
   * person who cannot hover opens it (the keyboard's Enter is the other: `focusControl`, then `key`); `press` is a
   * virtual press and opens it too, but is no input a person has. Fails if none opens, or if what opens is the
   * hover form. */
  touchPressStatusMark: (matcher: Matcher) => Promise<void>;
  /** Moves the pointer onto the first control named `toggle`, the toggle of a pane that is closed or
   * hidden (the sidebar's with the docked sidebar hidden, or a panel kind's with no panel open),
   * which floats the pane in over the content, then on to the pane (the right pane's first card), which
   * keeps it there for as long as the pointer rests on it, as a user would reach its content; no
   * tooltip is left up over it. It returns once the floating pane has finished fading in. Fails if
   * no pane floats in, which also catches a toggle whose pane is docked. */
  floatPane: (toggle: Matcher) => Promise<void>;
  /** Moves the pointer onto the first control named `host` and leaves it there, as `hoverIndicator`
   * does, for a control whose tooltip is anchored to it rather than linked to it (a resize handle's,
   * or an agent tab's that is not installed), mounted only while the pointer is on the control and
   * portalled out of it, so its name says nothing of the tooltip and it is found by its
   * `data-anchored-tooltip` mark. It fails unless the tooltip that opens reads `tooltip`, which also
   * catches a control drawn without a tooltip (nothing opens) and another control's tooltip. `tooltip`
   * is the text as the window shows it, with a chip's text in place. */
  hoverAnchored: (host: Matcher, tooltip: string) => Promise<void>;
  /** Moves the pointer onto the status letter of the change row named `row` and leaves it there, as
   * `hoverIndicator` does, for the tooltip that names `status`. The letter is `aria-hidden` and its
   * tooltip is the anchored shape, which is mounted only while the pointer is on the letter and is
   * portalled out of the row, so it is found among the tooltips by its `data-anchored-tooltip` mark and
   * not by the row; the step fails if the one that opens is not `status`'s word, which also catches
   * a letter drawn without a tooltip (nothing opens) and a row of another status than the one named.
   * A name that spells the row out in English fails in the other languages, so the row is named
   * with `changeRow`, which builds it in the window's own. */
  hoverChangeStatus: (row: Matcher, status: StatusKey) => Promise<void>;
  /** Moves the pointer onto the status chip of the open file viewer's title, the one that reads `status`'s
   * word, and leaves it there, for the tooltip it carries where there is more to say than the word (a
   * conflict's kind, which is the subject's own wording, so the step takes no text to compare). The chip is
   * `aria-hidden` inside the trigger of its indicator tooltip, so it is found by its word among the
   * title's chips and the pointer reaches the trigger from it. It fails if no chip reads the word, or if
   * no tooltip opens. */
  hoverTitleStatus: (status: StatusKey) => Promise<void>;
  /** Presses the hovered status letter without moving the pointer or sending pointerleave. Fails
   * if its anchored tooltip survives the press that opens the change viewer. */
  pressChangeStatus: (row: Matcher) => Promise<void>;
  /** Moves the pointer onto one part of the branch badge whose state is `status`, and leaves it
   * there, as `hoverIndicator` does. The badge has one tooltip for the glyph, branch and fixed
   * counts. Only the branch line fades and runs, so which of glyph or line the pointer
   * lands on is the scenario's to choose (`over`); it fails where the badge has no line, such as a
   * lone glyph. With `row` it is the badge of the sidebar project row named `row`, whose own name
   * must fold the badge's sentence in; without it, the badge in focus mode's header, found by its
   * own name. It fails unless the tooltip that opens is that sentence exactly, which also takes a
   * `status` that is not the one on screen. A badge that says "No commits yet" in visible text has
   * no tooltip, and fails here rather than hover the activity marker beside it. */
  hoverGitBadge: (status: GitStatus, over: "glyph" | "line", row?: Matcher) => Promise<void>;
  /** Moves the pointer onto the context size of the session card or row named `row` and leaves it
   * there, as `hoverIndicator` does, for the context window card. The size is the `role="img"`
   * trigger whose name starts with the "Context" heading, which a card's status glyph does not, so
   * a card or row with no size (hidden, or none reported) fails there, naming `row`. The step then
   * fails if the tooltip that opens does not carry the card's "Context window" title. */
  hoverSessionContext: (row: Matcher) => Promise<void>;
  /** Moves the pointer onto the agent's mark of the session row or focus mode card named `row`
   * (`session(title)`) and leaves it there, as `hoverIndicator` does, for the tooltip that names `agent`
   * and lists the account (`details.account`, left out for a session that recorded none), the model and
   * the effort. The mark is the last tooltip trigger drawn as a bare glyph; it fails if the tooltip that
   * opens says anything else. */
  hoverSessionAgent: (row: Matcher, agent: string, details: AgentDetails) => Promise<void>;
  /** Moves the pointer onto the project filter's conditions on the Projects heading's line and
   * leaves it there, as `hoverIndicator` does, for the tooltip that carries the whole filter. The
   * strip is the box around the keyword and tag groups; it carries a tooltip only while it is
   * clipped, so the step fails when none opens, which is a filter that fits. It also fails when the
   * tooltip that opens does not hold the strip's chips, naming both. */
  hoverFilterConditions: () => Promise<void>;
  /** Moves the pointer onto the terminal's text `text` (a run of one terminal row, as the fake terminal
   * draws it, not copy) and leaves it there, for the tooltip an OSC 8 hyperlink there puts up with
   * its real target. Fails if no tooltip opens, which is also what a run of text that is not a
   * hyperlink gives. */
  hoverTerminalLink: (text: string) => Promise<void>;
  /** Moves the pointer onto the terminal's text `text`, as `hoverTerminalLink` does, for a plain-text
   * address, which has no tooltip but is underlined, on every row it spans, under the pointer.
   * Fails if the terminal does not show the pointer cursor of a link once the pointer is there. */
  hoverTerminalUrl: (text: string) => Promise<void>;
  /** Clicks the terminal's text `text`, once the pointer on it shows the pointer cursor of a link, as
   * `hoverTerminalUrl` waits for, and returns when a dialog is up over the window, for a file path
   * that opens in the file viewer. Fails if the text is not a link or no dialog opens. */
  clickTerminalLink: (text: string) => Promise<void>;
  /** Moves focus onto the first control named `matcher`, waiting for it to appear, with the keyboard
   * as its source, so that the focus ring is drawn as it is for a user who tabbed there, and a key
   * pressed next goes to it: for a row of a list, which a press would open rather than focus. */
  focusControl: (matcher: Matcher) => Promise<void>;
  /** Presses `key` on the focused control, for what a user closes or confirms with the keyboard. */
  key: (key: string) => Promise<void>;
  /** Presses the key combination `binding` on the focused control, with the modifiers it stands for on this
   * platform (⌘ for `primary` on an Apple keyboard, Ctrl elsewhere), for a shortcut the page records or matches. */
  chord: (binding: Binding) => Promise<void>;
  /** Focuses the text field named `label` (its `aria-label`, or its label's text as assistive technology
   * reads it), waiting for it to appear, so `type` has one to fill. */
  focus: (label: string) => Promise<void>;
  /** Types `text` into the focused field, waiting for a field to take focus, and replaces what it holds. */
  type: (text: string) => Promise<void>;
  /** Waits for `ms`, for an animation to settle. */
  wait: (ms: number) => Promise<void>;
  /** Drops the control connection with its automatic attempts already spent — the banner's Retry
   * offer — for a state only reachable once the window is showing something: a scenario that
   * starts at `closed` answers no request, so nothing a step would open over it, the file viewer
   * among them, ever lists. */
  dropConnection: () => Promise<void>;
  /** Changes which agents are added and which are installed while the window shows something, for what only a
   * state that changes under an open dialog reaches (the last agent removed elsewhere, a tool uninstalled): a
   * second window, or the machine, did it. Each is left as it was when not given; an agent not in `installed` is
   * then not installed. */
  setAgents: (change: { added?: Agent[]; installed?: Agent[] }) => Promise<void>;
  /** Archives the session `id` while the window shows something, for what only a session that ends under an open
   * dialog reaches (a console session archived from a second window while a new session was being set up to report
   * to it). Fails if the window has no such session. */
  archiveSession: (id: string) => Promise<void>;
  /** Replaces the readings of the accounts' quotas while the window shows something, for what only a reading that
   * lands under an open dialog reaches (the account being switched to turns out to be used up after the menu that
   * offered it was read). */
  setAccountQuotas: (accountQuotas: AccountQuota[]) => Promise<void>;
  /** Applies daemon events to the store, in order, as the connection does for those the daemon broadcasts, so a
   * collection changes under an open view the way another window or the machine changes it (an account added, a
   * terminal closed there). An event may be written as a function of the store's state by then, for a whole-list
   * event that must keep the rest of the list. It returns once the changes have had time to animate. */
  daemonEvents: (...events: (DaemonEvent | ((state: State) => DaemonEvent))[]) => Promise<void>;
}

/** The icon the git badge draws for `status`, by the name lucide gives its class (`GitBadge.tsx`
 * chooses it: a check, pull or push in flight, a failed check, a detached `HEAD`, else a branch). */
function gitGlyphOf(status: GitStatus): string {
  if (status.activity !== "idle") return "refresh-cw";
  if (status.error) return "circle-alert";
  return status.detached ? "git-commit-horizontal" : "git-branch";
}

/** What react-aria marks `data-exiting` while it plays an element's exit transition, and removes the
 * element when that ends. */
const EXITING = "[data-exiting]";

/**
 * Waits for what is closing to be gone, and says what stays if something does. An exit transition
 * that never ends leaves the thing it was removing on screen for good (a settings section left
 * behind by the one picked after it, because a scroll-driven animation on the panel never finishes),
 * and a capture taken afterwards looks like any other, so the page reports it instead.
 */
export async function stuckExit(doc: Document): Promise<string | undefined> {
  for (let waited = 0; waited <= TIMEOUT_MS; waited += POLL_MS) {
    if (!doc.querySelector(EXITING)) return undefined;
    await new Promise<void>((resolve) => setTimeout(resolve, POLL_MS));
  }
  const stuck = doc.querySelector(EXITING);
  if (!stuck) return undefined;
  const text = (stuck.textContent ?? "").trim().replace(/\s+/g, " ").slice(0, 40);
  return `<${stuck.tagName.toLowerCase()}> beginning “${text}”`;
}

/**
 * Waits, for at most `limit` ms, until the page's finite animations and transitions have ended, so a
 * capture taken once the page is ready shows the settled view rather than a frame of a dialog still
 * easing to its height. Infinite animations (a spinner) are part of a view and are not waited for,
 * and neither are scroll-driven ones, which end only when the user scrolls.
 */
export async function settleMotion(doc: Document, limit: number): Promise<void> {
  if (typeof doc.getAnimations !== "function") return;
  const running = () =>
    doc.getAnimations().filter((animation) => {
      if (animation.playState !== "running" || !(animation.timeline instanceof DocumentTimeline)) return false;
      return Number.isFinite(animation.effect?.getComputedTiming().endTime);
    });
  const spent = new Promise<void>((resolve) => setTimeout(resolve, limit));
  for (let pending = running(); pending.length > 0; pending = running()) {
    const ended = await Promise.race([Promise.allSettled(pending.map((animation) => animation.finished)).then(() => true), spent.then(() => false)]);
    if (!ended) return;
  }
}

export function createUi(doc: Document, language: Language, daemon?: Daemon): Ui {
  const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

  /** Polls for what `look` finds, up to the timeout, so a step can name something a press or a
   * render has not yet brought into the page. */
  const waitFor = async <T>(look: () => T | undefined, missing: string): Promise<T> => {
    for (let waited = 0; waited <= TIMEOUT_MS; waited += POLL_MS) {
      const found = look();
      if (found !== undefined) return found;
      await wait(POLL_MS);
    }
    throw new Error(missing);
  };

  /** Clicks what `look` finds once it is there; with `modifier` the click carries the shortcut modifier, whether
   * or not the page has seen it held. */
  const clickWhenFound = async (look: () => HTMLElement | undefined, missing: string, modifier = false): Promise<void> => {
    const element = await waitFor(look, missing);
    if (modifier) {
      element.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, detail: 0, metaKey: IS_MAC, ctrlKey: !IS_MAC }));
    } else element.click();
    // A submenu a click opens closes again shortly unless the pointer is on its trigger, which a
    // click never places. Put it there afterwards — before the click, the same events do not hold
    // it open — so the submenu stays to be seen or pressed, as it does for a pointer.
    if (element.getAttribute("aria-haspopup") === "menu") {
      const box = element.getBoundingClientRect();
      const init: PointerEventInit = {
        bubbles: true,
        cancelable: true,
        pointerType: "mouse",
        clientX: box.x + box.width / 2,
        clientY: box.y + box.height / 2,
      };
      for (const type of ["pointermove", "pointerover", "pointerenter"]) element.dispatchEvent(new PointerEvent(type, init));
    }
    // The press's own effects (a menu opening, a dialog mounting) land on the next frames.
    await wait(150);
  };

  const findAll = (matcher: Matcher, root: ParentNode = doc): HTMLElement[] => {
    const named = Array.from(root.querySelectorAll<HTMLElement>(PRESSABLE)).filter((element) => isEnabled(element) && matches(matcher, nameOf(element)));
    // A control that stands inside another of the same name is the one to press: a section heading
    // is a `role="row"` band holding the button that folds it, and the band does nothing.
    return named.filter((element) => !named.some((other) => other !== element && element.contains(other)));
  };
  const find = (matcher: Matcher): HTMLElement | undefined => findAll(matcher)[0];

  /** Puts the pointer on the terminal's text `text`, waiting for a row that holds it. */
  const pointTerminalText = async (text: string): Promise<{ screen: Element; clientX: number; clientY: number }> => {
    // The text comes from the terminal's buffer, which says the same under the DOM renderer and
    // under WebGL (whose `.xterm-rows` is empty), and the pointer goes to a cell computed from the
    // screen's box. xterm finds the link under the pointer from the coordinates of a `mousemove`
    // alone.
    const found = await waitFor(() => {
      const view = (doc.querySelector(`${AGENT_TERMINAL} .xterm`) as TerminalElement | null)?.octoboardTerminalText;
      if (!view) return undefined;
      for (let y = 0; y < view.rows(); y++) {
        const col = view.row(y).indexOf(text);
        if (col >= 0) return { view, col, y };
      }
      return undefined;
    }, `No terminal row holds ${text}`);
    const screen = doc.querySelector(`${AGENT_TERMINAL} .xterm-screen`);
    if (!screen) throw new Error("The terminal has no screen");
    const box = screen.getBoundingClientRect();
    const cellWidth = box.width / found.view.cols();
    const cellHeight = box.height / found.view.rows();
    const at = { clientX: box.x + (found.col + text.length / 2) * cellWidth, clientY: box.y + (found.y + 0.5) * cellHeight };
    screen.dispatchEvent(new MouseEvent("mousemove", { bubbles: true, ...at }));
    return { screen, ...at };
  };

  /** Puts the pointer on the terminal's text `text` and waits for xterm to give its link pointer
   * cursor, which it does whichever renderer draws the underline. The text is found and the pointer
   * moved again while it waits: a link is found from the coordinates of a `mousemove` alone, and a
   * busy page can redraw the text elsewhere (the terminal redraws at its settled width), or finish
   * finding the link, after the first. */
  const pointOnTerminalLink = async (text: string) => {
    for (let waited = 0; waited <= TIMEOUT_MS; waited += POLL_MS) {
      const at = await pointTerminalText(text);
      await wait(POLL_MS);
      if (doc.querySelector(`${AGENT_TERMINAL} .xterm-cursor-pointer`)) return at;
    }
    throw new Error(`Hovering ${text} in the terminal did not give xterm its link pointer cursor: it is not a link`);
  };

  /** Puts a mouse pointer on `element`. A tooltip opens only for a pointer, and react-aria takes the
   * modality from the last event it saw: the presses before it were clicks, which it reads as
   * virtual. React raises `onPointerEnter` from a bubbling `pointerover`. */
  const pointAt = (element: HTMLElement) => {
    for (const type of ["pointermove", "pointerover"]) {
      element.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerType: "mouse" }));
    }
  };

  /** Puts the pointer on what `look` finds, waiting for it to appear, and waits for its tooltip.
   * Returns what it hovered. */
  const hover = async (look: () => HTMLElement | null | undefined, missing: string): Promise<HTMLElement> => {
    const indicator = await waitFor(() => look() ?? undefined, missing);
    pointAt(indicator);
    await wait(TOOLTIP_DELAY_MS + TOOLTIP_SHOW_MS);
    return indicator;
  };

  /** Presses `element` with a finger: the pointer events react-aria's `usePress` reads, then the click
   * that ends the press. The box has a size, since react-aria reads a pointer event of no size as a virtual one. */
  const touch = (element: HTMLElement) => {
    const init: PointerEventInit = { bubbles: true, cancelable: true, button: 0, pointerId: 1, isPrimary: true, pointerType: "touch", width: 20, height: 20 };
    element.dispatchEvent(new PointerEvent("pointerdown", init));
    element.dispatchEvent(new PointerEvent("pointerup", init));
    element.dispatchEvent(new MouseEvent("click", { bubbles: true, button: 0, detail: 1 }));
  };

  /** The popover a touch press opens in place of a tooltip, drawn like it, once it is up: the touch popover
   * of an indicator or a control (`TouchPopover`, the surface itself the dialog) or the press form of a popover
   * hint (a dialog in the surface), never the hover form, which is non-modal. */
  const touchPopover = (): HTMLElement | undefined =>
    doc.querySelector<HTMLElement>(".tooltip[role=dialog], .tooltip > [role=dialog]:not([aria-modal=false])") ?? undefined;

  /** Touches what `look` finds, waiting for it to appear, then for the popover, and waits for it to finish fading in. */
  const touchAndWait = async (look: () => HTMLElement | undefined, missing: string, unopened: string): Promise<void> => {
    touch(await waitFor(look, missing));
    await waitFor(touchPopover, unopened);
    await wait(TOOLTIP_SHOW_MS);
  };

  /** The popover of the popover hint's trigger `mark` (a status mark, a help tip), once it is open: the popover is
   * labelled by the trigger's id or its dialog is named as the trigger is, so another trigger's popover, or a stale one, is not
   * taken for it. The trigger says it is open while it is. */
  const markPopover = (mark: HTMLElement, missing: string): Promise<HTMLElement> =>
    waitFor(() => {
      if (mark.getAttribute("aria-expanded") !== "true") return undefined;
      const label = mark.getAttribute("aria-label");
      return Array.from(doc.querySelectorAll<HTMLElement>(".tooltip")).find((candidate) => {
        const dialog = candidate.querySelector("[role=dialog]");
        return dialog !== null && (candidate.getAttribute("aria-labelledby") === mark.id || (label !== null && dialog.getAttribute("aria-label") === label));
      });
    }, missing);

  /** The row of the confirmation dialog's callout list for the session titled `title`: the title is
   * its last element child, after the status glyph, the agent icon and the screen-reader-only name. */
  const calloutRow = (title: string): HTMLElement | undefined =>
    Array.from(doc.querySelectorAll<HTMLElement>("[role=alertdialog] li, [role=dialog] li")).find(
      (item) => item.lastElementChild?.textContent?.trim() === title,
    );

  const t: Ui["t"] = (key, ...args) => format(language, key, (args as [Record<string, string | number>?])[0]);

  return {
    t,
    // A session row's name begins with its whole title, so "Hub" does not also match "Hub 2"; the
    // title comes before the agent and the status in every catalog, which `catalog.test.ts` holds.
    session: (title) => {
      const prefix = t("sidebar.session.ariaLabel", { title, agent: "\0", status: "\0" }).split("\0")[0];
      return describing(`session(${title})`, (name) => name.startsWith(prefix));
    },
    // Every form of a change row's name begins with `git.row`'s text, in every catalog, which is why
    // a prefix and not an equality; `catalog.test.ts` holds each catalog to it.
    changeRow: (name, status) => {
      const prefix = t("git.row", { name, status: t(`git.status.${status}`) });
      return describing(`changeRow(${name}, ${status})`, (label) => label.startsWith(prefix));
    },
    wait,
    async dropConnection() {
      if (!daemon) throw new Error("This scenario has no daemon behind it to drop the connection to");
      daemon.store.setState({ connectionState: "closed" });
      await wait(300);
    },
    async setAgents({ added, installed }) {
      if (!daemon) throw new Error("This scenario has no daemon behind it to change the agents of");
      const current = daemon.store.getState();
      daemon.store.setState({
        ...(added && { settings: { ...current.settings, added_agents: added } }),
        ...(installed && {
          agentAvailability: new Map<Agent, AgentAvailability>(
            AGENT_OPTIONS.map(({ value: agent }) => [
              agent,
              { default_account_dir: `/Users/dev/.${agent}`, ...current.agentAvailability.get(agent), agent, availability: installed.includes(agent) ? "available" : "unavailable" },
            ]),
          ),
        }),
      });
      await wait(300);
    },
    async archiveSession(id) {
      if (!daemon) throw new Error("This scenario has no daemon behind it to archive a session of");
      const session = daemon.store.getState().sessions.get(id);
      if (!session) throw new Error(`The window has no session ${id} to archive`);
      daemon.store.setState({ sessions: new Map(daemon.store.getState().sessions).set(id, { ...session, status: "archived", ended_at: Date.now() }) });
      await wait(300);
    },
    async setAccountQuotas(accountQuotas) {
      if (!daemon) throw new Error("This scenario has no daemon behind it to change the quota readings of");
      daemon.store.setState({ accountQuotas });
      await wait(300);
    },
    async daemonEvents(...events) {
      if (!daemon) throw new Error("This scenario has no daemon behind it to send events from");
      // Not imported above: the store loads after the gallery frame has registered its own listeners (`frame.tsx`).
      const { applyDaemonEvent } = await import("../store");
      for (const event of events) applyDaemonEvent(daemon.store, typeof event === "function" ? event(daemon.store.getState()) : event);
      await wait(300);
    },
    async key(key) {
      const target = doc.activeElement ?? doc.body;
      for (const type of ["keydown", "keyup"]) {
        target.dispatchEvent(new KeyboardEvent(type, { key, bubbles: true, cancelable: true }));
      }
      await wait(300);
    },
    async chord(binding) {
      const { code, ...modifiers } = eventKey(binding);
      const key = /^Key[A-Z]$/.test(code) ? (binding.shift ? code.slice(3) : code.slice(3).toLowerCase()) : code;
      const target = doc.activeElement ?? doc.body;
      for (const type of ["keydown", "keyup"]) {
        target.dispatchEvent(new KeyboardEvent(type, { key, code, ...modifiers, bubbles: true, cancelable: true }));
      }
      await wait(300);
    },
    async focusControl(matcher) {
      const control = await waitFor(() => find(matcher), `Nothing to focus matching ${String(matcher)}`);
      // react-aria reads what the focus came from off the last key event, so Tab is sent first.
      (doc.activeElement ?? doc.body).dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true }));
      control.focus();
      await wait(150);
    },
    async tabToRegion(matcher) {
      for (let waited = 0; waited <= TIMEOUT_MS; waited += POLL_MS) {
        const region = Array.from(doc.querySelectorAll<HTMLElement>("[role=region][tabindex]")).find((element) => {
          const name = labelledNameOf(doc, element);
          return name != null && matches(matcher, name);
        });
        if (region) {
          // react-aria reads what the focus came from off the last key event, so Tab is sent first.
          (doc.activeElement ?? doc.body).dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true }));
          region.focus();
          await wait(150);
          return;
        }
        await wait(POLL_MS);
      }
      throw new Error(`No scroll region named ${String(matcher)}: check the name, and for a region that exists only while its content overflows (a dialog's body, the startup screen's message) that the window is short enough for that`);
    },
    async scrollCollection(matcher, top) {
      const collection = await waitFor(
        () => Array.from(doc.querySelectorAll<HTMLElement>('[role="grid"], [role="treegrid"], [role="tree"], [role="listbox"]')).find((element) => matches(matcher, nameOf(element))),
        `No collection named ${String(matcher)} to scroll`,
      );
      collection.scrollTo({ top, behavior: "instant" });
      await wait(300);
    },
    async scrollPanel(matcher, top) {
      const scroller = await waitFor(() => {
        const panel = Array.from(doc.querySelectorAll<HTMLElement>("[role=tabpanel]")).find((element) => {
          const name = labelledNameOf(doc, element);
          return name != null && matches(matcher, name);
        });
        return panel && Array.from(panel.querySelectorAll<HTMLElement>("*")).find((element) => {
          const overflow = (doc.defaultView ?? window).getComputedStyle(element).overflowY;
          return overflow === "auto" || overflow === "scroll";
        });
      }, `No tab panel named ${String(matcher)} with a vertical scroller to scroll: check the name`);
      scroller.scrollTo({ top, behavior: "instant" });
      await wait(300);
    },
    async type(text) {
      const field = await waitFor(
        () => (doc.activeElement instanceof HTMLInputElement || doc.activeElement instanceof HTMLTextAreaElement ? doc.activeElement : undefined),
        "No text field took focus to type into",
      );
      // React tracks the value through the element's own setter, so it is set through the prototype's.
      const prototype = field instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(prototype, "value")?.set?.call(field, text);
      field.dispatchEvent(new Event("input", { bubbles: true }));
      await wait(150);
    },
    async focus(label) {
      const field = await waitFor(
        () =>
          Array.from(doc.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>("input, textarea")).find(
            (input) =>
              // The accessible name: the `aria-label`, or the text of the label as a screen reader reads it.
              [input.getAttribute("aria-label"), input.labels?.[0] ? accessibleText(input.labels[0]) : null].some(
                (name) => name != null && withoutIsolates(name).trim() === label,
              ),
          ),
        `No text field labelled ${label}`,
      );
      field.focus();
    },
    async hoverIndicator(host) {
      await hover(() => find(host)?.querySelector<HTMLElement>("[role=img]"), `Nothing to hover an indicator of, matching ${String(host)}`);
    },
    async hoverRowStatus(row) {
      await hover(() => {
        const found = find(row);
        if (!found) return undefined;
        const glyph = found.firstElementChild;
        // Found, so waiting would not change it: the row's glyph is one without a tooltip.
        if (!glyph?.matches("[role=presentation]")) {
          throw new Error(`The session row matching ${String(row)} has no tooltip on its status glyph to hover`);
        }
        return glyph as HTMLElement;
      }, `No session row matching ${String(row)} to hover the status glyph of`);
    },
    async hoverCalloutStatus(title) {
      await hover(() => {
        const glyph = calloutRow(title)?.firstElementChild;
        return glyph?.matches("[role=img]") ? (glyph as HTMLElement) : undefined;
      }, `No session titled ${title} in a dialog's callout list with a status glyph to hover`);
      // A glyph without `withTooltip` is the same `role="img"` first child, and hovering it opens nothing.
      const shown = doc.querySelector("[role=tooltip]")?.textContent;
      const statuses = [...LIVE_STATUSES, ...DORMANT_STATUSES];
      if (!shown || !statuses.some((status) => statusLabel(t, status) === shown)) {
        throw new Error(`The status glyph of ${title} has no tooltip: its last tooltip is ${shown ? `"${shown}"` : "none"}`);
      }
    },
    async hoverCalloutAgent(title, agent, account) {
      const tooltip = agentTooltipText(agent, account);
      await hover(() => {
        const icon = calloutRow(title)?.children[1];
        return icon?.matches("[role=presentation]") ? (icon as HTMLElement) : undefined;
      }, `No session titled ${title} in a dialog's callout list with an agent icon to hover`);
      const shown = withoutIsolates(doc.querySelector("[role=tooltip]")?.textContent ?? "").trim();
      if (shown !== tooltip) {
        throw new Error(`The agent icon of ${title} has no tooltip "${tooltip}": its last tooltip is ${shown ? `"${shown}"` : "none"}`);
      }
    },
    async hoverTopBarStatus() {
      await hover(() => doc.querySelector<HTMLElement>("[data-region=topbar] [role=img]"), "No status glyph in the top bar");
    },
    async hoverActivityMarker(host) {
      await hover(() => {
        const found = find(host);
        if (!found) return undefined;
        // The last trigger: a project row's git badge is a `role="presentation"` trigger too, and
        // comes before the marker. A row with no marker would end up on the badge, which the check
        // below then refuses.
        return Array.from(found.querySelectorAll<HTMLElement>("[role=presentation]")).at(-1);
      }, `No row or chip matching ${String(host)} with an activity marker to hover`);
      const shown = doc.querySelector("[role=tooltip]")?.textContent;
      if (!shown || !ACTIVITIES.some((activity) => activityLabel(t, activity) === shown)) {
        throw new Error(`The control matching ${String(host)} has no activity marker: its last tooltip is ${shown ? `"${shown}"` : "none"}`);
      }
    },
    async hoverTitledControl(matcher) {
      const control = await hover(() => find(matcher), `Nothing to hover matching ${String(matcher)}`);
      const shown = doc.querySelector("[role=tooltip]")?.textContent;
      if (!shown || withoutIsolates(shown).trim() !== nameOf(control)) {
        throw new Error(`The control matching ${String(matcher)} has no tooltip of its name: its last tooltip is ${shown ? `"${shown}"` : "none"}`);
      }
    },
    async hoverPanel(host, panel) {
      const control = await waitFor(() => find(host), `Nothing to hover matching ${String(host)}`);
      pointAt(control);
      await waitFor(
        () => Array.from(doc.querySelectorAll<HTMLElement>("[role=dialog]")).find((dialog) => matches(panel, labelledNameOf(doc, dialog) ?? "")),
        `The control matching ${String(host)} floated in no dialog matching ${String(panel)} under the pointer`,
      );
    },
    async hoverStatusMark(matcher) {
      const mark = await waitFor(() => find(matcher), `Nothing to hover matching ${String(matcher)}`);
      pointAt(mark);
      const popover = await markPopover(mark, `The mark matching ${String(matcher)} opened no popover under the pointer`);
      if (popover.querySelector("[role=dialog]")?.getAttribute("aria-modal") !== "false") {
        throw new Error(`The popover of the mark matching ${String(matcher)} is not the hover form (it is modal)`);
      }
    },
    async touchPressStatusMark(matcher) {
      const mark = await waitFor(() => find(matcher), `Nothing to press matching ${String(matcher)}`);
      touch(mark);
      const popover = await markPopover(mark, `A touch press of the mark matching ${String(matcher)} opened no popover`);
      if (popover.querySelector("[role=dialog][aria-modal=false]")) {
        throw new Error(`The popover of the mark matching ${String(matcher)} is the hover form, not the press form`);
      }
      await wait(TOOLTIP_SHOW_MS);
    },
    async hoverAnchored(host, tooltip) {
      await hover(() => find(host), `Nothing to hover matching ${String(host)}`);
      const shown = withoutIsolates(
        await waitFor(
          () => doc.querySelector("[data-anchored-tooltip]")?.textContent ?? undefined,
          `The control matching ${String(host)} has no tooltip "${tooltip}": its anchored tooltip is none`,
        ),
      ).trim();
      if (shown !== tooltip) {
        throw new Error(`The control matching ${String(host)} has no tooltip "${tooltip}": its anchored tooltip is "${shown}"`);
      }
    },
    async floatPane(toggle) {
      const control = await waitFor(() => find(toggle), `Nothing to hover matching ${String(toggle)}`);
      pointAt(control);
      const pane = await waitFor(() => {
        // A docked pane is part of the layout; only a floating one is fixed over it. The right
        // pane's frame is rendered fixed only while it floats; the sidebar's is fixed whenever it is
        // hidden-docked, so the visibility check tells its floating from its being away.
        // It is settled once its fade-up has ended: fully opaque, visible and with no transition running.
        for (const found of doc.querySelectorAll<HTMLElement>("[data-pane=aside], [data-pane=sidebar]")) {
          const style = found.ownerDocument.defaultView?.getComputedStyle(found);
          const settled = style?.position === "fixed" && style.visibility === "visible" && style.opacity === "1" && (found.getAnimations?.().length ?? 0) === 0;
          if (settled) return found;
        }
        return undefined;
      }, `The control matching ${String(toggle)} floated in no pane under the pointer`);
      const init = { bubbles: true, pointerType: "mouse" } as const;
      // The floating pane's frame passes pointer events through, so the pointer lands on a card.
      const target = pane.querySelector<HTMLElement>("[data-panel-card]") ?? pane;
      control.dispatchEvent(new PointerEvent("pointerout", { ...init, relatedTarget: target }));
      target.dispatchEvent(new PointerEvent("pointerover", { ...init, relatedTarget: control }));
      await wait(TOOLTIP_DELAY_MS + TOOLTIP_SHOW_MS);
    },
    async hoverSessionContext(row) {
      const heading = t("session.context.heading", { count: 2, tokens: "\0" }).split("\0")[0];
      await hover(
        () =>
          Array.from(find(row)?.querySelectorAll<HTMLElement>("[role=img]") ?? []).find((element) =>
            withoutIsolates(element.getAttribute("aria-label") ?? "").includes(withoutIsolates(heading)),
          ),
        `No session card or row matching ${String(row)} with a context size to hover`,
      );
      // The mark's name starts with the heading; the tooltip is the context window card, which does not.
      const title = t("session.context.title");
      const shown = doc.querySelector("[role=tooltip]")?.textContent ?? "";
      if (!withoutIsolates(shown).includes(withoutIsolates(title))) {
        throw new Error(`The context size of ${String(row)} did not open its tooltip, which holds ${title}: ${shown === "" ? "no tooltip" : shown} is up`);
      }
    },
    async hoverSessionAgent(row, agent, details) {
      const tooltip = agentDetailsText(t, agent, details);
      await hover(
        () =>
          Array.from(find(row)?.querySelectorAll<HTMLElement>("[role=presentation]") ?? [])
            .filter((trigger) => trigger.firstElementChild?.matches("svg[aria-hidden=true]"))
            .at(-1),
        `No session row or card matching ${String(row)} with an agent mark to hover`,
      );
      const shown = shownAgentDetails(doc.querySelector("[role=tooltip]"));
      if (shown !== tooltip) {
        throw new Error(`The agent mark of ${String(row)} has no tooltip "${tooltip}": its last tooltip is ${shown ? `"${shown}"` : "none"}`);
      }
    },
    async hoverFilterConditions() {
      const strip = await hover(() => {
        const names = [t("sidebar.filter.keyword"), t("sidebar.filter.picked")];
        return Array.from(doc.querySelectorAll<HTMLElement>("[role=group]")).find((group) => names.includes(group.getAttribute("aria-label") ?? ""))
          ?.parentElement ?? undefined;
      }, "No project filter conditions beside the Projects heading to hover");
      const tooltip = await waitFor(() => doc.querySelector("[data-anchored-tooltip]") ?? undefined, "The project filter's conditions opened no tooltip: they fit the heading's line");
      const chipsOf = (root: Element, selector: string) => Array.from(root.querySelectorAll(selector), (chip) => chip.textContent).join(", ");
      // The strip's chips sit in its named groups; the tooltip's sit in it directly.
      const [expected, shown] = [chipsOf(strip, "[role=group] span"), chipsOf(tooltip, "span")];
      if (shown !== expected) {
        throw new Error(`The project filter's conditions (${expected}) opened another tooltip: it holds ${shown ? `"${shown}"` : "no chips"}`);
      }
    },
    async hoverTerminalLink(text) {
      await pointTerminalText(text);
      await wait(TOOLTIP_DELAY_MS + TOOLTIP_SHOW_MS);
      await waitFor(
        () => doc.querySelector("[data-anchored-tooltip]") ?? undefined,
        `Hovering ${text} in the terminal opened no tooltip: it is not a hyperlink`,
      );
    },
    async hoverTerminalUrl(text) {
      await pointOnTerminalLink(text);
    },
    async clickTerminalLink(text) {
      const { screen, clientX, clientY } = await pointOnTerminalLink(text);
      // xterm activates a link on the mouse-up that follows a mouse-down on the same one.
      for (const type of ["mousedown", "mouseup"]) {
        screen.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0, clientX, clientY }));
      }
      await waitFor(() => doc.querySelector("[role=dialog]") ?? undefined, `Clicking ${text} in the terminal opened no dialog`);
      await wait(600);
    },
    async hoverChangeStatus(row, status) {
      await hover(() => find(row)?.querySelector<HTMLElement>("[data-variant]"), `No change row matching ${String(row)} with a status letter to hover`);
      const shown = doc.querySelector("[data-anchored-tooltip]")?.textContent;
      if (shown !== t(STATUS_LABELS[status])) {
        throw new Error(`The row matching ${String(row)} did not open the tooltip for ${status} (${t(STATUS_LABELS[status])}): ${shown ? `"${shown}"` : "none"} is up`);
      }
    },
    async hoverTitleStatus(status) {
      const word = t(STATUS_LABELS[status]);
      await hover(
        () => Array.from(doc.querySelectorAll<HTMLElement>("[role=dialog] [data-variant]")).find((chip) => chip.textContent === word),
        `The open viewer's title has no status chip reading ${word}`,
      );
      if (!doc.querySelector("[role=tooltip]")?.textContent?.trim()) throw new Error(`The title's ${word} chip opened no tooltip`);
    },
    async pressChangeStatus(row) {
      const chip = await waitFor(() => find(row)?.querySelector<HTMLElement>("[data-variant]") ?? undefined, `No change row matching ${String(row)} with a status letter to press`);
      chip.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
      chip.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
      chip.click();
      await wait(150);
      await waitFor(() => doc.querySelector("[data-anchored-tooltip]") ? undefined : true, `The status tooltip survived pressing ${String(row)} without pointerleave`);
    },
    async hoverGitBadge(status, over, row) {
      const sentence = gitBadgeAriaLabel(language, t, status);
      const glyph = `svg.lucide-${gitGlyphOf(status)}`;
      await hover(
        () => {
          let trigger: HTMLElement | undefined;
          if (row === undefined) {
            const named = Array.from(doc.querySelectorAll<HTMLElement>("[data-region=sidebar] [role=img]")).find(
              (element) => element.getAttribute("aria-label") === sentence,
            );
            // Found, so waiting would not change it: the badge is a plain box, with no trigger.
            if (named && !named.matches(TOOLTIP_TRIGGER)) throw new Error("The git badge in focus mode's header carries no tooltip to hover");
            trigger = named;
          } else {
            const found = find(row);
            if (!found) return undefined;
            if (!nameOf(found).includes(withoutIsolates(sentence))) {
              throw new Error(`The row matching ${String(row)} does not name the badge's sentence "${sentence}": its name is "${nameOf(found)}"`);
            }
            // A row holds more than one tooltip trigger (the badge, then the activity marker of a
            // collapsed one), so the badge is the one holding its state glyph: a row whose badge is
            // no trigger (a repository with no commit yet, a directory that is not a repository)
            // fails here rather than hover the marker.
            trigger = Array.from(found.querySelectorAll<HTMLElement>(TOOLTIP_TRIGGER)).find((element) => element.querySelector(glyph));
            // Found, so waiting would not change it: the row's badge has no tooltip.
            if (!trigger) throw new Error(`The row matching ${String(row)} has a badge that carries no tooltip to hover`);
          }
          const mark = trigger?.querySelector(glyph);
          if (!mark) return undefined;
          // The branch has its own LTR line; upstream counts stay outside its fade and marquee.
          const part = over === "glyph" ? mark : trigger?.querySelector("[dir=ltr]");
          // Found, so waiting would not change it: the badge has no line, only its glyph.
          if (!part) throw new Error(`The git badge has no ${over} to hover`);
          return part as HTMLElement;
        },
        row === undefined ? `No git badge named "${sentence}" in the sidebar` : `No sidebar row matching ${String(row)} with a git badge to hover`,
      );
      // The tooltip shows the sentence as rich content, its references drawn as chips, so it holds no isolates.
      const spoken = withoutIsolates(sentence);
      const shown = withoutIsolates(doc.querySelector("[role=tooltip]")?.textContent ?? "").trim();
      if (shown !== spoken) {
        throw new Error(`The git badge's tooltip is not its sentence "${spoken}": the last tooltip is ${shown ? `"${shown}"` : "none"}`);
      }
    },
    async terminalContextMenu() {
      const screen = await waitFor(() => {
        const terminal = doc.querySelector(`${AGENT_TERMINAL} .xterm`) as TerminalElement | null;
        if (!terminal?.octoboardTerminalText?.row(0)) return undefined;
        return terminal.querySelector<HTMLElement>(".xterm-screen") ?? undefined;
      }, "No terminal output to right-click");
      const box = screen.getBoundingClientRect();
      screen.dispatchEvent(new MouseEvent("contextmenu", {
        bubbles: true, cancelable: true, button: 2,
        clientX: box.x + Math.min(240, box.width / 2),
        clientY: box.y + Math.min(120, box.height / 2),
      }));
      await wait(300);
    },
    async contextMenu(matcher) {
      const control = await waitFor(() => find(matcher), `Nothing to right-click matching ${String(matcher)}`);
      const box = control.getBoundingClientRect();
      control.dispatchEvent(new MouseEvent("contextmenu", {
        bubbles: true, cancelable: true, button: 2,
        clientX: box.x + box.width / 2, clientY: box.y + box.height / 2,
      }));
      await wait(300);
    },
    async press(matcher) {
      await clickWhenFound(() => find(matcher), `Nothing to press matching ${String(matcher)}`);
    },
    async pressMenuItem(matcher) {
      await clickWhenFound(() => findAll(matcher).find((element) => element.matches('[role="menuitem"]')), `No menu item matching ${String(matcher)}`);
    },
    async touchPress(matcher) {
      await touchAndWait(() => find(matcher), `Nothing to press matching ${String(matcher)}`, `A touch press of ${String(matcher)} opened no popover`);
    },
    async tapTopBarStatus() {
      await touchAndWait(
        () => doc.querySelector<HTMLElement>("[data-region=topbar] [role=img]") ?? undefined,
        "No status glyph in the top bar",
        "A tap on the top bar's status glyph opened no popover",
      );
    },
    async pressWithModifier(matcher) {
      await clickWhenFound(() => find(matcher), `Nothing to press matching ${String(matcher)}`, true);
    },
    async pressInPanel(kind, matcher) {
      await clickWhenFound(() => {
        const card = doc.getElementById(panelId(kind));
        return card ? findAll(matcher, card)[0] : undefined;
      }, `Nothing to press matching ${String(matcher)} in the ${kind} panel`);
    },
    async pressInDiff(matcher) {
      await clickWhenFound(() => {
        for (const host of doc.querySelectorAll("*")) {
          const found = host.shadowRoot && Array.from(host.shadowRoot.querySelectorAll<HTMLElement>("[data-expand-button], [role=button]")).find((element) => matches(matcher, nameOf(element)));
          if (found) return found;
        }
        return undefined;
      }, `Nothing to press matching ${String(matcher)} in the diff`);
    },
    async pressEach(matcher) {
      const controls = await waitFor(() => {
        const found = findAll(matcher);
        return found.length > 0 ? found : undefined;
      }, `Nothing to press matching ${String(matcher)}`);
      for (const control of controls) await clickWhenFound(() => (control.isConnected ? control : undefined), `${String(matcher)} went away before it was pressed`);
    },
    async confirm() {
      await clickWhenFound(
        () =>
          Array.from(
            doc.querySelectorAll<HTMLElement>('[role="dialog"] button[type="submit"], [role="alertdialog"] button[type="submit"]'),
          ).find(isEnabled),
        "No dialog has a button to confirm it with",
      );
    },
  };
}
