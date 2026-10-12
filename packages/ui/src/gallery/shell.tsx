// The gallery's own page: the scenario list and the controls around the iframe that renders the
// chosen scenario (`frame.tsx`). A dev tool, so plain, English and outside the app's i18n and theme.
// What is chosen lives in the URL, so a link reproduces a view.
import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";

import { LANGUAGES, LANGUAGE_NAMES, isLanguage, type Language } from "../i18n/languages";
import { GROUPS, SCENARIOS } from "./fixtures";
import { DEFAULT_WIDTH, type Scenario } from "./scenario";

const WIDTHS = [
  { label: "Wide", width: DEFAULT_WIDTH },
  { label: "Docked", width: 1148 },
  { label: "Narrow", width: 800 },
  { label: "Phone", width: 420 },
];

const MIN_WIDTH = 320;

interface View {
  scenario: string;
  theme: "light" | "dark";
  language: Language;
  /** Set by the user; otherwise the scenario's own width applies. */
  width?: number;
}

function readView(): View {
  const params = new URLSearchParams(window.location.search);
  const language = params.get("lang");
  const width = Number(params.get("width"));
  return {
    scenario: params.get("scenario") ?? SCENARIOS[0].id,
    theme: params.get("theme") === "dark" ? "dark" : "light",
    language: isLanguage(language) ? language : "en",
    width: Number.isFinite(width) && width > 0 ? width : undefined,
  };
}

function viewQuery(view: View): string {
  const params = new URLSearchParams({ scenario: view.scenario, theme: view.theme, lang: view.language });
  if (view.width) params.set("width", String(view.width));
  return `?${params}`;
}

/** The width in pixels as typed: applied on Enter or blur, so clearing the field to type another
 * number does not snap back, and a value under the smallest usable window is ignored. */
function WidthBox({ width, onCommit }: { width: number; onCommit: (width: number) => void }): React.ReactElement {
  const [text, setText] = useState(String(width));
  useEffect(() => setText(String(width)), [width]);
  const commit = () => {
    const typed = Number(text);
    if (typed >= MIN_WIDTH) onCommit(typed);
    else setText(String(width));
  };
  return (
    <input
      type="number"
      aria-label="Width in pixels"
      min={MIN_WIDTH}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}

/** The view that picking `scenario` reaches: a scenario carrying its own width opens at it, dropping
 * whatever width was chosen for an earlier one. A width is otherwise sticky across the list, and a
 * scenario whose steps open the drawer that exists only below the breakpoint cannot run in a wide
 * window — the press finds nothing and the page paints its failure overlay. */
function viewOf(view: View, scenario: Scenario): View {
  return { ...view, scenario: scenario.id, width: scenario.width ? undefined : view.width };
}

function Gallery(): React.ReactElement {
  const [view, setView] = useState(readView);
  const [run, setRun] = useState(0);
  const scenario = SCENARIOS.find((s) => s.id === view.scenario) ?? SCENARIOS[0];
  const width = view.width ?? scenario.width ?? DEFAULT_WIDTH;
  const update = (patch: Partial<View>) => setView((current) => ({ ...current, ...patch }));

  useEffect(() => {
    window.history.replaceState(null, "", viewQuery(view));
  }, [view]);

  const frameQuery = `?${new URLSearchParams({ scenario: scenario.id, theme: view.theme, lang: view.language })}`;

  return (
    <>
      <nav aria-label="Scenarios">
        {GROUPS.map(({ title, icon: Icon }, index) => (
          <section key={title} aria-labelledby={`group-${index}`}>
            <h2 id={`group-${index}`}>
              <Icon aria-hidden size={14} />
              {title}
            </h2>
            {SCENARIOS.filter((s) => s.group === title).map((s) => (
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
          </section>
        ))}
      </nav>
      <main>
        <header>
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
            <WidthBox width={width} onCommit={(w) => update({ width: w })} />
          </div>
          <a href={`gallery-frame.html${frameQuery}`} target="_blank" rel="noreferrer">
            Open alone
          </a>
          <button onClick={() => setRun((n) => n + 1)}>Reload</button>
          {scenario.description && <p className="description">{scenario.description}</p>}
        </header>
        <div className="stage">
          <iframe
            key={`${run}${frameQuery}`}
            title={scenario.title}
            src={`gallery-frame.html${frameQuery}`}
            style={{ width }}
          />
        </div>
      </main>
    </>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<Gallery />);
