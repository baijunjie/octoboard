import { Copy } from "lucide-react";
import React from "react";

import { ActionMenu } from "../components/ActionMenu";
import { RowControls } from "../components/RowControls";
import { useCopyPath } from "../components/useCopyPath";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";

/** The one button at the end of a row of the Files tree or of the Git mode's changes, opening the
 * menu of what can be done with that row's file or directory; Copy path is the only action so far. The button's
 * name says whose actions it opens, since the row does not add its own to it; its tooltip stays the
 * short "More actions". It is shown by the row's hover, focus and selection through `RowControls`. */
export function RowActions({ path, name, directory = false }: { path: string; name: string; directory?: boolean }): React.ReactElement | null {
  const t = useT();
  const copyPath = useCopyPath();
  // No button where nothing can be done: a menu whose only item cannot act is worse than none.
  if (!copyPath) return null;
  return (
    <RowControls className="ms-auto">
      <ActionMenu
        label={t(directory ? "browser.directory.actions" : "browser.row.actions", { name: isolate(name) })}
        items={[
          {
            label: t("common.copyPath"),
            icon: Copy,
            onClick: () => copyPath(path),
          },
        ]}
      />
    </RowControls>
  );
}
