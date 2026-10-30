import { Checkbox, Label } from "@heroui/react";
import React, { useId, useState } from "react";

import { ProcessCommand, ProcessTreeDetails, StopGlyph } from "../components/ProcessTreeDetails";
import { useT } from "../i18n/react";
import type { ProcessTree } from "../protocol";
import { useDaemonStore } from "../store";
import type { ConsequenceLine } from "./BranchCleanupOptions";
import { OptionDetails } from "./OptionDetails";

/** One process tree an archive stops while selected: the command on one line is the checkbox's
 * label, with the whole command as its tooltip when the line cuts it, and under it a disclosure of
 * where it works and what it is doing. */
function StopOption({ tree, isSelected, onChange, isReadOnly }: {
  tree: ProcessTree;
  isSelected: boolean;
  onChange: (selected: boolean) => void;
  isReadOnly: boolean;
}): React.ReactElement {
  const t = useT();
  const optionId = useId();
  return (
    <div className="flex flex-col gap-1">
      <div id={optionId}>
        <Checkbox variant="secondary" className="w-full min-w-0 max-w-full" isSelected={isSelected} onChange={onChange} isReadOnly={isReadOnly}>
          <Checkbox.Content className="min-h-6 w-full min-w-0">
            <Checkbox.Control><Checkbox.Indicator /></Checkbox.Control>
            <Label className="min-w-0 flex-1"><ProcessCommand tree={tree} leading={isSelected ? <StopGlyph /> : undefined} /></Label>
          </Checkbox.Content>
        </Checkbox>
      </div>
      {/* Under the command's text: the checkbox, its gap and the glyph come before it. */}
      <div className="ps-12">
        <OptionDetails describedBy={optionId} name={t("processes.details")}>
          <ProcessTreeDetails tree={tree} />
        </OptionDetails>
      </div>
    </div>
  );
}

/** One tree that is always stopped, such as a console session's in an archive or a project process in a project's removal: a row of its own, with no checkbox, since
 * there is no choice to make (a disabled checkbox would show an option the user cannot reach, and drop out
 * of the Tab order), with the same details disclosure under it. */
export function StoppedRow({ tree }: { tree: ProcessTree }): React.ReactElement {
  const t = useT();
  const rowId = useId();
  return (
    <div className="flex flex-col gap-1">
      <div id={rowId} className="min-h-6">
        <ProcessCommand tree={tree} leading={<StopGlyph />} />
      </div>
      <div className="ps-5">
        <OptionDetails describedBy={rowId} name={t("processes.details")}>
          <ProcessTreeDetails tree={tree} />
        </OptionDetails>
      </div>
    </div>
  );
}

/** The processes an archive can stop. Each checkbox starts selected; unchecked trees are sent as
 * `keep_process_ids`. Choices are kept by tree id across inspections, and a new tree starts selected.
 * A console session offers no Keep, so its trees are listed apart as rows that are stopped whatever is
 * chosen, under a line saying so, and count as stopped. */
export function useProcessStopping(processes: ProcessTree[]): {
  choices: { keep_process_ids?: string[] };
  consequences: ConsequenceLine[];
  extraControl: (busy: boolean) => React.ReactNode;
} {
  const t = useT();
  const sessions = useDaemonStore((s) => s.sessions);
  const [keptIds, setKeptIds] = useState<ReadonlySet<string>>(new Set());
  const always = processes.filter((tree) => tree.session_id !== undefined && sessions.get(tree.session_id)?.role === "console");
  const keepable = processes.filter((tree) => !always.includes(tree));
  const kept = keepable.filter((tree) => keptIds.has(tree.id));
  const stopped = processes.length - kept.length;
  return {
    choices: kept.length > 0 ? { keep_process_ids: kept.map((tree) => tree.id) } : {},
    consequences: [
      ...(stopped > 0 ? [{ key: "processesStopped", text: t("dialog.archiveSession.processesStopped", { count: stopped }) }] : []),
      ...(kept.length > 0 ? [{ key: "processesKept", text: t("dialog.archiveSession.processesKept", { count: kept.length }) }] : []),
    ],
    extraControl: (busy) => (
      <div className="flex min-w-0 flex-col gap-3">
        {keepable.length > 0 && (
          <div className="flex min-w-0 flex-col gap-2">
            <p>{t("dialog.archiveSession.processesStopLead")}</p>
            <div className="flex min-w-0 flex-col gap-2">
              {keepable.map((tree) => (
                <StopOption
                  key={tree.id}
                  tree={tree}
                  isSelected={!keptIds.has(tree.id)}
                  isReadOnly={busy}
                  onChange={(selected) => setKeptIds((current) => {
                    const next = new Set(current);
                    if (selected) next.delete(tree.id);
                    else next.add(tree.id);
                    return next;
                  })}
                />
              ))}
            </div>
          </div>
        )}
        {always.length > 0 && (
          <div className="flex min-w-0 flex-col gap-2">
            <p>{t("dialog.archiveSession.processesConsoleLead")}</p>
            <div className="flex min-w-0 flex-col gap-2">
              {always.map((tree) => <StoppedRow key={tree.id} tree={tree} />)}
            </div>
          </div>
        )}
      </div>
    ),
  };
}
