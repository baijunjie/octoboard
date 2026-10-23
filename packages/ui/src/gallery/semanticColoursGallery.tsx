import { Alert, ToggleButton, ToggleButtonGroup } from "@heroui/react";
import { CircleAlert, Info, Sparkles } from "lucide-react";
import React, { useEffect, useRef } from "react";
import { setInteractionModality } from "react-aria";

import { ActivityMarker } from "../components/ActivityMarker";
import { Button } from "../components/Button";
import { ErrorAlert } from "../components/ErrorAlert";
import { ExternalLink } from "../components/ExternalLink";
import { FailureMark } from "../components/FailureMark";
import { InlineReference } from "../components/InlineReference";
import { StatusBadge, type StatusBadgeColor } from "../components/StatusBadge";
import { TintedAlert } from "../components/TintedAlert";
import { FadedScroller, useFadeRegion } from "../components/scrollFade";

/** One semantic meaning and how the application draws it. The role tokens follow docs/ui-accessibility.md's "One
 * definition per semantic colour"; the purple help colour has no HeroUI token, so its icon colour is its text tint
 * and it has no solid text colour or Alert status. */
interface Meaning {
  color: StatusBadgeColor;
  label: string;
  /** Solid fill and the text on it; absent where no token exists. */
  solid: { fill: string; text?: string };
  soft: { token: string; alias: string };
  icon: string;
  badge: string;
  sample: string;
}

const MEANINGS: Meaning[] = [
  { color: "default", label: "Neutral (default)", solid: { fill: "--default", text: "--default-foreground" }, soft: { token: "--tint-default-bg", alias: "the project's neutral tint" }, icon: "--default-soft-foreground", badge: "Not detected", sample: "Claude Code" },
  { color: "accent", label: "Accent", solid: { fill: "--accent", text: "--accent-foreground" }, soft: { token: "--accent-soft", alias: "--tint-accent-bg" }, icon: "--accent-soft-foreground", badge: "Renamed", sample: "Working" },
  { color: "success", label: "Success", solid: { fill: "--success", text: "--success-foreground" }, soft: { token: "--success-soft", alias: "--tint-success-bg" }, icon: "--success-soft-foreground", badge: "Logged in", sample: "Idle" },
  { color: "warning", label: "Warning", solid: { fill: "--warning", text: "--warning-foreground" }, soft: { token: "--warning-soft", alias: "--tint-warning-bg" }, icon: "--warning-soft-foreground", badge: "Not logged in", sample: "Waiting for you" },
  { color: "danger", label: "Danger", solid: { fill: "--danger", text: "--danger-foreground" }, soft: { token: "--danger-soft", alias: "--tint-danger-bg" }, icon: "--danger-soft-foreground", badge: "Deleted", sample: "Failed" },
  { color: "help", label: "Help (purple)", solid: { fill: "--help" }, soft: { token: "--tint-help-bg", alias: "mixed from --help" }, icon: "--tint-help-fg", badge: "Agent", sample: "Shared" },
];

type AlertColor = "default" | "accent" | "success" | "warning" | "danger";

const tint = (color: StatusBadgeColor, role: "bg" | "fg" | "edge"): string => `--tint-${color}-${role}`;
const use = (token: string): string => `var(${token})`;

function Swatch({ role, tokens, children, style }: { role: string; tokens: string[]; children: React.ReactNode; style: React.CSSProperties }): React.ReactElement {
  return (
    <figure className="flex w-36 min-w-0 flex-col gap-1">
      <div className="flex h-12 items-center justify-center rounded-lg text-sm font-semibold" style={style}>{children}</div>
      <figcaption className="text-xs">
        <span className="block text-app-muted">{role}</span>
        {tokens.map((token) => <code key={token} className="block font-mono [overflow-wrap:anywhere]">{token}</code>)}
      </figcaption>
    </figure>
  );
}

/** Every role of one meaning as a labelled swatch, drawn on a white surface so the translucent fills read as they do
 * on a card. */
function Swatches({ meaning }: { meaning: Meaning }): React.ReactElement {
  const { color, solid, soft, icon } = meaning;
  return (
    <div data-semantic-swatches={color} className="flex flex-wrap gap-4">
      <Swatch role="Solid and the text on it" tokens={solid.text ? [solid.fill, solid.text] : [solid.fill, "no foreground token"]} style={{ background: use(solid.fill), color: solid.text ? use(solid.text) : "white" }}>Aa</Swatch>
      <Swatch role="Soft fill" tokens={[soft.token, `(${soft.alias})`]} style={{ background: use(soft.token), color: use("--foreground") }}>Aa</Swatch>
      <Swatch role="Icon or glyph" tokens={[icon]} style={{}}>
        <CircleAlert aria-hidden="true" className="size-6" style={{ color: use(icon) }} />
        <Info aria-hidden="true" className="ms-2 size-6" style={{ color: use(icon) }} />
      </Swatch>
      <Swatch role="Text in the colour" tokens={[tint(color, "fg")]} style={{ color: use(tint(color, "fg")) }}>Text Aa</Swatch>
      <Swatch role="Hairline edge" tokens={[tint(color, "edge")]} style={{ border: `1px solid ${use(tint(color, "edge"))}`, color: use("--foreground") }}>Aa</Swatch>
    </div>
  );
}

/** What each meaning has beyond a swatch: a mark where the application draws one. */
function Mark({ color }: { color: StatusBadgeColor }): React.ReactElement | null {
  switch (color) {
    case "accent":
      return <ActivityMarker activity="working" />;
    case "success":
      return <ActivityMarker activity="running" />;
    case "danger":
      return <FailureMark label="Quota unavailable">Claude Code took too long to report its quota.</FailureMark>;
    case "warning":
      return <FailureMark status="warning" label="Out of date: last read 12 minutes ago">Claude Code took too long to report its quota.</FailureMark>;
    case "default":
      return <Info aria-hidden="true" className="size-4" style={{ color: use("--default-soft-foreground") }} />;
    case "help":
      return <Sparkles aria-hidden="true" className="size-4" style={{ color: use("--tint-help-fg") }} />;
  }
}

function SampleAlert({ meaning }: { meaning: Meaning }): React.ReactElement {
  const { color } = meaning;
  if (color === "help") return <p className="text-xs text-app-muted">An Alert has no help status.</p>;
  const description = <>Could not read <InlineReference monospace wrap>AGENTS.md</InlineReference> in this project.</>;
  if (color === "danger" || color === "warning") {
    return <ErrorAlert status={color} title={color === "danger" ? "Could not save" : "Settings are out of date"}>{description}</ErrorAlert>;
  }
  const status: AlertColor = color;
  return (
    <TintedAlert status={status}>
      <Alert.Indicator aria-hidden="true" />
      <Alert.Content>
        <Alert.Title>{status === "default" ? "Nothing to show yet" : status === "accent" ? "A new version is ready" : "Settings saved"}</Alert.Title>
        <Alert.Description>{description}</Alert.Description>
      </Alert.Content>
    </TintedAlert>
  );
}

/** The real components drawn in one meaning, on whatever surface they are placed. */
function Specimen({ meaning, chip, alert, rowClassName = "" }: { meaning: Meaning; chip: boolean; alert: boolean; rowClassName?: string }): React.ReactElement {
  const { color, badge, label, sample } = meaning;
  return (
    <div data-semantic-specimen={color} className="flex min-w-0 flex-col gap-1.5">
      <div className={`flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 rounded-lg text-sm ${rowClassName}`}>
        <span className="w-24 shrink-0 text-xs text-app-muted">{label}</span>
        <span style={{ color: use(tint(color, "fg")) }}>{sample}</span>
        <StatusBadge color={color} variant="soft" separated={false}>{badge}</StatusBadge>
        <StatusBadge color={color} variant="bordered" separated={false}>{badge}</StatusBadge>
        <Mark color={color} />
      </div>
      {chip && (
        <p className="text-sm">
          Open <InlineReference color={color} monospace>AGENTS.md</InlineReference> in <InlineReference color={color}>Main</InlineReference> to continue.
        </p>
      )}
      {alert && <SampleAlert meaning={meaning} />}
    </div>
  );
}

function Surface({ name, note, className, children }: { name: string; note: string; className: string; children: React.ReactNode }): React.ReactElement {
  return (
    <section data-semantic-surface={name} aria-label={name} className="flex min-w-0 flex-col gap-2">
      <h3 className="text-sm font-medium">{name} <span className="font-normal text-app-muted">{note}</span></h3>
      <div className={`flex min-w-0 flex-col gap-3 p-3 ${className}`}>{children}</div>
    </section>
  );
}

/** A focused button, as a keyboard user lands on it: React Aria draws the ring from the input modality, so it is set
 * before focus moves. */
function FocusedButton(): React.ReactElement {
  const host = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    setInteractionModality("keyboard");
    host.current?.querySelector("button")?.focus();
  }, []);
  return (
    <span ref={host}>
      <Button variant="secondary">Focused</Button>
    </span>
  );
}

/** One settled page of the application's semantic colours: each meaning's tokens, the real components drawn in it
 * on every surface they sit on, and the roles that exist once (link, focus, selection, secondary text, counts). */
export function SemanticColoursGallery(): React.ReactElement {
  const region = useFadeRegion({ label: "Semantic colours" });
  return (
    <main className={`flex h-full min-h-0 flex-col ${region.focused ? "ring-2 ring-focus ring-inset" : ""}`}>
      <FadedScroller region={region} className="min-h-0 flex-1 p-6">
        <div className="mx-auto flex max-w-6xl flex-col gap-8">
          <h1 className="text-lg font-semibold">Semantic colours: tokens, components and surfaces</h1>
          <section className="flex flex-col gap-4" aria-labelledby="semantic-tokens-title">
            <h2 id="semantic-tokens-title" className="font-semibold">Tokens by meaning</h2>
            <div className="flex flex-col gap-5 rounded-lg bg-surface p-4 shadow-(--panel-shadow)">
              {MEANINGS.map((meaning) => (
                <div key={meaning.color} className="flex flex-col gap-2">
                  <h3 className="text-sm font-medium">{meaning.label}</h3>
                  <Swatches meaning={meaning} />
                </div>
              ))}
            </div>
          </section>
          <section className="flex flex-col gap-4" aria-labelledby="semantic-once-title">
            <h2 id="semantic-once-title" className="font-semibold">Roles that exist once</h2>
            <div data-semantic-once="" className="flex flex-col gap-4 rounded-lg bg-surface p-4 text-sm shadow-(--panel-shadow)">
              <p>Link in a sentence: <ExternalLink href="https://example.com/docs">layout notes</ExternalLink> (<code className="font-mono text-xs text-app-muted">--tint-accent-fg</code>).</p>
              <div className="flex flex-wrap items-center gap-3">
                <FocusedButton />
                <span className="text-xs text-app-muted">Focus ring, <code className="font-mono">--focus</code>, on a keyboard-focused button.</span>
              </div>
              <div className="flex flex-wrap items-center gap-3">
                <ToggleButtonGroup aria-label="Browser mode" selectionMode="single" disallowEmptySelection defaultSelectedKeys={["files"]}>
                  <ToggleButton id="files" size="sm">Files</ToggleButton>
                  <ToggleButton id="changes" size="sm">Changes</ToggleButton>
                </ToggleButtonGroup>
                <span className="text-xs text-app-muted">Selected toggle segment, HeroUI's stock selected state.</span>
              </div>
              <div className="flex flex-wrap items-center gap-3">
                <Button variant="primary" size="sm">Save</Button>
                <Button variant="danger" size="sm">Delete</Button>
                <span className="text-xs text-app-muted">Solid fills with <code className="font-mono">--accent-foreground</code> and <code className="font-mono">--danger-foreground</code>.</span>
              </div>
              <p>Line counts: <span className="text-(--tint-success-fg)">+12</span> <span className="text-(--tint-danger-fg)">−3</span> (<code className="font-mono text-xs text-app-muted">--tint-success-fg</code>, <code className="font-mono text-xs text-app-muted">--tint-danger-fg</code>).</p>
              <p className="text-app-muted">Secondary text, <code className="font-mono text-xs">--app-muted</code>: the date, the path and the description under a name.</p>
            </div>
          </section>
          <section className="flex flex-col gap-4" aria-labelledby="semantic-surfaces-title">
            <h2 id="semantic-surfaces-title" className="font-semibold">Components on each surface</h2>
            <div className="grid items-start gap-6 lg:grid-cols-2">
              <Surface name="Card" note="--surface" className="rounded-lg bg-surface shadow-(--panel-shadow)">
                {MEANINGS.map((meaning) => <Specimen key={meaning.color} meaning={meaning} chip alert />)}
              </Surface>
              <Surface name="Dialog" note="--overlay with the control fills" className="control-fills rounded-2xl bg-overlay shadow-overlay">
                {MEANINGS.map((meaning) => <Specimen key={meaning.color} meaning={meaning} chip alert />)}
              </Surface>
              <Surface name="Panel" note="--panel" className="rounded-lg border border-separator bg-panel">
                {MEANINGS.map((meaning) => <Specimen key={meaning.color} meaning={meaning} chip alert />)}
              </Surface>
              <Surface name="Tooltip" note="--overlay with the control fills" className="control-fills rounded-xl bg-overlay shadow-overlay">
                {MEANINGS.map((meaning) => <Specimen key={meaning.color} meaning={meaning} chip alert={false} />)}
              </Surface>
              <Surface name="Sidebar row, resting" note="--panel-sidebar" className="sidebar-fills rounded-lg bg-panel-sidebar">
                {MEANINGS.map((meaning) => <Specimen key={meaning.color} meaning={meaning} chip={false} alert={false} rowClassName="px-2 py-1" />)}
              </Surface>
              <Surface name="Sidebar row, selected" note="--panel-selected over --panel-sidebar" className="sidebar-fills rounded-lg bg-panel-sidebar">
                {MEANINGS.map((meaning) => <Specimen key={meaning.color} meaning={meaning} chip={false} alert={false} rowClassName="bg-panel-selected px-2 py-1" />)}
              </Surface>
            </div>
          </section>
        </div>
      </FadedScroller>
    </main>
  );
}
