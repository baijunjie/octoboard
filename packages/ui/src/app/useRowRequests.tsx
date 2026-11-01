import { GitBranch } from "lucide-react";

import { ActionableError } from "../errors/actionableError";
import { useStableHandler } from "../components/hooks/useLatest";
import { copyText } from "../components/host/copyText";
import type { ActionMenuEntry } from "../components/menus/ActionMenu";
import { reportExternalLinkFailure } from "../components/text/ExternalLink";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import type { ConsoleSessionColour, Project, Session } from "../protocol";
import { useDaemon } from "../store";
import { displayWirePath } from "../format/wirePath";
import { raiseError, alertFailure } from "../components/alerts/raiseError";

/**
 * The project and session rows' requests that only ask the daemon or the host for something and
 * say how it went: Git pulls, pushes and branch switches, revealing and copying, the terminal app,
 * the repository's page, and a session's colour, lock and pin. Each is one function for good
 * (`useStableHandler`), as every row of the sidebar takes it. The repository opener is there only
 * where the platform can open links.
 */
export function useRowRequests() {
  const t = useT();
  const { clipboard, links } = usePlatform();
  const { request, toastNotice } = useDaemon();

  const pullProjectBranch = (project: Project) => {
    request({ type: "pull_project_branch", project: project.id }).catch(alertFailure({ title: t("sidebar.project.pullFailedTitle") }));
  };

  const openRepository = (url: string) => {
    void links?.open(url).catch((error: unknown) => reportExternalLinkFailure(t, url, error));
  };

  const openProjectDirectory = (project: Project) => {
    request({ type: "open_project_directory", project: project.id }).catch(alertFailure({ title: t("common.revealFailedTitle") }));
  };

  const copyProjectName = (project: Project) => {
    copyText(project.name, { clipboard }).then(
      () => toastNotice(t("sidebar.project.nameCopied")),
      (cause: unknown) => raiseError(new ActionableError(t("sidebar.project.copyNameFailed"), "clipboard", { cause }), { title: t("common.copyFailedTitle") }),
    );
  };

  const copySessionId = (session: Session) => {
    copyText(session.id, { clipboard }).then(
      () => toastNotice(t("sidebar.session.idCopied")),
      (cause: unknown) => raiseError(new ActionableError(t("sidebar.session.copyIdFailed"), "clipboard", { cause }), { title: t("common.copyFailedTitle") }),
    );
  };

  const openSystemTerminal = (project: Project) => {
    request({ type: "open_project_terminal", project: project.id }).catch(alertFailure({ title: t("sidebar.project.openSystemTerminalFailedTitle") }));
  };

  const pushProjectBranch = (project: Project) => {
    request({ type: "push_project_branch", project: project.id })
      .then(() => toastNotice(t("sidebar.project.pushed")))
      .catch(alertFailure({ title: t("sidebar.project.pushFailedTitle") }));
  };

  const loadProjectBranches = async (project: Project): Promise<ActionMenuEntry[]> => {
    const [reply, source] = await Promise.all([
      request({ type: "list_project_branches", project: project.id }),
      request({ type: "get_project_source", project: project.id }),
    ]);
    if (reply.type !== "project_branches") throw new Error(t("sidebar.project.branchesFailed"));
    const git = source.type === "project_source" ? source.source.git : undefined;
    const currentBranch = git?.worktrees.find((worktree) => worktree.id === git.worktree)?.branch;
    const entries: ActionMenuEntry[] = reply.branches.map((branch) => {
      const name = displayWirePath(branch.name);
      const current = currentBranch === branch.name;
      return {
        label: name,
        content: <FadeOverflow as="span" dir="ltr" clip="start" className="block" titleWhenClipped={name}>{name}</FadeOverflow>,
        icon: GitBranch,
        selected: current,
        onClick: () => {
          if (current) return;
          request({ type: "switch_project_branch", project: project.id, branch: branch.name })
            .catch(alertFailure({ title: t("sidebar.project.switchBranchFailedTitle") }));
        },
      };
    });
    if (!entries.length) entries.push({ label: t("sidebar.project.noBranches"), icon: GitBranch, disabled: true, onClick: () => {} });
    if (!reply.complete) entries.push({ label: t("sidebar.project.branchesBeyondList"), note: true });
    return entries;
  };

  const setSessionColour = (session: Session, colour: ConsoleSessionColour) => {
    request({ type: "set_session_colour", session: session.id, colour }).catch(alertFailure({ title: t("sidebar.session.colourFailedTitle") }));
  };

  const setLocked = (session: Session, locked: boolean) => {
    request({ type: "set_session_locked", session: session.id, locked }).catch((err: unknown) =>
      raiseError(err, { title: t(locked ? "sidebar.session.lockFailedTitle" : "sidebar.session.unlockFailedTitle") }));
  };

  const setPinned = (target: { project: Project } | { session: Session }, pinned: boolean) => {
    const [body, failedTitle] =
      "project" in target
        ? [{ type: "update_project", project: target.project.id, pinned } as const, pinned ? "sidebar.project.pinFailedTitle" as const : "sidebar.project.unpinFailedTitle" as const]
        : [{ type: "set_session_pinned", session: target.session.id, pinned } as const, pinned ? "sidebar.session.pinFailedTitle" as const : "sidebar.session.unpinFailedTitle" as const];
    request(body).catch(alertFailure({ title: t(failedTitle) }));
  };

  const stableSetSessionColour = useStableHandler(setSessionColour);
  const stableOpenProjectDirectory = useStableHandler(openProjectDirectory);
  const stableOpenRepository = useStableHandler(openRepository);
  const stableOpenSystemTerminal = useStableHandler(openSystemTerminal);
  const stableCopySessionId = useStableHandler(copySessionId);
  const stableCopyProjectName = useStableHandler(copyProjectName);
  const stablePullProjectBranch = useStableHandler(pullProjectBranch);
  const stablePushProjectBranch = useStableHandler(pushProjectBranch);
  const stableLoadProjectBranches = useStableHandler(loadProjectBranches);
  const stableSetLocked = useStableHandler(setLocked);
  const stableSetPinned = useStableHandler(setPinned);
  const stableOpenRepositoryIfAny = links ? stableOpenRepository : undefined;
  return {
    stableSetSessionColour,
    stableOpenProjectDirectory,
    stableOpenRepository: stableOpenRepositoryIfAny,
    stableOpenSystemTerminal,
    stableCopySessionId,
    stableCopyProjectName,
    stablePullProjectBranch,
    stablePushProjectBranch,
    stableLoadProjectBranches,
    stableSetLocked,
    stableSetPinned,
  };
}
