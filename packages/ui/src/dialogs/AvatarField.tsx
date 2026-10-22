import { ImageUp, X } from "lucide-react";
import React, { useRef } from "react";

import { Button } from "../components/Button";
import { handFocusOff } from "../components/handFocusOff";
import { TitledControl } from "../components/TitledControl";
import { useT } from "../i18n/react";
import type { Console } from "../protocol";

/** The console dialog's avatar field: one round control, the whole field. With no image it is an
 * upload glyph to press, on the same fill as the dialog's text fields, so an optional field shows
 * nothing that looks already filled in; with one it shows the image, and a delete button sits over
 * its corner, revealed under the pointer or keyboard focus. The delete button stays in the DOM
 * while hidden, as `RowControls` does, so a keyboard user reaching it makes it visible, and it is
 * shown outright where the pointer is coarse: a touch has no hover to reveal it with, and tapping
 * the image opens the file picker rather than revealing anything. Deleting puts the control back to the upload glyph, from where
 * another image can be picked. `isPending` is for the time a picked file is being read: the upload
 * button stays focusable and announces it, where disabling it would blur the button just pressed.
 *
 * The field draws no label of its own: it sits beside the console's name, which names what it is
 * the avatar of, and the group carries "Avatar" as its accessible name instead. */
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
    <div role="group" aria-label={t("dialog.console.avatar")} className="group relative size-12">
      <TitledControl title={choose}>
        <Button
          ref={uploadButton}
          type="button"
          isIconOnly
          variant={icon ? "ghost" : "tertiary"}
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
            size="xs"
            variant="secondary"
            aria-label={remove}
            isDisabled={isDisabled}
            onPress={() => {
              // This button is gone once the image is, and focus on it would fall to `<body>`,
              // taking the dialog's Escape and Tab along.
              handFocusOff(removeButton.current, uploadButton.current, false);
              onRemove();
            }}
            className="absolute -end-1.5 -top-1.5 rounded-full opacity-0 transition-opacity duration-150 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100 motion-reduce:transition-none"
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
  );
}
