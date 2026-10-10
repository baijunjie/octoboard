import { Copy } from "lucide-react";
import React from "react";

import { ActionMenu } from "../components/ActionMenu";
import { RowControls } from "../components/RowControls";
import { useT } from "../i18n/react";
import { displayWirePath } from "../wirePath";
import type { ChangeItem } from "./changes";
import { useCopyPath } from "./useCopyPath";

/** The one button at a change row's end, opening the menu of what can be done with its change; Copy
 * path is the only action so far. The button's name says whose actions it opens, since the row does
 * not add its own to it; its tooltip stays the short "More actions". It is shown by the row's hover,
 * focus and selection through `RowControls`. */
export function ChangeRowActions({ item, name }: { item: ChangeItem; name: string }): React.ReactElement | null {
  const t = useT();
  const copyPath = useCopyPath();
  // No button where nothing can be done: a menu whose only item cannot act is worse than none.
  if (!copyPath) return null;
  return (
    <RowControls className="ms-auto">
      <ActionMenu
        label={t("git.row.actions", { name })}
        items={[
          {
            label: t("git.row.copyPath"),
            icon: Copy,
            // The path as the row's own text shows it: for a human to paste, so a byte that is not
            // UTF-8 comes out as U+FFFD rather than as the wire form's `%XX`.
            onClick: () => copyPath(displayWirePath(item.path)),
          },
        ]}
      />
    </RowControls>
  );
}
