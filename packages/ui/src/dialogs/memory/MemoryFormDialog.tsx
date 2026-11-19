import React, { useState } from "react";

import { ErrorAlert } from "../../components/alerts/ErrorAlert";
import { StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { ChangedElsewhereAnnouncer } from "../../components/announce/ChangedElsewhereAnnouncer";
import { Dialog } from "../../components/dialog/Dialog";
import { useDialogAction, useSubmitValidation } from "../../components/dialog/dialogHooks";
import { ConfirmOption } from "../../components/form/ConfirmOption";
import { TextAreaInput } from "../../components/form/TextAreaInput";
import { TextInput } from "../../components/form/TextInput";
import { useCheckedEdit } from "../../components/hooks/useCheckedEdit";
import { CODE } from "../../errors/codes";
import { CodedError } from "../../errors/codedError";
import { useT } from "../../i18n/react";
import { useDaemon } from "../../store";
import type { HeldMemory } from "./memoryState";
import { targetRequest, type MemoryTarget } from "./memoryTarget";
import {
  hasMemoryContent,
  isValidMemorySummary,
  MEMORY_MAX_CONTENT_BYTES,
  MEMORY_MAX_SUMMARY_CHARS,
  MEMORY_MAX_ENTRIES,
  memoryContentBytes,
} from "./memoryRules";

type Field = "summary" | "local" | "content";

/** The form that adds an entry to a memory (a console's or a project place's), or edits one (`editing` is its id). A
 * console's entry also has the machine-only flag; a project place's entries all hold only on the machine the project is on.
 *
 * An edit is checked against the version of the entry the form read: the daemon refuses it when a console session or
 * another window saved the entry since, so nothing is overwritten unseen. The entry is read from `memory` as it stands,
 * so a change arriving while the form is open (the same case before the refusal) puts each field it changed back to the
 * current value and announces it where the user had edited it, as the console dialog does; an entry deleted meanwhile
 * says so and cannot be saved. */
export function MemoryFormDialog({ target, editing, memory, reread, onClose }: {
  target: MemoryTarget;
  /** The id of the entry being edited; a new entry when left out. */
  editing?: string;
  memory: HeldMemory;
  /** Reads the entry being edited from the daemon again, for when a save is refused because it changed. */
  reread?: () => Promise<unknown>;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const live = editing === undefined ? undefined : memory.contents.get(editing);
  const flagged = target.kind === "console";
  // What the form read of the entry, which an edit is judged against.
  const [read, setRead] = useState(() => ({ summary: live?.summary ?? "", local: live?.local ?? false, content: live?.content ?? "" }));
  const [summary, setSummary] = useState(read.summary);
  const [local, setLocal] = useState(read.local);
  const [content, setContent] = useState(read.content);
  const { busy, run } = useDialogAction(undefined, t("memory.saveFailedTitle"));
  const { shown, attempt } = useSubmitValidation();

  // The entry's one version stands for each field a form edits, so a field moves to the current value only where its
  // value differs from what the form read.
  const versions = (version: number) => ({ summary: version, local: version, content: version });
  const edit = useCheckedEdit(live ? versions(live.version) : {}, live ? versions(live.version) : undefined, {
    same: (path) => {
      if (!live) return true;
      if (path === "summary") return live.summary === read.summary;
      if (path === "local") return live.local === read.local;
      return live.content === read.content;
    },
    edited: (path) => (path === "summary" ? summary !== read.summary : path === "local" ? local !== read.local : content !== read.content),
    reset: (paths) => {
      if (!live) return;
      const next = { ...read };
      if (paths.includes("summary")) {
        setSummary(live.summary);
        next.summary = live.summary;
      }
      if (paths.includes("local")) {
        setLocal(live.local);
        next.local = live.local;
      }
      if (paths.includes("content")) {
        setContent(live.content);
        next.content = live.content;
      }
      setRead(next);
    },
    writing: busy,
  });
  const fieldLabel = (path: string) => t(`memory.field.${path as Field}`);
  const deleted = editing !== undefined && !live;

  const summaryError = !isValidMemorySummary(summary) ? t("daemon.invalid_memory_summary", { limit: MEMORY_MAX_SUMMARY_CHARS }) : undefined;
  const bytes = memoryContentBytes(content);
  const contentError = !hasMemoryContent(content)
    ? t("daemon.memory_content_empty")
    : bytes > MEMORY_MAX_CONTENT_BYTES
      ? t("daemon.memory_content_too_long", { limit: MEMORY_MAX_CONTENT_BYTES, bytes })
      : undefined;
  const full = editing === undefined && memory.index.length >= MEMORY_MAX_ENTRIES;
  const fullError = full ? t("daemon.memory_full", { limit: MEMORY_MAX_ENTRIES }) : undefined;

  const submit = () => {
    if (!attempt(summaryError, contentError, fullError)) return;
    void run(async () => {
      // No try block here: the React Compiler does not compile a component whose try holds value blocks.
      await request({
        type: "save_memory",
        ...targetRequest(target),
        summary,
        content,
        ...(flagged && { local }),
        ...(editing !== undefined && { memory: editing, version: edit.base(["content"])?.content }),
      }).catch((error: unknown) => {
        // The entry was saved since the form read it, by something the window had not heard of yet: reading it again
        // brings the current entry into the form, as a change heard while it is open does.
        if (error instanceof CodedError && error.code === CODE.memory_changed) void reread?.().catch(() => {});
        throw error;
      });
      onClose();
    });
  };

  return (
    <Dialog
      title={t(editing === undefined ? "memory.entry.addTitle" : "memory.entry.editTitle")}
      onClose={onClose}
      submitLabel={t(editing === undefined ? "common.add" : "common.save")}
      submitDisabled={deleted}
      busy={busy}
      onSubmit={submit}
      size="lg"
    >
      <ChangedElsewhereAnnouncer count={edit.replaced.count} labels={edit.replaced.paths.map(fieldLabel)} onSaid={edit.said} />
      {shown(fullError) && <ErrorAlert status="danger" announce title={fullError} />}
      <StatusAnnouncer text={deleted ? t("memory.entryDeleted") : undefined} />
      {deleted && <p className="text-sm text-app-muted">{t("memory.entryDeleted")}</p>}
      <TextInput
        label={t("memory.field.summary")}
        value={summary}
        onChange={setSummary}
        placeholder={t("common.example", { example: t("memory.field.summaryExample") })}
        description={t("memory.field.summaryNote")}
        errorMessage={shown(summaryError)}
        characterLimit={MEMORY_MAX_SUMMARY_CHARS}
        autoFocus
      />
      <TextAreaInput
        label={t("memory.field.content")}
        value={content}
        onChange={setContent}
        placeholder={t("common.example", { example: t("memory.field.contentExample") })}
        description={t("memory.field.contentNote", { limit: MEMORY_MAX_CONTENT_BYTES })}
        errorMessage={shown(contentError)}
        rows={10}
      />
      {flagged && <ConfirmOption isSelected={local} onChange={setLocal} label={t("memory.field.local")} hint={t("memory.field.localNote")} />}
    </Dialog>
  );
}
