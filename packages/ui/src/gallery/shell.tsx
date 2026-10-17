// The gallery's own page: the scenario list and the controls around the iframe that renders the
// chosen scenario (`frame.tsx`). A dev tool, so plain, English and outside the app's i18n and theme.
// What is chosen lives in the URL, so a link reproduces a view.
import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";

import { LANGUAGES, LANGUAGE_NAMES, isLanguage, type Language } from "../i18n/languages";
import { DEFAULT_SCENARIO, GROUPS, SCENARIOS } from "./fixtures";
import { CATEGORIES } from "./fixtures/groups";
import { DEFAULT_WIDTH, MIN_HEIGHT, type Scenario } from "./scenario";

const WIDTHS = [
  { label: "Wide", width: DEFAULT_WIDTH },
  { label: "Docked", width: 1168 },
  { label: "Narrow", width: 800 },
  { label: "Phone", width: 420 },
];

/** The window heights offered besides fitting the space the page has: one short enough that a
 * dialog's body outgrows it, which is what a laptop screen does to the longer ones. */
const HEIGHTS = [{ label: "Short", height: 480 }];

const MIN_WIDTH = 320;

interface View {
  scenario: string;
  theme: "light" | "dark";
  language: Language;
  /** Set by the user; otherwise the scenario's own width applies. */
  width?: number;
  /** Likewise; otherwise the scenario's own, and otherwise the window fits the page. */
  height?: number;
  /** The shortcut modifier is held down, as the page's viewing condition, whatever the scenario. */
  held: boolean;
}

function readView(): View {
  const params = new URLSearchParams(window.location.search);
  const language = params.get("lang");
  const width = Number(params.get("width"));
  const height = Number(params.get("height"));
  return {
    scenario: params.get("scenario") ?? DEFAULT_SCENARIO.id,
    theme: params.get("theme") === "dark" ? "dark" : "light",
    language: isLanguage(language) ? language : "en",
    width: Number.isFinite(width) && width > 0 ? width : undefined,
    height: Number.isFinite(height) && height > 0 ? height : undefined,
    held: params.get("held") === "1",
  };
}

/** The parameters the shell page and the frame both take from a view: its scenario, theme and
 * language, and the shortcut modifier while held. */
function sharedParams(view: View): URLSearchParams {
  const params = new URLSearchParams({ scenario: view.scenario, theme: view.theme, lang: view.language });
  if (view.held) params.set("held", "1");
  return params;
}

function viewQuery(view: View): string {
  const params = sharedParams(view);
  if (view.width) params.set("width", String(view.width));
  if (view.height) params.set("height", String(view.height));
  return `?${params}`;
}

/** A size in pixels as typed: applied on Enter or blur, so clearing the field to type another
 * number does not snap back, and a value under the smallest usable window is ignored. With
 * `emptyMeans` an empty field is a size too, the one that text says. */
function SizeBox({
  label,
  size,
  min,
  emptyMeans,
  onCommit,
}: {
  label: string;
  size: number | undefined;
  min: number;
  emptyMeans?: string;
  onCommit: (size: number | undefined) => void;
}): React.ReactElement {
  const text0 = size === undefined ? "" : String(size);
  const [text, setText] = useState(text0);
  useEffect(() => setText(text0), [text0]);
  const commit = () => {
    const typed = Number(text);
    if (text.trim() === "" && emptyMeans) onCommit(undefined);
    else if (typed >= min) onCommit(typed);
    else setText(text0);
  };
  return (
    <input
      type="number"
      aria-label={label}
      placeholder={emptyMeans}
      min={min}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}

/** The view that picking `scenario` reaches: a scenario carrying its own width or height opens at it,
 * dropping whatever was chosen for an earlier one. Each is otherwise sticky across the list, and a
 * scenario whose steps open the drawer that exists only below the breakpoint cannot run in a wide
 * window — the press finds nothing and the page paints its failure overlay. */
function viewOf(view: View, scenario: Scenario): View {
  return {
    ...view,
    scenario: scenario.id,
    width: scenario.width ? undefined : view.width,
    height: scenario.height ? undefined : view.height,
  };
}

function Gallery(): React.ReactElement {
  const [view, setView] = useState(readView);
  const [run, setRun] = useState(0);
  const scenario = SCENARIOS.find((s) => s.id === view.scenario) ?? DEFAULT_SCENARIO;
  const category = CATEGORIES.find(({ id }) => id === scenario.category)!;
  const width = view.width ?? scenario.width ?? DEFAULT_WIDTH;
  const height = view.height ?? scenario.height;
  const update = (patch: Partial<View>) => setView((current) => ({ ...current, ...patch }));

  useEffect(() => {
    window.history.replaceState(null, "", viewQuery(view));
  }, [view]);

  const frameQuery = `?${sharedParams({ ...view, scenario: scenario.id })}`;

  return (
    <>
      <nav aria-label="Scenarios">
        {GROUPS.map(({ title, icon: Icon }, index) => (
          <section key={title} aria-labelledby={`group-${index}`}>
            <h2 id={`group-${index}`}>
              <Icon aria-hidden size={14} />
              {title}
            </h2>
            {CATEGORIES.map((category) => {
              const scenarios = SCENARIOS.filter((s) => s.group === title && s.category === category.id);
              return (
                <div key={category.id} className="category-section" data-category={category.id}>
                  <h3 title={category.description}>{category.label} <span>{scenarios.length}</span></h3>
                  {scenarios.length === 0 && <p className="no-scenarios">No previews in this section</p>}
                  {scenarios.map((s) => (
                    <a
                      key={s.id}
                      href={viewQuery(viewOf(view, s))}
                      aria-current={s.id === scenario.id ? "page" : undefined}
                      onClick={(e) => {
                        e.preventDefault();
                        setView((current) => viewOf(current, s));
                      }}
                    >
                      {s.title}
                    </a>
                  ))}
                </div>
              );
            })}
          </section>
        ))}
      </nav>
      <main>
        <div className="preview-context" data-category={scenario.category}>
          <div className="preview-location">{scenario.group} <span aria-hidden> / </span><strong className="category-badge">{category.label}</strong></div>
          <h1>{scenario.title}</h1>
          {scenario.description && <p className="description">{scenario.description}</p>}
        </div>
        <header aria-label="Preview controls">
          <div className="group">
            {(["light", "dark"] as const).map((theme) => (
              <button key={theme} aria-pressed={view.theme === theme} onClick={() => update({ theme })}>
                {theme}
              </button>
            ))}
          </div>
          <label className="group">
            Language
            <select value={view.language} onChange={(e) => update({ language: e.target.value as Language })}>
              {LANGUAGES.map((language) => (
                <option key={language} value={language}>
                  {language} · {LANGUAGE_NAMES[language]}
                </option>
              ))}
            </select>
          </label>
          <div className="group">
            Width
            {WIDTHS.map(({ label, width: w }) => (
              <button key={label} aria-pressed={width === w} onClick={() => update({ width: w })}>
                {label} {w}
              </button>
            ))}
            <SizeBox label="Width in pixels" size={width} min={MIN_WIDTH} onCommit={(w) => update({ width: w })} />
          </div>
          <div className="group">
            Height
            <button aria-pressed={height === undefined} onClick={() => update({ height: undefined })}>
              Fit
            </button>
            {HEIGHTS.map(({ label, height: h }) => (
              <button key={label} aria-pressed={height === h} onClick={() => update({ height: h })}>
                {label} {h}
              </button>
            ))}
            <SizeBox label="Height in pixels" size={height} min={MIN_HEIGHT} emptyMeans="fit" onCommit={(h) => update({ height: h })} />
          </div>
          <div className="group">
            Modifier (⌘ / Ctrl)
            {([false, true] as const).map((held) => (
              <button key={String(held)} aria-pressed={view.held === held} onClick={() => update({ held })}>
                {held ? "Held" : "Released"}
              </button>
            ))}
          </div>
          <a href={`gallery-frame.html${frameQuery}`} target="_blank" rel="noreferrer">
            Open alone
          </a>
          <button onClick={() => setRun((n) => n + 1)}>Reload</button>
        </header>
        <div className="stage">
          <iframe
            key={`${run}${frameQuery}`}
            title={scenario.title}
            src={`gallery-frame.html${frameQuery}`}
            style={{ width, height }}
          />
        </div>
      </main>
    </>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<Gallery />);
