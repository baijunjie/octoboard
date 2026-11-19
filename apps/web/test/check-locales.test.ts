import assert from "node:assert/strict";
import test from "node:test";

import { productNameLiterals } from "../scripts/check-locales.mjs";

// The product name is written `{appName}` in website copy; `check:locales` refuses the literal.

test("a message with the product name as a word is refused, whatever surrounds it", () => {
  const messages = { plain: "Open Octoboard now.", glued: "Octoboard’s daemon", cjk: "打开 Octoboard。", glueCjk: "打开Octoboard应用", kana: "Octoboardはアプリです", hangul: "Octoboard은 앱입니다", clean: "Open {appName} now." };
  assert.deepEqual(productNameLiterals(messages, "Octoboard"), ["plain", "glued", "cjk", "glueCjk", "kana", "hangul"]);
});

test("the daemon's binary name and a lowercase host are not the product name", () => {
  const messages = { binary: "Install octoboardd, the {appName} daemon.", host: "https://octoboard.dev/docs", longer: "Octoboardd", digit: "Octoboard2", underscore: "Octoboard_beta", prefixed: "myOctoboard" };
  assert.deepEqual(productNameLiterals(messages, "Octoboard"), []);
});

test("a name is matched as the text it spells, not as a pattern", () => {
  assert.deepEqual(productNameLiterals({ spelled: "Open A.B now", other: "Open AxB now" }, "A.B"), ["spelled"]);
});
