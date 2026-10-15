import type { Translate } from "../i18n/catalog";
import type { StatusItemEntry, StatusItemMenu, StatusItemSection } from "../platform";
import type { Console, Project, Session, SessionStatus } from "../protocol";
import { sessionWithLocation, statusLabel } from "../sessionLabel";
import { sessionsInTreeOrder } from "../sidebar/order";

/** The live statuses the menu lists, each under its own heading, most pressing first. */
const SECTION_ORDER: SessionStatus[] = ["waiting_user", "working", "idle"];

/** How many sessions a section names before the rest are summed up in one line, which keeps the
 * menu on screen however many sessions run. */
export const MAX_PER_SECTION = 10;

/** Glyphs an agent puts at the start of its terminal title to show what it is doing — Claude Code's
 * rotating circle and its idle star, the braille spinners Codex and other terminal tools use, and
 * the star and diamond markers other agent CLIs use. Read into the menu as they are, they would
 * change its text several times a second, and each change replaces the menu, which closes it if it
 * is open. */
const SPINNER_GLYPHS = /^[\u2800-\u28FF\u25D0-\u25D3\u25F0-\u25F7\u2722-\u2727\u2729-\u273D\u00B7\u2022\u25CF\u25CB\u25C9\u25CE\u25C6\u25C7]+(?:\s+|$)/u;

/** A terminal title with the animated glyphs it starts with removed; a glyph only counts when the
 * title goes on after a space or ends there, so a title that merely begins with such a character
 * keeps it. */
export function withoutSpinnerGlyphs(title: string): string {
  return title.replace(SPINNER_GLYPHS, "");
}

export interface StatusItemMenuSource {
  consoles: Console[];
  projects: Project[];
  sessions: Session[];
  consoleMap: Map<string, Console>;
  projectMap: Map<string, Project>;
}

function steadyTitle(session: Session): Session {
  if (!session.terminal_title) return session;
  return { ...session, terminal_title: withoutSpinnerGlyphs(session.terminal_title) };
}

/** The live sessions — waiting for the user, working, awaiting instructions — in the sidebar's
 * order, each status under its own heading. */
export function buildStatusItemMenu(t: Translate, source: StatusItemMenuSource): StatusItemMenu {
  const { consoles, projects, sessions, consoleMap, projectMap } = source;
  const sections = SECTION_ORDER.flatMap((status): StatusItemSection[] => {
    const listed = sessionsInTreeOrder(consoles, projects, sessions, (s) => s.status === status);
    if (listed.length === 0) return [];
    const items: StatusItemEntry[] = listed
      .slice(0, MAX_PER_SECTION)
      .map((session) => ({ label: sessionWithLocation(t, steadyTitle(session), consoleMap, projectMap), session: session.id }));
    if (listed.length > MAX_PER_SECTION) items.push({ label: t("statusItem.more", { count: listed.length - MAX_PER_SECTION }) });
    return [{ heading: t("statusItem.section", { status: statusLabel(t, status), count: listed.length }), items }];
  });
  return {
    sections: sections.length > 0 ? sections : [{ heading: t("statusItem.noSessions"), items: [] }],
    open: t("statusItem.open"),
    quit: t("menu.quit"),
  };
}
