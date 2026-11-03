/** A provider's mark as the path of a 24-unit square and its fill, `currentColor` for a mark drawn in the text colour. */
export interface ProviderMark {
  readonly d: string;
  readonly fill: string;
}

/** A provider's colour in each appearance, as `#rrggbb`. */
export interface ProviderColour {
  readonly light: string;
  readonly dark: string;
}

/** The messages of a provider the components look up by name, `provider.<id>.<name>` (see `ProviderMessage`). A provider
 * without one is worded by the generic message the component falls back to, or by none. */
export type ProviderMessageName =
  /** What trusting a project's own configuration lets the provider do. */
  | "trustCaution"
  /** How far the trust the provider records reaches beyond the project asked about, when it reaches further. */
  | "trustScope"
  /** What a shared account configuration never shares, beyond the login credentials. */
  | "sharingNeverShared"
  /** What an account's configuration directory needs besides existing, when it needs more. */
  | "dirHint";

/** What the UI presents of one provider, apart from the facts the daemon serves about it (`ProviderInfo`): its mark, its
 * colour, its brand-name placeholders, its messages and its error codes. `messages` holds the provider's catalog entries
 * per language, keys under `provider.<id>.`, for the catalogs to merge in: its names, the copy named by
 * `ProviderMessageName`, the copy of each setting its descriptor declares (`provider.<id>.setting.<setting>.*`, see
 * `settingMessageKey`) and the words of each code it declares (`provider.<id>.<code>`, with `.summary`, `.recovery` and
 * variants as for a core code). A provider without a `colour` draws in the neutral foreground. */
export interface ProviderPresentation<Id extends string = string, Messages extends Record<string, string> = Record<string, string>> {
  readonly id: Id;
  readonly mark: ProviderMark;
  readonly colour?: ProviderColour;
  /** The brand names its messages (and other providers') use as global placeholders (`{claude}` for "Claude Code"),
   * which no caller passes: the same in every language. */
  readonly params?: Readonly<Record<string, string>>;
  /** The literals (file names) a message names, set apart as references in the message's own placeholders: the
   * message `provider.<id>.<name>` fills `{<placeholder>}` with the text beside it. */
  readonly references?: { readonly [Name in ProviderMessageName]?: Readonly<Record<string, string>> };
  /** The literals a setting's description names, set apart as references like `references`, by setting id. */
  readonly settingReferences?: Readonly<Record<string, Readonly<Record<string, string>>>>;
  /** The display names of plans whose id would read badly in the generic case (`self_serve_business_prolite`), by the
   * plan id the daemon serves. */
  readonly planNames?: Readonly<Record<string, string>>;
  /** What the UI knows of each error or notice code the provider's descriptor declares (`provider.<id>.<code>`), by
   * code: the same record the core codes have (`CodeRecord` in `errors/codes.ts`, which this layer cannot import: the
   * registry there checks the merged records against it). The code's words are among `messages`, under the code
   * itself. */
  readonly codes?: Readonly<Record<string, object>>;
  readonly messages: { readonly en: Messages; readonly "zh-Hans": Messages };
}
