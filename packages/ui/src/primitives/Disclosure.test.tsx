// @vitest-environment jsdom
import { expect, it } from "vitest";

import { FlushDisclosureContent } from "../components/disclosure/FlushDisclosureContent";
import { QuietDisclosureTrigger } from "../components/disclosure/QuietDisclosure";
import { mountHost } from "../test/mountHost";
import { Disclosure, DisclosureBody, DisclosureContent, DisclosureHeading, DisclosureTrigger } from "./Disclosure";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// jsdom loads no stylesheet, so the padding itself cannot be read: what is asserted is the marker the stylesheet keys on.
const bodyOf = (container: HTMLElement) => container.querySelector(".disclosure__body")!;

it("marks the outer element of a flush body, and not that of an ordinary one", () => {
  const flush = mountHost(
    <Disclosure expanded>
      <DisclosureContent><DisclosureBody flush><span data-testid="inner">in</span></DisclosureBody></DisclosureContent>
    </Disclosure>,
  );
  expect(bodyOf(flush).hasAttribute("data-flush")).toBe(true);
  expect(bodyOf(flush).querySelector("[data-testid=inner]")).not.toBeNull();
  const plain = mountHost(
    <Disclosure expanded>
      <DisclosureContent><DisclosureBody>in</DisclosureBody></DisclosureContent>
    </Disclosure>,
  );
  expect(bodyOf(plain).hasAttribute("data-flush")).toBe(false);
});

it("is the flush body that FlushDisclosureContent puts its content in", () => {
  const container = mountHost(
    <Disclosure expanded>
      <DisclosureHeading><QuietDisclosureTrigger>More</QuietDisclosureTrigger></DisclosureHeading>
      <FlushDisclosureContent className="inner-class"><span>folded away</span></FlushDisclosureContent>
    </Disclosure>,
  );
  expect(bodyOf(container).hasAttribute("data-flush")).toBe(true);
  expect(bodyOf(container).textContent).toBe("folded away");
  expect(bodyOf(container).querySelector(".inner-class")).not.toBeNull();
});

it("marks the outer element of an indented body, and not that of an ordinary or a flush one", () => {
  const indent = mountHost(
    <Disclosure expanded>
      <DisclosureContent><DisclosureBody flush indent>in</DisclosureBody></DisclosureContent>
    </Disclosure>,
  );
  expect(bodyOf(indent).hasAttribute("data-indent")).toBe(true);
  expect(bodyOf(indent).hasAttribute("data-flush")).toBe(true);
  const flush = mountHost(
    <Disclosure expanded>
      <DisclosureContent><DisclosureBody flush>in</DisclosureBody></DisclosureContent>
    </Disclosure>,
  );
  expect(bodyOf(flush).hasAttribute("data-indent")).toBe(false);
  const plain = mountHost(
    <Disclosure expanded>
      <DisclosureContent><DisclosureBody>in</DisclosureBody></DisclosureContent>
    </Disclosure>,
  );
  expect(bodyOf(plain).hasAttribute("data-indent")).toBe(false);
});

it("gives a trigger the id it is given, and describes it by the element it names", () => {
  const container = mountHost(
    <Disclosure>
      <DisclosureTrigger id="the-trigger" describedBy="the-description">More</DisclosureTrigger>
    </Disclosure>,
  );
  const trigger = container.querySelector("button")!;
  expect(trigger.id).toBe("the-trigger");
  expect(trigger.getAttribute("aria-describedby")).toBe("the-description");
});
