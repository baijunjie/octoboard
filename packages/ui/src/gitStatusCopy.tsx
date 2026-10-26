import React from "react";

import { InlineReference } from "./components/InlineReference";
import type { Translate } from "./i18n/catalog";
import { joinNodes } from "./i18n/joinPhrases";
import type { Language } from "./i18n/languages";
import { Message } from "./i18n/react";
import type { GitStatus } from "./protocol";

/** The tooltip's rich facts, `gitBadgeAriaLabel`'s sentence with the branch drawn as a reference. A failed
 * check is not among them: the failure mark beside the badge holds it, Git's own text included. */
export function gitBadgeContent(language: Language, t: Translate, status: GitStatus): React.ReactNode {
  const branch = status.branch == null ? undefined : <InlineReference monospace wrap>{status.branch}</InlineReference>;
  const phrases: React.ReactNode[] = [];
  if (status.activity !== "idle") {
    const key = status.activity === "pushing" ? "sidebar.git.pushing" : status.activity === "pulling" ? "sidebar.git.pulling" : "sidebar.git.checking";
    const unknown = status.activity === "pushing" ? "sidebar.git.pushingUnknown" : status.activity === "pulling" ? "sidebar.git.pullingUnknown" : "sidebar.git.checkingUnknown";
    phrases.push(branch ? <Message id={key} params={{ branch }} /> : t(unknown));
  } else if (branch) {
    phrases.push(<Message id={status.detached ? "sidebar.git.detachedBranch" : "sidebar.git.branch"} params={{ branch }} />);
  } else if (!status.error) {
    phrases.push(t("sidebar.git.noCommitsYet"));
  }
  if (status.ahead > 0) phrases.push(t("sidebar.git.ahead", { count: status.ahead }));
  if (status.behind > 0) phrases.push(t("sidebar.git.behind", { count: status.behind }));
  return joinNodes(language, phrases.map((_, index) => String(index)), (index, key) => <React.Fragment key={key}>{phrases[Number(index)]}</React.Fragment>);
}
