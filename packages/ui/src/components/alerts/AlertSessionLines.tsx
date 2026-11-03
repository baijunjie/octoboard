import type React from "react";
import { useState } from "react";

import { InlineReference } from "../../i18n/InlineReference";
import { Message, useT } from "../../i18n/react";
import { sessionDisplayTitle } from "../../sessions/sessionLabel";
import { useDaemonStoreOrEmpty } from "../../store";
import { sameValue } from "../../values/sameValue";

interface Lines { name: string; place?: { kind: "project" | "console"; name?: string } }

/** The lines an error alert about a session opens with: which session, and the project or console it is in. The daemon
 * words its message as "this session" without naming it, so these are what tell the user which one. They are named as
 * the store has them while the alert is on screen, so a rename made after the alert was raised shows. A session that
 * leaves the store while the alert is on screen keeps the lines it last had. One the store does not hold when the alert
 * shows is named by `fallbackName`, the name its request carried (a failed launch's notice), with no place; with
 * neither there are no lines. */
export function AlertSessionLines({ session: id, fallbackName }: { session?: string; fallbackName?: string }): React.ReactElement | null {
  const t = useT();
  const session = useDaemonStoreOrEmpty((s) => (id === undefined ? undefined : s.sessions.get(id)));
  const place = useDaemonStoreOrEmpty((s) => {
    const held = id === undefined ? undefined : s.sessions.get(id);
    return held && (held.project_id ? s.projects.get(held.project_id) : s.consoles.get(held.console_id));
  });
  const current: Lines | undefined = session && { name: sessionDisplayTitle(t, session), place: { kind: session.project_id ? "project" : "console", name: place?.name } };
  const [remembered, setRemembered] = useState(current);
  if (current && !sameValue(current, remembered)) setRemembered(current);
  const lines = current ?? remembered ?? (fallbackName ? { name: fallbackName } : undefined);
  if (!lines) return null;
  const placeName = lines.place?.name === undefined ? t("error.alertLocationUnknown") : <InlineReference dir="auto" wrap>{lines.place.name}</InlineReference>;
  return (
    <div className="mb-2 flex flex-col gap-1">
      <p dir="auto"><Message id="error.alertSession" params={{ session: <InlineReference dir="auto" wrap>{lines.name}</InlineReference> }} /></p>
      {lines.place && <p dir="auto">{lines.place.kind === "project"
        ? <Message id="error.alertProject" params={{ project: placeName }} />
        : <Message id="error.alertConsole" params={{ console: placeName }} />}</p>}
    </div>
  );
}
