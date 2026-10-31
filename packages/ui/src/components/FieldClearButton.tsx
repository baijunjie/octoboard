import { X } from "lucide-react";
import React from "react";

import { Button, type ButtonProps } from "./Button";
import { TitledControl } from "./TitledControl";
import { useT } from "../i18n/react";

/** The shared compact ghost action for clearing a field. */
export function FieldClearButton(props: ButtonProps): React.ReactElement {
  const t = useT();
  return (
    <TitledControl title={t("common.clear")}>
      <Button {...props} data-field-clear="" isIconOnly size="xs" variant="ghost" aria-label={t("common.clear")}>
        <X aria-hidden="true" />
      </Button>
    </TitledControl>
  );
}
