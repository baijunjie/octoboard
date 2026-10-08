import { Folder } from "lucide-react";
import React from "react";

import { abbreviateHome } from "../pathDisplay";
import { useDaemonStore } from "../store";
import { FadeOverflow } from "./FadeOverflow";

/** A directory path set apart from the prose around it (a setting's description): a folder icon
 * ahead of it and the path in monospace, cut from its start when too long so the directory's own
 * name stays visible, while the icon stays in place. It is shown with the daemon host's home
 * directory as `~`, and its tooltip is then the full path even while it fits. It reads the store
 * for that home directory, which is why it is a component of its own rather than an option on the
 * plain path label. */
export function MarkedPath({ path }: { path: string }): React.ReactElement {
  const home = useDaemonStore((s) => s.homeDir);
  const shown = abbreviateHome(path, home);
  return (
    <div className="flex min-w-0 items-center gap-1.5">
      <Folder size={14} aria-hidden className="shrink-0 text-muted" />
      <FadeOverflow
        className="min-w-0 flex-1 font-mono text-xs leading-5"
        dir="ltr"
        clip="start"
        titleWhenClipped={path}
        title={shown === path ? undefined : path}
      >
        {shown}
      </FadeOverflow>
    </div>
  );
}
