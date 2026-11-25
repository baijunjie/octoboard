import React from "react";

import { AgentIcon } from "../components/icons/AgentIcon";
import { FadedScroller, useFadeRegion } from "../components/scroll/scrollFade";
import { minutesFromNow } from "../fixtures/builders";
import { quotaOf, quotaWindow } from "../fixtures/quotas";
import type { StatusItemIcon } from "../platform";
import type { StatusItemMenuDraft } from "../statusItem/statusItemMenu";
import { withIcons } from "../statusItem/statusItemIcons";
import { agentIconKey } from "../statusItem/statusItemIconKeys";
import type { Appearance } from "../preferences/cssColour";
import { BUILT_IN, providerColour } from "../providers/registry";
import { AccountUsageBar } from "../quota/AccountUsageBar";
import { QuotaMeter } from "../quota/QuotaMeter";

/** The built-in providers in registry order, each with its English display name from its own module. */
const PROVIDERS = BUILT_IN.map((module) => ({
  id: module.id,
  name: (module.messages.en as Readonly<Record<string, string>>)[`provider.${module.id}.label`] ?? module.id,
}));

const SESSION = quotaWindow("session", 40, 90);
const WEEKLY = quotaWindow("weekly", 65, 3 * 24 * 60);
/** The fixtures' clock, so the bars count down from the same instant the windows' reset times were set from. */
const NOW = minutesFromNow(0);

const APPEARANCES: readonly Appearance[] = ["light", "dark"];

/** What the menu's picture of each provider's mark is, per appearance, from the app's own `withIcons`. */
async function menuIcons(appearance: Appearance): Promise<Record<string, StatusItemIcon>> {
  const draft: StatusItemMenuDraft = {
    sections: PROVIDERS.map(({ id }) => ({ heading: id, icon: agentIconKey(id), items: [] })),
    submenus: [],
    open: "",
    quit: "",
    disconnect: "",
    app: { this_computer: "", open_in_browser: "" },
    attention: false,
    keep_awake_label: "",
  };
  return (await withIcons(draft, appearance)).icons;
}

// Drawn before the sheet renders, so the page is settled when it is first shown.
const ICONS = Object.fromEntries(await Promise.all(APPEARANCES.map(async (appearance) => [appearance, await menuIcons(appearance)] as const)));

/** The two grounds a menu bar picture is shown on; the system tints a template picture to the menu's text colour. */
const MENUS: Record<Appearance, { background: string; ink: string }> = {
  light: { background: "#ececec", ink: "#000" },
  dark: { background: "#2b2b2b", ink: "#fff" },
};

function MenuPicture({ id, appearance }: { id: string; appearance: Appearance }): React.ReactElement {
  const icon = ICONS[appearance][agentIconKey(id)];
  const { background, ink } = MENUS[appearance];
  const url = icon && `url(data:image/png;base64,${icon.png})`;
  return (
    <div data-menu-picture={`${id}-${appearance}`} className="flex h-8 items-center gap-2 rounded-md px-3 text-xs" style={{ background, color: ink }}>
      {icon?.template ? (
        <span aria-hidden className="block size-[18px]" style={{ backgroundColor: ink, maskImage: url, maskSize: "contain", WebkitMaskImage: url, WebkitMaskSize: "contain" }} />
      ) : icon ? (
        <img alt="" src={`data:image/png;base64,${icon.png}`} className="size-[18px]" />
      ) : (
        <span className="size-[18px]">?</span>
      )}
      <span>{appearance === "light" ? "Light menu" : "Dark menu"}{icon?.template ? ", template" : ", own colour"}</span>
    </div>
  );
}

/** One provider's colour on the compact bar and the meter, on one surface. */
function Bars({ id, edge }: { id: string; edge: boolean }): React.ReactElement {
  const quota = quotaOf(id, null, null, [SESSION, WEEKLY]);
  return (
    <div className="flex min-w-0 flex-col gap-3">
      <div className="flex items-center gap-4">
        <AccountUsageBar agent={id} quota={quota} now={NOW} edge={edge} />
        <AccountUsageBar agent={id} quota={quota} now={NOW} edge={edge} className="w-24" />
      </div>
      <div className="w-56">
        <QuotaMeter agent={id} naming={{ windowNaming: undefined, windows: [SESSION] }} window={SESSION} />
      </div>
    </div>
  );
}

function Surface({ name, className, children }: { name: string; className: string; children: React.ReactNode }): React.ReactElement {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <span className="text-xs text-app-muted">{name}</span>
      <div className={`min-w-0 rounded-xl p-3 ${className}`}>{children}</div>
    </div>
  );
}

function Row({ id, name }: { id: string; name: string }): React.ReactElement {
  const colour = providerColour(id);
  return (
    <section data-provider-row={id} aria-label={name} className="flex flex-col gap-3 rounded-lg bg-surface p-4 shadow-(--panel-shadow)">
      <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
        <h2 className="font-semibold">{name}</h2>
        <code className="font-mono text-xs text-app-muted">{id}</code>
        <span className="font-mono text-xs text-app-muted">{colour ? `colour light ${colour.light} / dark ${colour.dark}` : "no colour: bars in the text colour"}</span>
      </div>
      <div className="flex flex-wrap items-start gap-x-8 gap-y-4">
        <div className="flex flex-col gap-1">
          <span className="text-xs text-app-muted">Mark, 16px and 20px</span>
          <div className="flex items-center gap-4 py-2">
            <AgentIcon agent={id} className="size-4" />
            <AgentIcon agent={id} className="size-5" />
          </div>
        </div>
        <Surface name="Settings and dialog surface (--control-fill track)" className="floating-surface control-fills bg-overlay shadow-overlay">
          <Bars id={id} edge={false} />
        </Surface>
        <Surface name="Popover and menu surface (--default track, edged)" className="floating-surface bg-overlay shadow-overlay">
          <Bars id={id} edge />
        </Surface>
        <div className="flex flex-col gap-1">
          <span className="text-xs text-app-muted">Menu picture (statusItemIcons)</span>
          <div className="flex flex-col gap-2">
            {APPEARANCES.map((appearance) => <MenuPicture key={appearance} id={id} appearance={appearance} />)}
          </div>
        </div>
      </div>
    </section>
  );
}

/** A settled sheet of every built-in provider's mark and colour, drawn by the real components. */
export default function ProviderMarksSheet(): React.ReactElement {
  const region = useFadeRegion({ label: "Provider marks and colours" });
  return (
    <main className={`flex h-full min-h-0 flex-col ${region.insetRing}`}>
      <FadedScroller region={region} className="min-h-0 flex-1 p-6">
        <div className="mx-auto flex max-w-6xl flex-col gap-4">
          <h1 className="text-lg font-semibold">Provider marks and colours</h1>
          {PROVIDERS.map(({ id, name }) => <Row key={id} id={id} name={name} />)}
        </div>
      </FadedScroller>
    </main>
  );
}
