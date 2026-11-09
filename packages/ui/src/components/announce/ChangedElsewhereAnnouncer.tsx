import React from "react";

import { joinValues } from "../../i18n/joinPhrases";
import { useCurrentLanguage, useT } from "../../i18n/react";
import { ChangeAnnouncer } from "./ChangeAnnouncer";

/** `ChangeAnnouncer` for fields that were replaced because they changed elsewhere: `labels` are the labels of the fields
 * of the latest change, said as a list, the sentence agreeing with how many they are. `onSaid` is `ChangeAnnouncer`'s: a
 * form that gathers the fields of changes arriving while one waits starts afresh from it. */
export function ChangedElsewhereAnnouncer({ count, labels, onSaid }: { count: number; labels: readonly string[]; onSaid?: () => void }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const unique = [...new Set(labels)];
  return <ChangeAnnouncer count={count} text={t("dialog.changedElsewhere", { count: unique.length, fields: joinValues(language, unique) })} onSaid={onSaid} />;
}
