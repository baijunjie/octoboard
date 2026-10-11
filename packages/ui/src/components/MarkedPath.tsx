import { Folder, type LucideIcon } from "lucide-react";
import React from "react";

import { abbreviateHome } from "../pathDisplay";
import { useDaemonStore } from "../store";
import { FadeOverflow } from "./FadeOverflow";

/** A path set apart from the prose around it (a setting's description, a file viewer's header): an
 * icon ahead of it (a folder, unless `icon` says what the path is) and the path in monospace, cut
 * from its start when too long so its own last name stays visible, while the icon stays in place.
 * `shown` is the wording when it differs from the path (see `MarkedPath`); the tooltip is then the
 * full path even while it fits. It is a `span` laid out as a block-level flex box, as it was when
 * it was a `div` (the Settings rows depend on its full width and height), so it belongs in a flex
 * container or a block, whose flex item or line it becomes. It takes the wording to show as a prop
 * and so needs no daemon; `MarkedPath` is the variant that reads the home directory from the
 * store. */
export function PlainMarkedPath({
  path,
  shown = path,
  icon: Icon = Folder,
}: {
  path: string;
  shown?: string;
  icon?: LucideIcon;
}): React.ReactElement {
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      <Icon size={14} aria-hidden className="shrink-0 text-muted" />
      <FadeOverflow
        as="span"
        className="min-w-0 flex-1 font-mono text-xs leading-5"
        dir="ltr"
        clip="start"
        titleWhenClipped={path}
        title={shown === path ? undefined : path}
      >
        {shown}
      </FadeOverflow>
    </span>
  );
}

/** A directory path in `PlainMarkedPath`, shown with the daemon host's home directory as `~`. It
 * reads the store for that home directory, which is why it is a component of its own rather than
 * an option on the plain path label. */
export function MarkedPath({ path }: { path: string }): React.ReactElement {
  const home = useDaemonStore((s) => s.homeDir);
  return <PlainMarkedPath path={path} shown={abbreviateHome(path, home)} />;
}
