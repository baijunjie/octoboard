// @vitest-environment jsdom
import { expect, it } from "vitest";

import { FlushDisclosureContent, QuietDisclosureTrigger } from "../components/disclosure/QuietDisclosure";
import { mountHost } from "../test/mountHost";
import { Disclosure, DisclosureBody, DisclosureContent, DisclosureHeading } from "./Disclosure";

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
