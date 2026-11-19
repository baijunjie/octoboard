import React from "react";

import { useT } from "../../i18n/react";
import { useFocusVisibleProps } from "../../primitives";
import type { Session } from "../../protocol";
import { sessionDisplayTitle } from "../../sessions/sessionLabel";
import { FadeOverflow } from "./FadeOverflow";

/** A session's title as a link inside a sentence, which `onOpen` answers by showing that session (the history's owner
 * link, a scheduled task's run). */
export function InlineSessionLink({ session, onOpen }: { session: Session; onOpen: (session: Session) => void }): React.ReactElement {
  const t = useT();
  const focus = useFocusVisibleProps();
  const title = sessionDisplayTitle(t, session);
  return (
    // Not HeroUI's Button: an inline link inside a sentence, whose text flows with the line and fades in its own
    // label; the Button imposes its own height, padding, press-shrink and hover fill.
    <button
      type="button"
      {...focus}
      onClick={() => onOpen(session)}
      className="min-w-0 cursor-pointer rounded-sm text-(--tint-accent-fg) underline underline-offset-2 outline-none ui-keyboard-focus:ring-2 ui-keyboard-focus:ring-focus"
    >
      <FadeOverflow as="span" className="block min-w-0" dir="auto" titleWhenClipped={title}>
        {title}
      </FadeOverflow>
    </button>
  );
}
