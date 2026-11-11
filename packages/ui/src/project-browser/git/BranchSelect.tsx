import { GitBranch } from "lucide-react";
import type React from "react";

import { FadeOverflow } from "../../components/text/FadeOverflow";
import { MotionListBox } from "../../components/motion/MotionListBox";
import { InlineReference } from "../../i18n/InlineReference";
import { StatusBadge } from "../../components/status/StatusBadge";
import { useScrollFade } from "../../components/scroll/scrollFade";
import { Message, useT } from "../../i18n/react";
import type { BranchInfo } from "../../protocol";
import { displayWirePath } from "../../format/wirePath";
import { BranchOriginBadge } from "./BranchOriginBadge";
import { CommitIdLabel } from "./CommitIdLabel";
import { Label, ListBoxItem, Select, SelectPopover, SelectTrigger } from "../../primitives";

/** Every option in a branch selector is one line of this height, which is what lets a repository's
 * thousands of branches be listed virtualized. */
const BRANCH_ROW_HEIGHT = 32;

/** A choice of one of the repository's local branches: its label in the grid's first column, its
 * trigger in the second. A value the list does not have is named as gone (`gone`), as a branch read
 * from worktrees older than the list can be. */
export function BranchSelect({
  label,
  branches,
  value,
  gone,
  isDisabled,
  isLoading,
  selectedSessionId,
  onChange,
}: {
  label: string;
  branches: BranchInfo[];
  value: string | undefined;
  gone: boolean;
  /** For a repository that has no branches to offer: a selector that could only open on an empty
   * list is a control with nothing behind it. */
  isDisabled?: boolean;
  isLoading?: boolean;
  selectedSessionId: string | undefined;
  onChange: (branch: string) => void;
}): React.ReactElement {
  const t = useT();
  const branchFade = useScrollFade({ className: "max-h-80 overflow-y-auto px-0" });
  const name = value === undefined ? undefined : displayWirePath(value);
  const title = name === undefined ? undefined : gone ? <Message id="git.branch.gone" params={{ branch: <InlineReference monospace wrap>{name}</InlineReference> }} /> : name;
  return (
    // The subgrid restates the column gap: HeroUI's `.select` sets a gap of its own, which would
    // otherwise replace the parent grid's for this row's gutter. The default variant is used, as it is for a control
    // on a pane's own background.
    <Select
      fullWidth
      disabled={isDisabled}
      loading={isLoading}
      value={value ?? null}
      onChange={(key) => key !== null && onChange(key)}
      className="col-span-2 grid grid-cols-subgrid items-center gap-x-2"
    >
      <Label className="text-sm">{label}</Label>
      <SelectTrigger
        className="group min-w-0"
        valueClassName="flex min-w-0 items-center gap-2"
        value={
          <>
            {!(isLoading && name === undefined) && <GitBranch aria-hidden="true" className="size-4 shrink-0 text-app-muted" />}
            <FadeOverflow
              as="span"
              dir={name === undefined ? undefined : "ltr"}
              clip={name === undefined ? "end" : "start"}
              className="min-w-0 flex-1"
              titleWhenClipped={title}
            >
              {name ?? t(isLoading ? "common.loading" : "git.branch.choose")}
            </FadeOverflow>
            {name !== undefined && gone && <StatusBadge>{t("git.branch.goneStatus")}</StatusBadge>}
          </>
        }
      />
      {/* As wide as the trigger: the virtualized list lays its rows out to the popover's width
          rather than taking one from them, and without a width of its own the popover grows past
          the window. */}
      <SelectPopover className="w-(--trigger-width)">
        {/* The side inset moves from the scroll container onto the rows: the virtualized layout
              makes each row as wide as the list's inner width and places it inside the list's own
              padding, so a padded list overflows sideways and clips the rows' focus ring. `pe-7` on
              a row restores the room HeroUI reserves for the check mark, which its Select styles
              override with a narrower padding, so the commit would run under the mark. */}
        {/* The fade goes on the list, never on the popover around it: the popover is what animates
              away, and the fade's scroll-timeline animation never ends, so the popover's exit
              transition would wait for it forever. */}
        <MotionListBox rowHeight={BRANCH_ROW_HEIGHT} items={branches} dependencies={[selectedSessionId]} {...branchFade}>
          {(branch) => {
            const branchName = displayWirePath(branch.name);
            return (
              <ListBoxItem data-list-motion={branch.name} id={branch.name} textValue={branchName} indicator className="mx-1.5 h-8 min-h-0 w-auto py-0 pe-7">
                <FadeOverflow as="span" dir="ltr" clip="start" className="min-w-0 flex-1" titleWhenClipped={branchName}>
                  {branchName}
                </FadeOverflow>
                {branch.origin && <BranchOriginBadge origin={branch.origin} selectedSessionId={selectedSessionId} />}
                <CommitIdLabel commit={branch.commit} />
              </ListBoxItem>
            );
          }}
        </MotionListBox>
      </SelectPopover>
    </Select>
  );
}
