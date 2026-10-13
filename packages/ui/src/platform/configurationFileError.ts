/** Why a configuration file could not be saved or opened: a file too large to be one, a file that
 * is not text, or anything else, whose `detail` is the system's own text. */
export class ConfigurationFileError extends Error {
  readonly kind: "too_large" | "not_text" | "io";
  constructor(kind: ConfigurationFileError["kind"], detail = "") {
    super(detail);
    this.kind = kind;
  }
}
