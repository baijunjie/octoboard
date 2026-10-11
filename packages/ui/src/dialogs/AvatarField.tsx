import { Button } from "@heroui/react";
import { ImageUp, X } from "lucide-react";
import React, { useRef } from "react";

import { handFocusOff } from "../components/handFocusOff";
import { TitledControl } from "../components/TitledControl";
import { useT } from "../i18n/react";
import type { Console } from "../protocol";
import { LabelledGroup } from "./LabelledGroup";

/** The console dialog's avatar field: one round control, the whole field. With no image it is an
 * upload glyph to press, so an optional field shows nothing that looks already filled in; with one
 * it shows the image, and a delete button sits over its corner, revealed under the pointer or
 * keyboard focus. The delete button stays in the DOM while hidden, as `RowControls` does, so a
 * keyboard user reaching it makes it visible, and it is shown outright where the pointer is
 * coarse: a touch has no hover to reveal it with, and tapping the image opens the file picker
 * rather than revealing anything. Deleting puts the control back to the upload glyph, from where
 * another image can be picked. `isPending` is for the time a picked file is being read: the upload
 * button stays focusable and announces it, where disabling it would blur the button just pressed.
 *
 * The delete button is drawn 20px, the size of HeroUI's own clear button, with a 2px margin of hit
 * area around it as that button has, which brings the target to the 24px WCAG 2.2 AA 2.5.8 asks of
 * one that no exception covers. */
export function AvatarField({
  icon,
  onPick,
  onRemove,
  isDisabled,
  isPending,
}: {
  /** The avatar's image, `null` with none picked. */
  icon: Console["icon"];
  onPick: (file: File) => void;
  onRemove: () => void;
  isDisabled?: boolean;
  isPending?: boolean;
}): React.ReactElement {
  const t = useT();
  const fileInput = useRef<HTMLInputElement>(null);
  const uploadButton = useRef<HTMLButtonElement>(null);
  const removeButton = useRef<HTMLButtonElement>(null);
  const choose = icon ? t("dialog.console.avatarReplace") : t("dialog.console.avatarChoose");
  const remove = t("dialog.console.avatarRemove");
  return (
    <LabelledGroup label={t("dialog.console.avatar")} isDisabled={isDisabled}>
      <div className="group relative size-12">
        <TitledControl title={choose}>
          <Button
            ref={uploadButton}
            type="button"
            isIconOnly
            variant={icon ? "ghost" : "outline"}
            aria-label={choose}
            isDisabled={isDisabled}
            isPending={isPending}
            onPress={() => fileInput.current?.click()}
            className="size-12 overflow-hidden rounded-full p-0"
          >
            {icon ? (
              <img src={icon} alt="" className="size-12 rounded-full object-cover" />
            ) : (
              <ImageUp className="size-5" />
            )}
          </Button>
        </TitledControl>
        {icon && (
          <TitledControl title={remove}>
            <Button
              ref={removeButton}
              type="button"
              isIconOnly
              size="sm"
              variant="secondary"
              aria-label={remove}
              isDisabled={isDisabled}
              onPress={() => {
                // This button is gone once the image is, and focus on it would fall to `<body>`,
                // taking the dialog's Escape and Tab along.
                handFocusOff(removeButton.current, uploadButton.current, false);
                onRemove();
              }}
              className="absolute -end-1.5 -top-1.5 size-5 min-w-0 rounded-full p-0 after:absolute after:-inset-0.5 after:content-[''] opacity-0 transition-opacity duration-150 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100 motion-reduce:transition-none"
            >
              <X className="size-3" />
            </Button>
          </TitledControl>
        )}
        <input
          ref={fileInput}
          type="file"
          accept="image/*"
          hidden
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) onPick(file);
            // Cleared so choosing the same file again still counts as a change.
            e.target.value = "";
          }}
        />
      </div>
    </LabelledGroup>
  );
}
