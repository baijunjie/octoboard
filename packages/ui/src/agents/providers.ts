import { isMessageKey, type MessageKey, type PlainMessageKey, type Translate } from "../i18n/catalog";
import type { Agent, ModelDisplay, ProviderInfo } from "../protocol";
import type { ProviderMessageName } from "../providers/types";

/** The UI's records of the codes the providers declare (`PROVIDER_CODES`, for the code registry) and their plan names
 * (`providerPlanName`, for `accounts/quotaFormat.ts`). Re-exported here because `errors/` and `accounts/` may import
 * `agents/` but not `providers/`, which holds them. */
export { PROVIDER_CODES, providerPlanName } from "../providers/registry";

/** The name of provider `id` in full ("Claude Code"). A provider with a presentation module is named by the module's
 * message; any other by the label the daemon serves of it in `providers`, else by its id (a provider the daemon does
 * not serve). `providers` is required so that no caller can leave the served label out. */
export function providerLabel(t: Translate, providers: readonly ProviderInfo[], id: string): string {
  const key = `provider.${id}.label`;
  return isMessageKey(key) ? t(key as PlainMessageKey) : (servedProvider(providers, id)?.label ?? id);
}

/** The shorter name of provider `id` ("Claude" for "Claude Code"), as the menu bar icon's menu uses it. A provider
 * without a presentation module is named by the short name the daemon serves of it in `providers`, else by its id.
 * Brand names, so not translated. */
export function providerShortName(t: Translate, providers: readonly ProviderInfo[], id: string): string {
  const key = `provider.${id}.shortName`;
  return isMessageKey(key) ? t(key as PlainMessageKey) : (servedProvider(providers, id)?.short_name ?? id);
}

/** Whether `id` is the id of a provider the daemon serves. */
export function isKnownAgent(providers: readonly ProviderInfo[], id: string): boolean {
  return servedProvider(providers, id) !== undefined;
}

/** The registered provider `id`, `undefined` while the daemon has not served the providers or serves none by that id. */
export function servedProvider(providers: readonly ProviderInfo[], id: string): ProviderInfo | undefined {
  return providers.find((provider) => provider.id === id);
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

/** The provider's usual config directory as the account form shows it, under the home directory (`~/.claude`). */
export function providerConfigDir(providers: readonly ProviderInfo[], id: string): string | undefined {
  const served = servedProvider(providers, id);
  return served && `~/${served.default_config_dir}`;
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
