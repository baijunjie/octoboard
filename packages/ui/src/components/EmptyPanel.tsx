import { Button, EmptyState } from "@heroui/react";
import type { LucideIcon } from "lucide-react";
import React from "react";

import { TitledControl } from "./TitledControl";

/** HeroUI's `EmptyState` with the shape every empty list in the app takes: a glyph, a line
 * saying what is missing, and the action that fills it when there is one. `compact` is a row with
 * the action as an icon button, for a list nested inside another (a project with no sessions); a
 * message too long for the row wraps rather than being cut, since some (the install prompt) are
 * instructions to act on. */
export function EmptyPanel({
  icon: Icon,
  message,
  action,
  compact,
}: {
  icon: LucideIcon;
  message: string;
  action?: { label: string; icon?: LucideIcon; onPress: () => void };
  compact?: boolean;
}): React.ReactElement {
  const ActionIcon = action?.icon;
  if (compact) {
    return (
      <EmptyState className="flex items-center gap-2 py-1 ps-2 pe-1">
        <Icon aria-hidden="true" className="size-4 shrink-0" />
        <p className="min-w-0 flex-1 break-words">{message}</p>
        {action && (
          <TitledControl title={action.label}>
            <Button
              isIconOnly
              size="sm"
              variant="ghost"
              aria-label={action.label}
              preventFocusOnPress
              onPress={action.onPress}
              className="size-7 min-w-0 shrink-0"
            >
              {ActionIcon && <ActionIcon aria-hidden="true" className="size-4" />}
            </Button>
          </TitledControl>
        )}
      </EmptyState>
    );
  }
  return (
    <EmptyState className="flex flex-col items-center gap-3 px-4 py-10 text-center">
      <span className="flex size-12 items-center justify-center rounded-full bg-default text-muted">
        <Icon aria-hidden="true" className="size-6" />
      </span>
      <p>{message}</p>
      {action && (
        // The sidebar's controls never take focus on a mouse press (see `ActionMenu`); the dialog
        // the action opens takes it instead.
        <Button size="sm" variant="secondary" preventFocusOnPress onPress={action.onPress}>
          {ActionIcon && <ActionIcon aria-hidden="true" className="size-4" />}
          {action.label}
        </Button>
      )}
    </EmptyState>
  );
}
