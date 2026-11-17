import { GitBranch } from "lucide-react";
import type React from "react";

import { FadeOverflow } from "../../components/text/FadeOverflow";
import { OptionSelect } from "../../components/form/OptionSelect";
import { InlineReference } from "../../i18n/InlineReference";
import { StatusBadge } from "../../components/status/StatusBadge";
import { Message, useT } from "../../i18n/react";
import type { BranchInfo } from "../../protocol";
import { displayWirePath } from "../../format/wirePath";
import { BranchOriginBadge } from "./BranchOriginBadge";
import { CommitIdLabel } from "./CommitIdLabel";

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
  const name = value === undefined ? undefined : displayWirePath(value);
  const title = name === undefined ? undefined : gone ? <Message id="git.branch.gone" params={{ branch: <InlineReference monospace wrap>{name}</InlineReference> }} /> : name;
  return (
    <OptionSelect
      label={label}
      besideLabel
      variant="primary"
      virtualized
      isDisabled={isDisabled}
      isLoading={isLoading}
      value={value ?? ""}
      onChange={onChange}
      options={branches.map((branch) => ({
        value: branch.name,
        label: displayWirePath(branch.name),
        labelDir: "ltr",
        labelClip: "start",
        endContent: <>
          {branch.origin && <BranchOriginBadge origin={branch.origin} selectedSessionId={selectedSessionId} />}
          <CommitIdLabel commit={branch.commit} />
        </>,
      }))}
      valueContent={<>
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
      </>}
    />
  );
}
