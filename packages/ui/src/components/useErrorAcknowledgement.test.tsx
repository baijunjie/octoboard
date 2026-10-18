// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { useErrorAcknowledgement } from "./useErrorAcknowledgement";

function FailedState({ message }: { message?: string }): null {
  useErrorAcknowledgement(message, "Could not read the file");
  return null;
}

it("acknowledges a durable failure once, and reports a new failed attempt after recovery starts", () => {
  const container = document.createElement("div");
  const root = createRoot(container);
  const render = (message?: string) => act(() => root.render(<FailedState message={message} />));
  try {
    render("Permission denied");
    expect(errorAlerts.getSnapshot()).toMatchObject([{ message: "Permission denied", title: "Could not read the file" }]);
    act(() => errorAlerts.acknowledge());
    render("Permission denied");
    expect(errorAlerts.getSnapshot()).toEqual([]);
    render();
    render("Permission denied");
    expect(errorAlerts.getSnapshot()).toHaveLength(1);
  } finally {
    act(() => root.unmount());
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
    container.remove();
  }
});
