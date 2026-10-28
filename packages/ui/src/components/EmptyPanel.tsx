import { EmptyState } from "@heroui/react";
import type { LucideIcon } from "lucide-react";
import React, { useId } from "react";

import { Button } from "./Button";
import { DetailsButton } from "./DetailsButton";
import { MarkLineBox } from "./MarkLineBox";
import { TitledControl } from "./TitledControl";
import { useAnnouncement } from "./useAnnouncement";

/** `ErrorAlert`'s announcement of a failure panel. */
interface Announcement {
  announce?: boolean;
  announceKey?: number;
  announceDelayMs?: number;
}

/** HeroUI's `EmptyState` with the shape every empty list in the app takes: a glyph, a heading
 * saying what is missing, an optional line explaining it and the action that fills it when there is
 * one. `compact` is a row, for a list nested inside another (a project with no sessions); with an
 * action the whole row is the button that runs it. A message too long for the row wraps rather than
 * being cut.
 *
 * `silent` is for a state another region already announces: it hides this copy from assistive
 * technology so the state is not read twice. It cannot be combined with an action, the primary one
 * or the secondary: a button inside an `aria-hidden` subtree stays reachable by Tab yet has no name
 * or role. A plain compact row accepts it too.
 *
 * `tone="danger"` is the shape of a failure that is the whole state of what the panel fills (a pane
 * whose subject could not be loaded): a danger glyph, the heading saying what could not be loaded,
 * and in the centred row the recovery `action` and Details, the button that opens `details` in a
 * popover (further explanation in the application's own words, then raw text). `remedy` is the one
 * main thing to do, a second line under the first. A startup screen, whose row is kept for its recovery and
 * Quit, passes `headingMark` instead of `details`: the mark follows the heading and the row holds the actions
 * alone. `announce`, `announceKey` and `announceDelayMs` are
 * `ErrorAlert`'s: the heading and the first line, and nothing else, are spoken as an alert. */
export function EmptyPanel({
  icon: Icon,
  message,
  headingLevel = 2,
  description,
  remedy,
  action,
  secondaryAction,
  details,
  headingMark,
  compact,
  silent,
  tone,
  actionRef,
  announce = false,
  announceKey,
  announceDelayMs,
}: {
  icon: LucideIcon;
  message: React.ReactNode;
  /** The full panel's heading level: the level of the heading naming whatever the panel stands in
   * for, so the two read as one thing in the view's outline rather than the panel opening a section
   * of its own. The default suits a pane's own title and a dialog's, both `h2`; the sidebar's
   * `SectionHeading` is an `h3`, so a panel inside one of those sections passes 3, and a panel that
   * is the whole page (a startup failure) passes 1. A compact row has no heading. */
  headingLevel?: 1 | 2 | 3;
  /** The full panel's line under the heading, explaining what the heading leaves out; a compact
   * row has no room for one. It sits in a `p`, so it takes phrasing content only. */
  description?: React.ReactNode;
  /** Under the description, outside the announcement: the one main thing to do. */
  remedy?: React.ReactNode;
  compact?: boolean;
  /** The full panel's tone: `danger` for a failure. */
  tone?: "danger";
} & (
  | ({
    /** `icon` shows only on the full panel's button; a compact row already leads with its glyph.
     * `isPending` keeps the button where it is, unavailable, while what it started runs; `landing`
     * marks it as the F6 landing of its region. */
    action?: {
      label: string;
      icon?: LucideIcon;
      variant?: "primary" | "secondary";
      isPending?: boolean;
      landing?: boolean;
      onPress: () => void;
    };
    /** The action's button, for an owner that puts focus on it itself, as the press never does. Beside `action`
     * rather than in it, as a ref held in a prop object makes every read of that object a read of a ref. */
    actionRef?: React.Ref<HTMLButtonElement>;
    silent?: never;
  } & (
    // A row holds at most two buttons: the action and one more, a second way out or a failure's Details. A heading
    // mark takes the place of Details, and with it the panel cannot announce, since the mark would sit
    // inside the announced element.
    | (Announcement & { headingMark?: never } & (
      | {
        /** A second way out beside the action, quieter than it, for a full panel only. */
        secondaryAction?: { label: string; icon?: LucideIcon; onPress: () => void };
        details?: never;
      }
      | {
        secondaryAction?: never;
        /** What a failure's Details button opens (further explanation, then raw text), in the centred
         * row after the action; with none there is no Details button. The full panel only, and like the actions never with `silent`. */
        details?: React.ReactNode;
      }
    ))
    | {
      secondaryAction?: { label: string; icon?: LucideIcon; onPress: () => void };
      details?: never;
      /** A mark (`StatusMark`) right after the heading, on its line, holding a startup screen's raw error
       * text, whose row is kept for its recovery and Quit. The full panel only, never with `silent`,
       * `announce` or `details`. */
      headingMark?: React.ReactNode;
      announce?: false;
      announceKey?: never;
      announceDelayMs?: never;
    }
  ))
  | (Announcement & { action?: never; actionRef?: never; secondaryAction?: never; details?: never; headingMark?: never; silent?: true })
)): React.ReactElement {
  const messageId = useId();
  const actionId = useId();
  const announcement = useAnnouncement(announce, announceKey, announceDelayMs);
  if (compact) {
    // A `span` rather than a `p`: inside a button only phrasing content is valid.
    const row = (
      <>
        <Icon aria-hidden="true" className="size-4 shrink-0 text-app-muted" />
        <span id={messageId} className="min-w-0 flex-1 break-words text-app-muted">
          {message}
        </span>
      </>
    );
    // `min-h-8` on both forms is the sidebar rows' height (`TreeRow`): a row standing in for them
    // takes up the line one of them would, so the list does not jump when its first row arrives.
    if (!action) return <EmptyState aria-hidden={silent} className="flex min-h-8 items-center gap-2 py-1 ps-2 pe-1">{row}</EmptyState>;
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
            ref={actionRef}
            preventFocusOnPress
            onPress={action.onPress}
            className="h-auto min-h-8 w-full justify-start gap-2 rounded-lg py-1 ps-2 pe-1 text-start font-normal text-[inherit] whitespace-normal hover:bg-panel-hover [&_svg]:m-0"
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
  const Heading = `h${headingLevel}` as const;
  const SecondaryIcon = secondaryAction?.icon;
  return (
    <EmptyState aria-hidden={silent} className="flex flex-col items-center gap-3 px-4 py-10 text-center">
      <span className={`flex size-12 items-center justify-center rounded-full ${tone === "danger" ? "bg-danger-soft text-danger-soft-foreground" : "bg-default text-app-muted"}`}>
        <Icon aria-hidden="true" className="size-6" />
      </span>
      {/* The heading carries its own size and colour: HeroUI's `.empty-state` base sets `text-sm
          text-muted` on the whole panel, so a heading left to inherit would come out at the
          explaining line's size and colour both, leaving the weight as the only thing telling the
          two apart — too little to read as a heading. */}
      <div key={announcement.key} role={announcement.role} className="flex min-w-0 max-w-full flex-col items-center gap-3">
        {headingMark ? (
          <div className="flex min-w-0 max-w-full items-start justify-center gap-1.5">
            <Heading className="min-w-0 text-base font-semibold text-foreground">{message}</Heading>
            <MarkLineBox size="text-base">{headingMark}</MarkLineBox>
          </div>
        ) : (
          <Heading className="text-base font-semibold text-foreground">{message}</Heading>
        )}
        {description && <p className="max-w-sm text-sm break-words text-app-muted">{description}</p>}
      </div>
      {remedy && <p className="-mt-3 max-w-sm text-sm break-words text-app-muted">{remedy}</p>}
      {(action || secondaryAction || details) && (
        <div className="flex flex-wrap items-center justify-center gap-3">
          {action && (
            // A press never takes focus: it would drop to `<body>` when the panel goes away, and
            // whatever the action opens or starts places focus itself.
            <Button
              ref={actionRef}
              data-region-landing={action.landing ? "" : undefined}
              size="sm"
              variant={action.variant ?? "secondary"}
              preventFocusOnPress
              isPending={action.isPending}
              onPress={action.onPress}
            >
              {ActionIcon && <ActionIcon aria-hidden="true" className="size-4" />}
              {action.label}
            </Button>
          )}
          {secondaryAction && (
            <Button size="sm" variant="secondary" preventFocusOnPress onPress={secondaryAction.onPress}>
              {SecondaryIcon && <SecondaryIcon aria-hidden="true" className="size-4" />}
              {secondaryAction.label}
            </Button>
          )}
          {details && <DetailsButton placement="bottom">{details}</DetailsButton>}
        </div>
      )}
    </EmptyState>
  );
}

/** An `EmptyPanel` centred in the column it fills, for a view whose content starts at the top when
 * there is any: only the empty branch takes the centring, so a list's rows still begin where the
 * column does. The column itself stays the parent's, so nothing it holds while it is not empty
 * moves. Safe centring, here and at every other surface that centres a panel: plain `center` pushes
 * the overflow of a panel taller than its column past the start edge, where a scrolling column
 * cannot reach it and a plain one draws it over whatever sits above. */
export function CenteredEmptyPanel(props: React.ComponentProps<typeof EmptyPanel>): React.ReactElement {
  return (
    <div className="flex min-h-0 flex-1 flex-col justify-center-safe">
      <EmptyPanel {...props} />
    </div>
  );
}
