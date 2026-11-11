// The renderer of every kind of component a page can hold, in one map keyed by the kind's `type`. A kind the protocol
// adds to `Component` is a type error here until it has its entry, and a kind of a later milestone is one entry more.
import React from "react";

import type { Component } from "../../protocol";
import { ChartView } from "./ChartView";
import { HeadingLevelContext, useChildHeadingLevel } from "./headingLevel";
import { usePageForm } from "./PageFormContext";
import { PageMarkdown } from "./PageMarkdown";
import {
  CheckboxesFieldView, DateFieldView, DatetimeFieldView, NumberFieldView, RadioFieldView, SelectFieldView, SliderFieldView,
  TextareaFieldView, TextFieldView, TimeFieldView, ToggleFieldView,
} from "./fields";
import { ChoicesView, SubmitView } from "./submitters";
import { CalloutView, CodeView, DiffView, KeyValuesView, StatView } from "./content";
import { CopyableView } from "./copyable";
import { ColumnsView, DividerView, TabsView } from "./layout";
import { TableView } from "./PageTable";

export type ComponentType = Component["type"];

/** What the renderer of one kind is given: its component, typed as that kind. */
export interface ComponentProps<K extends ComponentType> {
  component: Extract<Component, { type: K }>;
}

/** Draws the components of `list` that are shown, one after the other. */
export function Components({ list }: { list: readonly Component[] }): React.ReactElement {
  return <>{list.map((component, index) => <ComponentView key={index} component={component} />)}</>;
}

function SectionView({ component }: ComponentProps<"section">): React.ReactElement {
  const { Tag, inner } = useChildHeadingLevel(component.heading !== undefined);
  return (
    <section className="flex flex-col gap-4">
      {component.heading !== undefined && <Tag className="-mb-2.5 text-base font-semibold align-match-parent" dir="auto">{component.heading}</Tag>}
      <HeadingLevelContext.Provider value={inner}>
        <Components list={component.children} />
      </HeadingLevelContext.Provider>
    </section>
  );
}

function MarkdownView({ component }: ComponentProps<"markdown">): React.ReactElement {
  return <PageMarkdown text={component.text} />;
}

export const COMPONENT_RENDERERS: { [K in ComponentType]: React.ComponentType<ComponentProps<K>> } = {
  section: SectionView,
  markdown: MarkdownView,
  text: TextFieldView,
  textarea: TextareaFieldView,
  number: NumberFieldView,
  date: DateFieldView,
  time: TimeFieldView,
  datetime: DatetimeFieldView,
  radio: RadioFieldView,
  select: SelectFieldView,
  checkboxes: CheckboxesFieldView,
  toggle: ToggleFieldView,
  slider: SliderFieldView,
  submit: SubmitView,
  choices: ChoicesView,
  columns: ColumnsView,
  tabs: TabsView,
  divider: DividerView,
  callout: CalloutView,
  key_values: KeyValuesView,
  stat: StatView,
  code: CodeView,
  diff: DiffView,
  copyable: CopyableView,
  table: TableView,
  chart: ChartView,
};

/** One component through its kind's renderer, or nothing while its `visible_when` is not met. */
export function ComponentView({ component }: { component: Component }): React.ReactElement | null {
  const form = usePageForm();
  if (!form.isShown(component)) return null;
  const Renderer = COMPONENT_RENDERERS[component.type] as React.ComponentType<{ component: Component }>;
  return <Renderer component={component} />;
}
