import { expect, it } from "vitest";

import { BROWSER_HOST, MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { galleryAppMenu } from "./appMenu";
import { SCENARIOS } from "./fixtures";

it("gives a desktop-app scenario the native menu it has always had where it gives no host, or a host that has one", () => {
  expect(galleryAppMenu(true, undefined)).toBeDefined();
  expect(galleryAppMenu(true, MAC_HOST)).toBeDefined();
});

it("gives a desktop-app scenario no native menu where its host has none, as the Windows app has none", () => {
  expect(galleryAppMenu(true, WINDOWS_HOST)).toBeUndefined();
  expect(galleryAppMenu(true, BROWSER_HOST)).toBeUndefined();
});

it("gives a scenario that is not the desktop app no native menu, whatever its host says", () => {
  expect(galleryAppMenu(undefined, undefined)).toBeUndefined();
  expect(galleryAppMenu(false, MAC_HOST)).toBeUndefined();
});

it("has a desktop-app scenario on a Windows host, which the page's own Settings shortcut is seen on", () => {
  const found = SCENARIOS.filter(({ desktopApp, host }) => desktopApp && host?.os === "windows" && !host.appMenu);
  expect(found.map(({ id }) => id)).toContain("settings-shortcuts-windows");
});
