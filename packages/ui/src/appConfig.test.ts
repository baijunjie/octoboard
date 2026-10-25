import { readFileSync } from "node:fs";

import { expect, it } from "vitest";

import { appConfig } from "./appConfig";

const repository = (file: string) => readFileSync(new URL(`../../../${file}`, import.meta.url), "utf8");

it("states the license and copyright the repository's LICENSE file does", () => {
  const license = repository("LICENSE");
  expect(license.startsWith(`${appConfig.license} License`)).toBe(true);
  expect(license).toContain(`Copyright (c) ${appConfig.copyrightYear} ${appConfig.author.name}`);
});

it("names the website the web package serves by default", () => {
  expect(repository("apps/web/site.config.ts")).toContain(`"${appConfig.websiteUrl}"`);
});
