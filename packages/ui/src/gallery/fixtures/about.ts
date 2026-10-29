import type { LicensedPackage, ThirdPartyLicenses } from "../../about/thirdPartyLicenses";
import { describing, type Matcher } from "../interact";
import type { Scenario, Step } from "../scenario";
import { SAMPLE, snapshotState } from "./builders";

const GROUP = "Settings";

const state = snapshotState({ consoles: [SAMPLE.console], projects: [SAMPLE.web], sessions: SAMPLE.sessions });

const MIT = [
  "MIT License",
  "",
  "Copyright (c) 2024 The Example Authors",
  "",
  "Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated",
  "documentation files (the \"Software\"), to deal in the Software without restriction, including without limitation the",
  "rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to",
  "permit persons to whom the Software is furnished to do so, subject to the following conditions:",
  "",
  "The above copyright notice and this permission notice shall be included in all copies or substantial portions of the",
  "Software.",
  "",
  "THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE",
  "WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.",
].join("\n");

const APACHE = [
  "Copyright 2024 The Example Authors",
  "",
  "Licensed under the Apache License, Version 2.0 (the \"License\"); you may not use this file except in compliance with",
  "the License. You may obtain a copy of the License at",
  "",
  "    http://www.apache.org/licenses/LICENSE-2.0",
  "",
  "Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on",
  "an \"AS IS\" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the",
  "specific language governing permissions and limitations under the License.",
].join("\n");

const ISC = [
  "ISC License",
  "",
  "Copyright (c) 2024, Example Maintainers",
  "",
  "Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby",
  "granted, provided that the above copyright notice and this permission notice appear in all copies.",
].join("\n");

const [MIT_TEXT, APACHE_TEXT, ISC_TEXT] = [0, 1, 2];

const LONG_NAME = "@octoboard-example/internal-layout-primitives-for-extremely-long-package-names";

const npm = (name: string, version: string, license: string, texts: number[]): LicensedPackage => ({ name, version, ecosystem: "npm", license, texts });
const crate = (name: string, version: string, license: string, texts: number[]): LicensedPackage => ({ name, version, ecosystem: "cargo", license, texts });

/** What a build ships, in a few ecosystems and licenses: dual-licensed crates, a license shared by many packages, one
 * that says only "See license file" and ships no text (under a name too long for the row). */
const LICENSES: ThirdPartyLicenses = {
  texts: [MIT, APACHE, ISC],
  packages: [
    npm(LONG_NAME, "10.12.4-beta.3", "See license file", []),
    npm("@tauri-apps/api", "2.1.1", "Apache-2.0 OR MIT", [APACHE_TEXT, MIT_TEXT]),
    npm("lucide-react", "0.468.0", "ISC", [ISC_TEXT]),
    npm("react", "19.2.0", "MIT", [MIT_TEXT]),
    npm("react-dom", "19.2.0", "MIT", [MIT_TEXT]),
    npm("tailwindcss", "4.1.4", "MIT", [MIT_TEXT]),
    crate("hyper", "1.5.0", "MIT", [MIT_TEXT]),
    crate("ring", "0.17.8", "MIT AND ISC", [MIT_TEXT, ISC_TEXT]),
    crate("rustls", "0.23.16", "Apache-2.0 OR ISC OR MIT", [APACHE_TEXT, ISC_TEXT, MIT_TEXT]),
    crate("serde", "1.0.214", "MIT OR Apache-2.0", [MIT_TEXT, APACHE_TEXT]),
    crate("tauri", "2.1.1", "Apache-2.0 OR MIT", [APACHE_TEXT, MIT_TEXT]),
    crate("tokio", "1.41.1", "MIT", [MIT_TEXT]),
    crate("unicode-ident", "1.0.13", "(MIT OR Apache-2.0) AND Unicode-3.0", [MIT_TEXT, APACHE_TEXT]),
  ],
};

/** The summary of a package's row, which starts with its name and version. */
const packageRow = (name: string, version: string): Matcher => describing(`package(${name} ${version})`, (label) => label.startsWith(`${name} ${version}`));

const openAbout: Step[] = [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.about"))];
const search = (query: string): Step[] => [
  (ui) => ui.focus(ui.t("settings.about.thirdParty.search")),
  (ui) => ui.type(query),
];

export const aboutScenarios: Scenario[] = [
  {
    id: "settings-about-no-updater",
    group: GROUP,
    category: "normal",
    title: "About, a platform that cannot update itself",
    description:
      "The icon and the application's name with no version line under it, then no update rows at all: straight to the Website, GitHub repository and Issue tracker links, License with its View license link, Copyright, and Third-party licenses with View licenses. Compare with the Updates group's up-to-date scenario, the same section where the platform can update.",
    state,
    steps: openAbout,
  },
  {
    id: "settings-about-licenses",
    group: GROUP,
    category: "normal",
    title: "Third-party licenses, the whole list",
    description:
      "The dialog has the search field, a muted count of 13 packages and one closed row per package: a right-pointing chevron, name and version, its license, and whether it is an npm package or a Rust crate. The package with the very long name wraps in its row instead of widening the dialog, its license and npm label dropping to a second line; the licenses written as expressions (OR, AND) stay on one line each.",
    licensesDialog: LICENSES,
  },
  {
    id: "settings-about-licenses-expanded",
    group: GROUP,
    category: "normal",
    title: "Third-party licenses, packages expanded",
    description:
      "The first row, the package with the very long name, is open (its chevron points down) on the line \"This package does not ship a license text.\"; the second, @tauri-apps/api, is open with its two license texts one under the other, each in its own tinted block that keeps the text's line breaks and wraps long lines (scroll the dialog to see both). The other rows stay closed.",
    licensesDialog: LICENSES,
    steps: [(ui) => ui.press(packageRow(LONG_NAME, "10.12.4-beta.3")), (ui) => ui.press(packageRow("@tauri-apps/api", "2.1.1"))],
  },
  {
    id: "settings-about-licenses-search",
    group: GROUP,
    category: "normal",
    title: "Third-party licenses, searched",
    description:
      "\"apache\" is typed in the search field: only the packages whose name or license contains it are left (a license counts as well as a name, whatever its case), and the count above the list says 5 packages.",
    licensesDialog: LICENSES,
    steps: search("apache"),
  },
  {
    id: "settings-about-licenses-no-match",
    group: GROUP,
    category: "normal",
    title: "Third-party licenses, nothing matches the search",
    description: "The list is gone and an empty state stands in its place, under the search field holding the query and a count of 0: a search-off glyph over the heading \"No matching packages\", with nothing else.",
    licensesDialog: LICENSES,
    steps: search("zzz-no-such-package"),
  },
  {
    id: "settings-about-licenses-empty",
    group: GROUP,
    category: "normal",
    title: "Third-party licenses, not included in this build",
    description: "A build without the generated list: no search field and no count, only an empty state, a scroll glyph over the heading \"No license list in this build\" and the sentence that the list is generated when the application is built and this build was made without it.",
    licensesDialog: { texts: [], packages: [] },
  },
  {
    id: "settings-about-licenses-loading",
    group: GROUP,
    category: "normal",
    title: "Third-party licenses, loading",
    description: "The dialog's title and introduction with the muted line \"Loading licenses…\" as its only content, before the list arrives.",
    licensesDialog: "loading",
  },
  {
    id: "settings-about-licenses-failed",
    group: GROUP,
    category: "exceptional",
    title: "Third-party licenses, the list could not be loaded",
    description:
      "The dialog's title and introduction over a danger empty state: a danger glyph, the heading \"Could not load the licenses\", its summary and the remedy, with the loader's own text on a surface of its own under them and Try again as the primary button below it. The first load failed and the next one succeeds, so Try again replaces the failure with the list.",
    licensesDialog: { failsOnce: LICENSES },
  },
];
