/** Why a check for an update or an install did not go through. `busy`, `installed` and `nothing_to_install` are
 * refusals of the shell that the update store answers for itself (a busy shell is left to finish, an installed update
 * awaits its restart, an update the shell no longer holds is withdrawn); `failed` carries the updater's own text as
 * `message`, which cannot be localized. */
export class UpdaterError extends Error {
  readonly kind: "busy" | "installed" | "nothing_to_install" | "failed";
  constructor(kind: UpdaterError["kind"], detail = "") {
    super(detail);
    this.kind = kind;
  }
}
