import { isMessageKey, type MessageKey, type PlainMessageKey, type Translate } from "../i18n/catalog";
import type { Agent, AgentAvailability, ModelDisplay, ProviderInfo, SettingDescriptor } from "../protocol";
import type { ProviderMessageName } from "../providers/types";

/** The UI's records of the codes the providers declare (`PROVIDER_CODES`, for the code registry) and their plan names
 * (`providerPlanName`, for `accounts/quotaFormat.ts`). Re-exported here because `errors/` and `accounts/` may import
 * `agents/` but not `providers/`, which holds them. */
export { PROVIDER_CODES, providerPlanName } from "../providers/registry";

/** The name of provider `id` in full ("Claude Code"). A provider with a presentation module is named by the module's
 * message; any other by the label the daemon serves of it in `providers`, else by its id (a provider the daemon does
 * not serve). `providers` is required so that no caller can leave the served label out. */
export function providerLabel(t: Translate, providers: readonly ProviderInfo[], id: string): string {
  const key = providerLabelKey(id);
  return isMessageKey(key) ? t(key as PlainMessageKey) : (servedProvider(providers, id)?.label ?? id);
}

/** The key of the message that names provider `id` in full, which only a provider with a presentation module has. */
function providerLabelKey(id: string): string {
  return `provider.${id}.label`;
}

/** The shorter name of provider `id` ("Claude" for "Claude Code"), as the menu bar icon's menu uses it. A provider
 * without a presentation module is named by the short name the daemon serves of it in `providers`, else by its id.
 * Brand names, so not translated. */
export function providerShortName(t: Translate, providers: readonly ProviderInfo[], id: string): string {
  const key = `provider.${id}.shortName`;
  return isMessageKey(key) ? t(key as PlainMessageKey) : (servedProvider(providers, id)?.short_name ?? id);
}

/** Whether provider `id` has a name of its own to be shown by (see `providerLabel`): a presentation module's, or the
 * label the daemon serves of it; not a provider known only by its bare id. */
export function hasProviderName(providers: readonly ProviderInfo[], id: string): boolean {
  return isMessageKey(providerLabelKey(id)) || isKnownAgent(providers, id);
}

/** Whether `id` is the id of a provider the daemon serves. */
export function isKnownAgent(providers: readonly ProviderInfo[], id: string): boolean {
  return servedProvider(providers, id) !== undefined;
}

/** The registered provider `id`, `undefined` while the daemon has not served the providers or serves none by that id. */
export function servedProvider(providers: readonly ProviderInfo[], id: string): ProviderInfo | undefined {
  return providers.find((provider) => provider.id === id);
}

/** The effective provider switch value, falling back to its declared default before settings arrive; a default this
 * build does not know (a newer daemon's) reads off, so nothing is switched on by it. */
export function switchValue(descriptor: SettingDescriptor, stored: unknown): boolean {
  return typeof stored === "boolean" ? stored : descriptor.kind.type === "switch" && descriptor.kind.default === "on";
}

/** How a model id of provider `id` is shown, as the daemon serves it; `undefined` while it has not served the providers
 * or serves none by that id, when an id is shown as it is. */
export function providerModelDisplay(providers: readonly ProviderInfo[], id: string): ModelDisplay | undefined {
  return servedProvider(providers, id)?.model_display;
}

/** The agent a control opens on when nothing else chooses one: the first registered provider, or, before the daemon has
 * served them (when no dialog is open), the empty id, which names no provider. */
export function defaultProviderAgent(providers: readonly ProviderInfo[]): Agent {
  return providers[0]?.id ?? "";
}

/** Where the provider's own instructions for installing it are: the provider's page, never ours. */
export function providerInstallUrl(providers: readonly ProviderInfo[], id: string): string | undefined {
  return servedProvider(providers, id)?.install_url;
}

/** Whether provider `id` can give accounts beyond the default one a directory each (`capabilities.owned_accounts`):
 * where it cannot, the window offers no Add account for it and its section shows the default account only. A provider
 * the daemon does not serve offers none. A daemon that predates the capability never sends it, and every provider it
 * serves offered owned accounts, so a missing field reads as offered: only an explicit `false` withholds them. */
export function offersOwnedAccounts(providers: readonly ProviderInfo[], id: string): boolean {
  const served = servedProvider(providers, id);
  // The generated type has the field required, but a released daemon's snapshot does not carry it.
  return served !== undefined && (served.capabilities.owned_accounts as boolean | undefined) !== false;
}

/** Whether Octoboard reads a plan quota for provider `id`: the provider declares at least one quota window kind
 * (`quota_windows.kinds`). A provider that declares none has no usage to show anywhere, no tab in the usage panel and
 * no bar in an account list, and its accounts are never waited on for a reading. Read from the declaration, never from
 * a reading. A provider the daemon does not serve has none to withhold, so it reads as having one. */
export function hasPlanQuota(providers: readonly ProviderInfo[], id: string): boolean {
  const served = servedProvider(providers, id);
  return served === undefined || served.quota_windows.kinds.length > 0;
}

/** Whether the window can sign an account of provider `id` in without a terminal (`capabilities.login_flow`), which is
 * what Log in drives. Where it cannot, an account that is not logged in says how to log in with the agent's own command
 * instead (the provider's `signInHint`). A provider the daemon does not serve offers none. A daemon that predates the
 * capability never sends it, and every provider it serves had a flow, so a missing field reads as offered: only an
 * explicit `false` withholds it. */
export function offersLoginFlow(providers: readonly ProviderInfo[], id: string): boolean {
  const served = servedProvider(providers, id);
  // The generated type has the field required, but a released daemon's snapshot does not carry it.
  return served !== undefined && (served.capabilities.login_flow as boolean | undefined) !== false;
}

/** Whether the agent of provider `id` takes the path of an image file pasted into its terminal as an attached image
 * (`capabilities.message_images`), which is what pasting or dropping an image onto a session's terminal relies on.
 * Where it does not, an image is pasted as into any terminal and a dropped image file is attached as any file. A
 * provider the daemon does not serve takes none. A daemon that predates the capability never sends it, and every
 * provider it serves took images, so a missing field reads as taken: only an explicit `false` withholds it. */
export function takesMessageImages(providers: readonly ProviderInfo[], id: string): boolean {
  const served = servedProvider(providers, id);
  // The generated type has the field required, but a released daemon's snapshot does not carry it.
  return served !== undefined && (served.capabilities.message_images as boolean | undefined) !== false;
}

/** The provider's usual config directory as the account form shows it, under the home directory (`~/.claude`). */
export function providerConfigDir(providers: readonly ProviderInfo[], id: string): string | undefined {
  const served = servedProvider(providers, id);
  return served && `~/${served.default_config_dir}`;
}

/** The configuration directory of an agent's default account as the window names it: the one the daemon found
 * (`default_account_dir` of the agent's availability), else the provider's usual one under the home directory; empty
 * where neither is known. */
export function defaultAccountConfigDir(agentAvailability: ReadonlyMap<string, AgentAvailability>, providers: readonly ProviderInfo[], id: string): string {
  return agentAvailability.get(id)?.default_account_dir ?? providerConfigDir(providers, id) ?? "";
}

/** The names at a project's top level that point at some provider (`CLAUDE.md`, `.claude`), as the daemon serves them,
 * in the registry's order and each once, though several providers read the same generic name. */
export function detectionMarkers(providers: readonly ProviderInfo[]): string[] {
  return [...new Set(providers.flatMap((provider) => provider.detection_markers))];
}

/** The key of the provider `id`'s message `name` (`provider.<id>.<name>`), or `undefined` for a provider whose
 * presentation module has none. */
export function providerMessageKey(id: string, name: ProviderMessageName): MessageKey | undefined {
  const key = `provider.${id}.${name}`;
  return isMessageKey(key) ? key : undefined;
}

/** The catalog keys of provider `id`'s wording of the question `question` its login asks: the question itself
 * (`provider.<id>.loginQuestion.<question>`) and its two answers (`.yes` and `.no`). `undefined` unless the catalogs
 * hold all three (a provider with no presentation module, a question a newer daemon asks): a question is offered for an
 * answer only where the window can say the whole of it. */
export function loginQuestionKeys(id: string, question: string): { question: MessageKey; yes: PlainMessageKey; no: PlainMessageKey } | undefined {
  const key = `provider.${id}.loginQuestion.${question}`;
  const [yes, no] = [`${key}.yes`, `${key}.no`];
  return isMessageKey(key) && isMessageKey(yes) && isMessageKey(no) ? { question: key, yes: yes as PlainMessageKey, no: no as PlainMessageKey } : undefined;
}

export function secretLoginQuestionKeys(id: string, question: string): { question: MessageKey; field: PlainMessageKey; submit: PlainMessageKey } | undefined {
  const key = `provider.${id}.loginQuestion.${question}`;
  const [field, submit] = [`${key}.field`, `${key}.submit`];
  return isMessageKey(key) && isMessageKey(field) && isMessageKey(submit) ? { question: key, field: field as PlainMessageKey, submit: submit as PlainMessageKey } : undefined;
}

/** The label of the action that opens provider `id`'s sessions in its own desktop application
 * (`provider.<id>.openInApp`), or `undefined` for a provider that declares no such application
 * (`capabilities.native_app_scheme`) or whose presentation module has no copy for it. */
export function nativeAppLabel(t: Translate, providers: readonly ProviderInfo[], id: string): string | undefined {
  if (!servedProvider(providers, id)?.capabilities.native_app_scheme) return undefined;
  const key = providerMessageKey(id, "openInApp");
  return key && t(key as PlainMessageKey);
}

/** An id in words, for a setting or choice a provider's module has no copy for: its separators as spaces, capitalised
 * (`bypass_permissions` as "Bypass permissions"). */
export function idWords(id: string): string {
  const spaced = id.replace(/[_-]+/g, " ").trim();
  return spaced.charAt(0).toUpperCase() + spaced.slice(1);
}

/** The catalog key of the message `name` of the setting `setting` of provider `id`
 * (`provider.<id>.setting.<setting>.<name>`), where the name is `heading`, `helpLabel`, `label`, `description` /
 * `helpTiming` / `helpCustom`, `inherit` / `inheritDescription` / `inheritSummary`, `saveFailedTitle`,
 * `choice.<value>` / `.description` / `.summary` or
 * `confirm.<value>.title` / `.consequence` / `.message`; `undefined` for a provider, setting or name the catalogs have
 * no message for (a provider with no presentation module). */
export function settingMessageKey(id: string, setting: string, name: string): MessageKey | undefined {
  const key = `provider.${id}.setting.${setting}.${name}`;
  return isMessageKey(key) ? key : undefined;
}
