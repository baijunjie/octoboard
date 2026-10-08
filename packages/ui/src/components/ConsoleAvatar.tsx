import { Network } from "lucide-react";
import React from "react";

import type { Console } from "../protocol";

/** The fills a default avatar picks from (`--avatar-*` in `style.css`), each written out in full
 * so Tailwind emits it. */
const FILLS = [
  "bg-(--avatar-olive)",
  "bg-(--avatar-teal)",
  "bg-(--avatar-azure)",
  "bg-(--avatar-indigo)",
  "bg-(--avatar-plum)",
  "bg-(--avatar-copper)",
  "bg-(--avatar-slate)",
] as const;

/** What a name is read as: composed (NFC), so the same name typed with precomposed or combining
 * characters gets the same fill, and without the spaces and the invisible format characters
 * (`\p{Cf}`: zero-width and bidirectional marks, a byte order mark) around it, which show nothing
 * and so must not change the fill. */
function readName(name: string): string {
  return name.normalize("NFC").replace(/^[\s\p{Cf}]+|[\s\p{Cf}]+$/gu, "");
}

/** The fill for a name: a hash of the whole name as it is read (FNV-1a, then mixed so short names
 * spread over the fills), so the same name always gets the same fill — on the rail and in the
 * dialog's preview while it is typed — and consoles whose names start alike still tend to differ. */
export function avatarFill(name: string): (typeof FILLS)[number] {
  let hash = 0x811c9dc5;
  for (const char of readName(name)) hash = Math.imul(hash ^ char.codePointAt(0)!, 0x01000193);
  hash ^= hash >>> 16;
  hash = Math.imul(hash, 0x85ebca6b);
  hash ^= hash >>> 13;
  hash = Math.imul(hash, 0xc2b2ae35);
  hash ^= hash >>> 16;
  return FILLS[(hash >>> 0) % FILLS.length];
}

/** A console's avatar: its custom image when it has one, otherwise the network glyph on a fill
 * picked from the name, so consoles without an image can still be told apart on the rail. Round either
 * way. Always decorative: the console's name is written out beside it or is its control's
 * accessible name. */
export function ConsoleAvatar({
  name,
  icon,
  className = "size-6",
}: {
  name: string;
  icon: Console["icon"];
  /** Sets the size, and nothing else. */
  className?: string;
}): React.ReactElement {
  if (icon) {
    return <img src={icon} alt="" className={`shrink-0 rounded-full object-cover ${className}`} />;
  }
  return (
    <span
      aria-hidden="true"
      className={`flex shrink-0 items-center justify-center rounded-full text-avatar-foreground ${avatarFill(name)} ${className}`}
    >
      <Network className="size-[50%]" />
    </span>
  );
}
