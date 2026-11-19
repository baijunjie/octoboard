import { FileSearch, FileX } from "lucide-react";
import type React from "react";

import { IconToggle } from "../components/controls/IconToggle";
import { useT } from "../i18n/react";

/** The toggle beside a field that includes the files Git ignores, or leaves them out: named and drawn for what pressing
 * it does next, so its name and icon swap with its state. Shared by the file filter and Find in files. */
export function IncludeIgnoredToggle({ included, onChange }: { included: boolean; onChange: (included: boolean) => void }): React.ReactElement {
  const t = useT();
  return (
    <IconToggle besideField label={t(included ? "browser.filter.excludeIgnored" : "browser.filter.includeIgnored")} isSelected={included} onChange={onChange}>
      {included ? <FileSearch className="size-4" /> : <FileX className="size-4" />}
    </IconToggle>
  );
}
