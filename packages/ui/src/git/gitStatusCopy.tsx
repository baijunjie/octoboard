import React from "react";

import { InlineReference } from "../i18n/InlineReference";
import type { Translate } from "../i18n/catalog";
import { joinNodes } from "../i18n/joinPhrases";
import type { Language } from "../i18n/languages";
import { Message } from "../i18n/react";
import type { GitStatus } from "../protocol";
import { gitBranchPhrase, gitCountPhrases } from "./gitStatusLabel";

/** The tooltip's rich facts, `gitBadgeAriaLabel`'s sentence with the branch drawn as a reference. A failed
 * check is not among them: the failure mark beside the badge holds it, Git's own text included. */
export function gitBadgeContent(language: Language, t: Translate, status: GitStatus): React.ReactNode {
  const branch = gitBranchPhrase(status);
  const phrases: React.ReactNode[] = [];
  if (branch) {
    phrases.push("branch" in branch
      ? <Message id={branch.key} params={{ branch: <InlineReference monospace wrap>{branch.branch}</InlineReference> }} />
      : t(branch.key));
  }
  phrases.push(...gitCountPhrases(t, status));
  return joinNodes(language, phrases.map((_, index) => String(index)), (index, key) => <React.Fragment key={key}>{phrases[Number(index)]}</React.Fragment>);
}
