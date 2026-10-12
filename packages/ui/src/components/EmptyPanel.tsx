import { Button, EmptyState } from "@heroui/react";
import type { LucideIcon } from "lucide-react";
import React, { useId } from "react";

import { TitledControl } from "./TitledControl";

/** HeroUI's `EmptyState` with the shape every empty list in the app takes: a glyph, a line
 * saying what is missing, an optional line explaining it and the action that fills it when there is
 * one. `compact` is a row, for a list nested inside another (a project with no sessions); with an
 * action the whole row is the button that runs it. A message too long for the row wraps rather than
 * being cut, since some (the install prompt) are instructions to act on. */
export function EmptyPanel({
  icon: Icon,
  message,
  description,
  action,
  compact,
}: {
  icon: LucideIcon;
  message: string;
  /** The full panel's line under `message`, which then makes `message` a heading; a compact row has no room for one. */
  description?: string;
  /** `icon` shows only on the full panel's button; a compact row already leads with its glyph. */
  action?: { label: string; icon?: LucideIcon; onPress: () => void };
  compact?: boolean;
}): React.ReactElement {
  const messageId = useId();
  const actionId = useId();
  if (compact) {
    // A `span` rather than a `p`: inside a button only phrasing content is valid.
    const row = (
      <>
        <Icon aria-hidden="true" className="size-4 shrink-0" />
        <span id={messageId} className="min-w-0 flex-1 break-words">
          {message}
        </span>
      </>
    );
    if (!action) return <EmptyState className="flex items-center gap-2 py-1 ps-2 pe-1">{row}</EmptyState>;
    // Named by its visible message and then by what a press does: the message has to be inside the
    // name verbatim (WCAG 2.2 A 2.5.3), and the action says what the row is, which the message
    // alone does not. Two referenced elements rather than one joined string, so neither half is a
    // fragment a catalog has to join. The classes keep the plain row's look and the session rows'
    // hover tint, where HeroUI's button would recolour the text, pull its icon in by 2px
    // (`margin-inline: -2px` on a child `svg`) and keep the label on one line. `p-0` drops
    // `EmptyState`'s own padding — the row carries the padding of the rows it stands among, and a
    // wrapper's on top of it would inset the row from its list, and its glyph from theirs. The
    // plain row above overrides that padding outright.
    return (
      <EmptyState className="flex p-0">
        <TitledControl title={action.label}>
          <Button
            variant="ghost"
            aria-labelledby={`${messageId} ${actionId}`}
            // The tooltip says the action and nothing more, which the name already ends with, so it
            // is not also this row's description — `TitledControl` drops a tooltip that repeats an
            // `aria-label` by itself, but cannot see into a name assembled from other elements.
            aria-describedby=""
            preventFocusOnPress
            onPress={action.onPress}
            className="h-auto min-h-0 w-full justify-start gap-2 rounded-lg py-1 ps-2 pe-1 text-start font-normal text-[inherit] whitespace-normal hover:bg-panel-hover [&_svg]:m-0"
          >
            {row}
            <span id={actionId} className="sr-only">
              {action.label}
            </span>
          </Button>
        </TitledControl>
      </EmptyState>
    );
  }
  const ActionIcon = action?.icon;
  return (
    <EmptyState className="flex flex-col items-center gap-3 px-4 py-10 text-center">
      <span className="flex size-12 items-center justify-center rounded-full bg-default text-muted">
        <Icon aria-hidden="true" className="size-6" />
      </span>
      {description ? <h2 className="text-sm font-semibold">{message}</h2> : <p>{message}</p>}
      {description && <p className="max-w-sm text-sm text-muted">{description}</p>}
      {action && (
        // A press never takes focus: it would drop to `<body>` when the panel goes away, and
        // whatever the action opens or starts places focus itself.
        <Button size="sm" variant="secondary" preventFocusOnPress onPress={action.onPress}>
          {ActionIcon && <ActionIcon aria-hidden="true" className="size-4" />}
          {action.label}
        </Button>
      )}
    </EmptyState>
  );
}
