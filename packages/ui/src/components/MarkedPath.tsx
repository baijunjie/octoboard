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
 * store. `size` "md" sets it in body text with a 16px icon, to stand beside 14px text. */
export function PlainMarkedPath({
  path,
  shown = path,
  icon: Icon = Folder,
  size = "sm",
}: {
  path: string;
  shown?: string;
  icon?: LucideIcon | React.ReactElement;
  size?: "sm" | "md";
}): React.ReactElement {
  return (
    <span className={size === "md" ? "flex min-w-0 items-center gap-2" : "flex min-w-0 items-center gap-1.5"}>
      {React.isValidElement(Icon) ? Icon : <Icon size={size === "md" ? 16 : 14} aria-hidden className="shrink-0 text-app-muted" />}
      <PathText path={path} shown={shown} className={size === "md" ? "min-w-0 flex-1 font-mono text-sm leading-5" : "min-w-0 flex-1 font-mono text-xs leading-5"} />
    </span>
  );
}

/** The path text alone, with no icon or type style of its own: cut from its start when too long so
 * its own last name stays visible, with the full path as the tooltip while it is clipped or while
 * `shown` words it differently. `className` supplies the sizing, as for `FadeOverflow`. */
export function PathText({ path, shown = path, className }: { path: string; shown?: string; className: string }): React.ReactElement {
  return (
    <FadeOverflow as="span" className={className} dir="ltr" clip="start" titleWhenClipped={path} title={shown === path ? undefined : path}>
      {shown}
    </FadeOverflow>
  );
}

/** A directory path in `PlainMarkedPath`, shown with the daemon host's home directory as `~`. It
 * reads the store for that home directory, which is why it is a component of its own rather than
 * an option on the plain path label. */
export function MarkedPath({ path, size }: { path: string; size?: "sm" | "md" }): React.ReactElement {
  const home = useDaemonStore((s) => s.homeDir);
  return <PlainMarkedPath path={path} shown={abbreviateHome(path, home)} size={size} />;
}
