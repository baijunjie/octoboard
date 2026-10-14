import { Alert } from "@heroui/react";
import React from "react";

import { CodeBlock, InlineCode } from "../components/InlineCode";
import { TintedAlert } from "../components/TintedAlert";
import type { CodeColor } from "../components/codeTone";
import { FadedScroller, useFadeRegion } from "../components/scrollFade";

const COLORS: CodeColor[] = ["default", "primary", "secondary", "success", "warning", "danger"];
const STATUSES = ["default", "accent", "success", "warning", "danger"] as const;

/** One settled comparison of the real code wrappers, both outside and inside alert contexts. */
export function CodeGallery(): React.ReactElement {
  const region = useFadeRegion({ label: "Code colors and alert tones" });
  return (
    <main className={`flex h-full min-h-0 flex-col ${region.focused ? "ring-2 ring-app-focus ring-inset" : ""}`}>
      <FadedScroller region={region} className="min-h-0 flex-1 p-6">
      <div className="mx-auto flex max-w-5xl flex-col gap-6">
        <h1 className="text-lg font-semibold">Code colors and alert tones</h1>
        <section className="flex flex-col gap-3" aria-labelledby="code-colors-title">
          <h2 id="code-colors-title" className="font-semibold">Six colors outside alerts</h2>
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {COLORS.map((color) => (
              <div key={color} data-code-sample={color} className="min-w-0 rounded-lg border border-separator p-3">
                <p className="mb-2 text-sm">{color}: <InlineCode color={color}>VITE_DAEMON_PORT</InlineCode></p>
                <CodeBlock color={color}>{"env -0\nOCTOBOARD_DAEMON_PORT=5174"}</CodeBlock>
              </div>
            ))}
          </div>
        </section>
        <section className="flex flex-col gap-3" aria-labelledby="alert-tones-title">
          <h2 id="alert-tones-title" className="font-semibold">Alert tone overrides each requested code color</h2>
          {STATUSES.map((status) => (
            <TintedAlert key={status} status={status} data-alert-sample={status}>
              <Alert.Indicator aria-hidden="true" />
              <Alert.Content className="min-w-0">
                <h3 className="mb-3 font-semibold">{status}</h3>
                <div className="flex flex-wrap gap-3">
                  {COLORS.map((color) => <span key={color} className="text-sm">{color}: <InlineCode color={color}>CLAUDE.md</InlineCode></span>)}
                </div>
                <CodeBlock>{"env -0 && printf '%s' 'OCTOBOARD-ENV-SNAPSHOT-END-12345678-1234-1234-1234-123456789abc'"}</CodeBlock>
              </Alert.Content>
            </TintedAlert>
          ))}
        </section>
        <section className="flex flex-col gap-3" aria-labelledby="nested-tones-title">
          <h2 id="nested-tones-title" className="font-semibold">Nearest alert wins; outside code keeps its own tone</h2>
          <TintedAlert status="warning" data-nested-sample="outer">
            <Alert.Content className="flex min-w-0 flex-col gap-3">
              <p>Outer warning: <InlineCode color="danger">AGENTS.md</InlineCode></p>
              <TintedAlert status="success" data-nested-sample="inner">
                <Alert.Content>Inner success: <InlineCode color="danger">.claude</InlineCode></Alert.Content>
              </TintedAlert>
              <p>Outer warning again: <InlineCode color="primary">.codex</InlineCode></p>
            </Alert.Content>
          </TintedAlert>
          <p>After both alerts: <InlineCode color="secondary">~/Projects</InlineCode></p>
        </section>
      </div>
      </FadedScroller>
    </main>
  );
}
