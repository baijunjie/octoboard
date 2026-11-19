import { expect, it } from "vitest";

import { translatorFor } from "./catalog";
import { LANGUAGES } from "./languages";

// The Run now button's accessible name has to hold the words it shows (WCAG 2.5.3, Label in Name), in each language.
it.each(LANGUAGES)("names the Run now button by a phrase that holds its visible label in %s", (language) => {
  const words = translatorFor(language);
  const visible = words("scheduledTasks.runNow");
  const name = words("scheduledTasks.runNowLabel", { name: "Morning report" });
  expect(name.toLocaleLowerCase(language)).toContain(visible.toLocaleLowerCase(language));
  expect(name).toContain("Morning report");
});
