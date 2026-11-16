import React, { useState } from "react";
import { Network, Pencil } from "lucide-react";

import { CopyCommand } from "../components/controls/CopyBox";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { MenuItems } from "../components/menus/MenuItems";
import type { ActionMenuEntry } from "../components/menus/menuEntries";
import { useT } from "../i18n/react";
import { isolate } from "../i18n/isolate";
import { Menu, MenuPopover, MenuTrigger } from "../primitives";

/** The forward command's placeholder for the remote host, which the owner fills in; the menu's last item puts it back. */
const SERVER_PLACEHOLDER = "<server>";

/** How the address in the command looks: a token in the monospace text that reads as pressable (dotted underline, a fill
 * on hover and while its menu is open, the application's focus ring on keyboard focus), kept to the text's own size. It
 * takes padding at its end only, so the fill and the ring never lie over the text before it. Its pointer target is
 * stretched to 24px tall by an `::after`, which draws nothing. */
const TOKEN_CLASS =
  "relative inline cursor-pointer rounded-sm bg-default/60 pe-1 font-[inherit] text-[length:inherit] underline decoration-dotted underline-offset-4 outline-none hover:bg-default aria-expanded:bg-default ui-keyboard-focus:ring-2 ui-keyboard-focus:ring-focus after:absolute after:inset-x-0 after:-inset-y-0.5 after:content-['']";

/** The SSH forward command to copy, with the address another computer reaches this one by as a token in it that opens a
 * menu of `addresses` (this computer's name and addresses as the daemon lists them, the first chosen to begin with, so
 * the command is ready to copy) and a last item, "Enter the address yourself", which puts `<server>` back for the owner
 * to fill in. The command's text, which the copy button copies, carries what the token shows. With no `addresses` it is
 * the command as the daemon gave it, plain. A pick that the daemon stops listing falls back to the first address. */
export function ForwardCommand({ command, addresses, copyText, failedText }: {
  command: string;
  addresses: readonly string[];
  copyText: string;
  failedText: string;
}): React.ReactElement {
  const t = useT();
  const [picked, setPicked] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const at = command.indexOf(SERVER_PLACEHOLDER);
  if (addresses.length === 0 || at < 0) {
    return <CopyCommand text={command} copyText={copyText} failedText={failedText} />;
  }
  const address = picked !== null && (picked === SERVER_PLACEHOLDER || addresses.includes(picked)) ? picked : addresses[0];
  const entries: ActionMenuEntry[] = [
    ...addresses.map((value) => ({ label: value, content: <FadeOverflow as="span" className="block font-mono text-xs" dir="ltr" titleWhenClipped={value}>{value}</FadeOverflow>, icon: Network, selected: value === address, onClick: () => setPicked(value) })),
    { label: t("settings.remoteAccess.forward.address.manual"), icon: Pencil, selected: address === SERVER_PLACEHOLDER, onClick: () => setPicked(SERVER_PLACEHOLDER) },
  ];
  const before = command.slice(0, at);
  const after = command.slice(at + SERVER_PLACEHOLDER.length);
  return (
    <CopyCommand
      text={before + address + after}
      content={
        <>
          {before}
          <Menu open={open} onOpenChange={setOpen}>
            <MenuTrigger label={t("settings.remoteAccess.forward.address.change", { address: isolate(address) })} className={TOKEN_CLASS}>
              {address}
            </MenuTrigger>
            <MenuPopover className="min-w-48 max-w-72">
              <MenuItems entries={entries} />
            </MenuPopover>
          </Menu>
          {after}
        </>
      }
      copyText={copyText}
      failedText={failedText}
    />
  );
}
