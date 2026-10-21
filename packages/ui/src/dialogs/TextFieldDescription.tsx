import { Description } from "@heroui/react";

import { Message, useT } from "../i18n/react";
import { textLength } from "./textLength";

export function TextLengthHint({ value, limit }: { value: string; limit: number }): React.ReactElement | null {
  const t = useT();
  const count = textLength(value);
  return count >= limit * 0.9 ? <span className="block text-app-muted tabular-nums"><Message id="dialog.characterCount" params={{ ratio: <bdi dir="ltr">{t("dialog.characterRatio", { count, limit })}</bdi> }} /></span> : null;
}

export function TextFieldDescription({ value, characterLimit, description }: { value: string; characterLimit?: number; description?: React.ReactNode }): React.ReactElement | null {
  const count = textLength(value);
  const showCount = characterLimit !== undefined && count >= characterLimit * 0.9;
  if (!description && !showCount) return null;
  return (
    <Description>
      {description}
      {showCount && <TextLengthHint value={value} limit={characterLimit} />}
    </Description>
  );
}
