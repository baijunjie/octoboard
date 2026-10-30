import { FolderX, Play, PlayOff } from "lucide-react";
import React from "react";

import { useCurrentLanguage, useT } from "../i18n/react";
import type { ProcessTree } from "../protocol";
import { processFacts } from "../processFacts";
import { useNow } from "../useNow";
import { displayWirePath } from "../wirePath";
import { FadeOverflow } from "./FadeOverflow";
import { MarkedPath } from "./MarkedPath";
import { StatusBadge } from "./StatusBadge";

/** The glyph before the command of a tree that is running. */
function PlayGlyph(): React.ReactElement {
  return <Play aria-hidden="true" className="mt-0.75 size-3.5 shrink-0 text-success-soft-foreground" />;
}

/** The glyph that replaces it where the tree is to be stopped. */
export function StopGlyph(): React.ReactElement {
  return <PlayOff aria-hidden="true" className="mt-0.75 size-3.5 shrink-0 text-danger-soft-foreground" />;
}

/** A process tree's command in monospace, after a glyph: on one line that fades where it is cut, with
 * the whole command as its tooltip then, or with `wrap` the whole command, wrapped. A command the
 * daemon could not read is said so. `leading` replaces the static glyph. */
export function ProcessCommand({ tree, wrap = false, leading }: { tree: ProcessTree; wrap?: boolean; leading?: React.ReactNode }): React.ReactElement {
  const t = useT();
  const known = tree.command !== "";
  const text = known ? tree.command : t("processes.commandUnknown");
  return (
    <span className="flex min-h-5 min-w-0 items-start gap-1.5 text-xs leading-5">
      {leading ?? <PlayGlyph />}
      <span className="block min-w-0 flex-1">
        {wrap
          ? <span dir={known ? "ltr" : undefined} className={known ? "block font-mono [overflow-wrap:anywhere]" : "block"}>{text}</span>
          : <FadeOverflow as="span" dir={known ? "ltr" : undefined} className={known ? "block font-mono" : "block"} titleWhenClipped={text}>{text}</FadeOverflow>}
      </span>
    </span>
  );
}

/** Where a process tree works and what it is doing: its working directory, marked when it was
 * deleted, and a line of facts (memory, ports, age, how many processes). Only the path's text starts
 * at the text column of a `ProcessCommand` above it, after its 14px glyph; optional actions end the facts line. */
export function ProcessTreeDetails({ tree, actions }: { tree: ProcessTree; actions?: React.ReactNode }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  const cwd = tree.cwd === null ? null : displayWirePath(tree.cwd);
  const facts = processFacts(t, language, tree, now);
  return (
    <span className="flex min-w-0 flex-col gap-1 text-xs leading-5 text-app-muted">
      <span className="flex min-w-0 items-center gap-1.5">
        {cwd === null
          ? <><FolderX aria-hidden="true" className="size-3.5 shrink-0" />{t("processes.cwdUnknown")}</>
          : <MarkedPath path={cwd} />}
        {tree.cwd_missing && <StatusBadge variant="soft" color="warning">{t("processes.cwdDeleted")}</StatusBadge>}
      </span>
      {/* Read as one description, the path and the facts would run together; a comma, as `StatusBadge`'s, parts them. */}
      <span className="sr-only">, </span>
      <span className={`flex min-w-0 items-end ${actions ? "min-h-6" : ""}`}>
        <FadeOverflow as="span" className="block min-w-0 flex-1" titleWhenClipped={facts}>{facts}</FadeOverflow>
        {actions}
      </span>
    </span>
  );
}
