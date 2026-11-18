import { X } from "lucide-react";
import React from "react";

import { Button, type ButtonProps } from "../../primitives";
import { useT } from "../../i18n/react";

/** The shared compact ghost action for clearing a field. */
export function FieldClearButton(props: ButtonProps): React.ReactElement {
  const t = useT();
  return (
    <Button tooltip={t("common.clear")} {...props} data-field-clear="" iconOnly size="xs" variant="ghost" label={t("common.clear")}>
      <X />
    </Button>
  );
}
