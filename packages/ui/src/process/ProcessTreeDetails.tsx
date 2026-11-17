import { FolderX, Globe, Play, PlayOff } from "lucide-react";
import React from "react";

import { useCurrentLanguage, useT } from "../i18n/react";
import type { ProcessTree } from "../protocol";
import { processFacts } from "./processFacts";
import { useNow } from "../format/useNow";
import { displayWirePath } from "../format/wirePath";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { MarkedPath } from "../components/text/MarkedPath";
import { StatusBadge } from "../components/status/StatusBadge";
import { ExternalLink } from "../components/text/ExternalLink";
import { useListMotion } from "../components/motion/useListMotion";

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

/** The addresses a browser can open for the ports of a tree that serve web pages, after a globe glyph. They name
 * `localhost` rather than an address family, so the server is reached whichever one it listens on. The list runs in
 * the interface's direction from the glyph, each address reading left to right; ports that come and go move the others
 * with list motion. */
function WebLinks({ ports }: { ports: number[] }): React.ReactElement {
  const listRef = useListMotion<HTMLSpanElement>();
  return (
    <span className="flex min-w-0 items-start gap-1.5">
      <Globe aria-hidden="true" className="mt-0.75 size-3.5 shrink-0" />
      {/* The row gap keeps wrapped links' pointer targets 24px apart. */}
      <span ref={listRef} className="flex min-w-0 flex-wrap gap-x-3 gap-y-1">
        {ports.map((port) => (
          <span key={port} data-list-motion={String(port)} className="flex min-w-0">
            <ExternalLink href={`http://localhost:${port}/`} dir="ltr" className="font-mono">{`localhost:${port}`}</ExternalLink>
          </span>
        ))}
      </span>
    </span>
  );
}

/** Where a process tree works and what it is doing: its working directory, marked when it was
 * deleted, with `links` the addresses of its ports that serve web pages, and a line of facts (memory, ports, age, how
 * many processes). Only the path's text starts at the text column of a `ProcessCommand` above it, after its 14px
 * glyph; optional actions end the facts line. */
export function ProcessTreeDetails({ tree, actions, links = false }: { tree: ProcessTree; actions?: React.ReactNode; links?: boolean }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  const cwd = tree.cwd === null ? null : displayWirePath(tree.cwd);
  const facts = processFacts(t, language, tree, now);
  const webPorts = links ? tree.http_ports ?? [] : [];
  return (
    <span className="flex min-w-0 flex-col gap-1 text-xs leading-5 text-app-muted">
      <span className="flex min-w-0 items-center gap-1.5">
        {cwd === null
          ? <><FolderX aria-hidden="true" className="size-3.5 shrink-0" />{t("processes.cwdUnknown")}</>
          : <MarkedPath path={cwd} />}
        {tree.cwd_missing && <StatusBadge variant="soft" color="warning">{t("processes.cwdDeleted")}</StatusBadge>}
      </span>
      {/* Read as one description, the path, the links and the facts would run together; a comma, as `StatusBadge`'s,
          parts them. */}
      <span className="sr-only">, </span>
      {webPorts.length > 0 && <><WebLinks ports={webPorts} /><span className="sr-only">, </span></>}
      <span className={`flex min-w-0 items-end ${actions ? "min-h-6" : ""}`}>
        <FadeOverflow as="span" className="block min-w-0 flex-1" titleWhenClipped={facts}>{facts}</FadeOverflow>
        {actions}
      </span>
    </span>
  );
}
