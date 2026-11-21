import { X } from "lucide-react";
import React from "react";

import { Button, type ButtonProps } from "../../primitives";
import { useT } from "../../i18n/react";

/** The shared compact ghost action for clearing a search field or an optional select, transparent at rest; its hover
 * and press fills are blended over the field it sits on (`[data-field-clear]` in `primitives/Field.css`). A select's
 * clear button stands beside its trigger, never inside that button, and gives focus back to the trigger after
 * clearing. */
export function FieldClearButton(props: ButtonProps): React.ReactElement {
  const t = useT();
  return (
    <Button tooltip={t("common.clear")} {...props} data-field-clear="" iconOnly size="xs" variant="ghost" label={t("common.clear")}>
      <X />
    </Button>
  );
}
