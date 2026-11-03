import { Tabs } from "@heroui/react";
import React from "react";

import { useTabScrollLabels } from "./tabScrollLabels";

/** HeroUI's `Tabs.ListContainer` with its scroll buttons named in the current language (see `useTabScrollLabels`).
 * `after` is placed behind the container's content, inside its element, for what is laid over the strip. */
export function LocalizedTabListContainer({ after, ...props }: Omit<React.HTMLAttributes<HTMLDivElement>, "children"> & { children?: React.ReactNode; after?: React.ReactNode }): React.ReactElement {
  const tabScrollLabels = useTabScrollLabels();
  return <Tabs.ListContainer {...props} render={(rendered) => <div {...rendered}>{tabScrollLabels(rendered.children)}{after}</div>} />;
}
