import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import {
  appDownloads,
  appPanelKeys,
  installCommand,
  releasesUrl,
  linuxCommandParts,
  serverDownloads,
  type Downloads,
} from "./downloads.ts";
import { formatPrivacyDate } from "./privacy-date.ts";
import {
  availablePlatforms,
  developerSections,
  githubPrivacyUrl,
  heroAgents,
  featureGroups,
  homeFeatures,
  homeQuestions,
  homeQuestionAnswerKey,
  homeSteps,
  plannedPlatforms,
  privacySections,
  workflowNodes,
} from "./page-outline.ts";
import {
  languageHomeUrl,
  localeCodeFromPath,
  locales,
  localizedPath,
  markdownUrl,
  pagePaths,
  seoKeysFor,
  type SiteLocale,
} from "./site-seo.ts";

// GitHub Pages cannot send X-Robots-Tag, Link, or Vary, and it cannot
// negotiate Accept. Each Markdown file therefore names the canonical HTML
// URL in its body, and the HTML head points at the file. The copies stay
// crawlable, so one can be indexed beside its HTML page. Header, footer,
// decorative art, section eyebrows, section numbers, the scroll cue, the
// MIT badge, and interactive controls are omitted. The
// badge repeats the license line; the other labels are section chrome.

export type AgentCopy = {
  siteUrl: string;
  appName: string;
  operatorName: string;
  contactEmail: string;
  repositoryUrl: string;
  downloads: Downloads;
  messages: Record<string, Record<string, string>>;
};

export function formatMessage(
  message: string,
  params: Record<string, string>,
): string {
  const result = message.replace(/\{(\w+)\}/g, (match, key: string) => {
    const value = params[key];
    return value === undefined ? match : value;
  });
  if (result.includes("{")) {
    throw new Error(`Unreplaced placeholder: ${result}`);
  }
  return result;
}

function flatten(value: string): string {
  return value
    .replace(/[ \t]*\n[ \t]*/g, " ")
    .replace(/ {2,}/g, " ")
    .trim();
}

function document(parts: Array<string | false | undefined>): string {
  return `${parts
    .filter((part): part is string => Boolean(part))
    .join("\n\n")
    .trim()}\n`;
}

function heading(level: number, text: string): string {
  return `${"#".repeat(level)} ${flatten(text)}`;
}

function link(title: string, href: string): string {
  const safe = flatten(title).replaceAll("[", "\\[").replaceAll("]", "\\]");
  return `[${safe}](${href})`;
}

function message(
  messages: Record<string, string>,
  key: string,
  params: Record<string, string>,
): string {
  const value = messages[key];
  if (value === undefined) throw new Error(`Missing message: ${key}`);
  return formatMessage(value, params);
}

function licenseUrl(repositoryUrl: string): string {
  return `${repositoryUrl.replace(/\/$/, "")}/blob/main/LICENSE`;
}

function valuesFor(copy: AgentCopy): Record<string, string> {
  return { appName: copy.appName, operatorName: copy.operatorName };
}

function catalog(copy: AgentCopy, code: string): Record<string, string> {
  const messages = copy.messages[code];
  if (!messages) throw new Error(`Missing catalog: ${code}`);
  return messages;
}

type Text = (key: string, values?: Record<string, string>) => string;

// The Linux server's part of the download page: the install command, how to reach and keep the server, how to
// upgrade and the archives.
function linuxParts(copy: AgentCopy, text: Text, headings: boolean): Array<string | false> {
  const code = (value: string) => `\`${value}\``;
  const commands = Object.fromEntries(
    Object.entries(linuxCommandParts).map(([name, command]) => [name, code(command)]),
  );
  return [
    headings && heading(2, text("downloadLinuxTitle")),
    text("downloadLinuxDescription"),
    text("downloadLinuxRange"),
    `\`\`\`sh\n${installCommand(copy.siteUrl)}\n\`\`\``,
    text("downloadServerReach", commands),
    text("downloadPathNote", commands),
    text("downloadServiceSteps", commands),
    text("downloadUpgrade", commands),
    [
      text("downloadArchivesLabel"),
      ...serverDownloads.map(
        (target) =>
          `- ${link(text("downloadArchive", { architecture: target.architecture }), copy.downloads[target.id])}`,
      ),
    ].join("\n"),
  ];
}

function appLinks(copy: AgentCopy, text: Text): string {
  return appDownloads
    .map(
      (target) =>
        `- ${link(text("downloadButton", { platform: target.name }), copy.downloads[target.id])}`,
    )
    .join("\n");
}

function downloadMarkdown(copy: AgentCopy, locale: SiteLocale): string {
  const messages = catalog(copy, locale.code);
  const params = valuesFor(copy);
  const text: Text = (key, values = {}) => message(messages, key, { ...params, ...values });
  const canonical = copy.siteUrl + localizedPath(locale.code, "download/");
  return document([
    heading(1, text("downloadPageTitle")),
    `Canonical: ${canonical}`,
    text("downloadPageSummary"),
    ...appDownloads.flatMap((target) => [
      heading(2, target.name),
      link(text("downloadButton", { platform: target.name }), copy.downloads[target.id]),
      ...appPanelKeys[target.id].map((key) => text(key)),
    ]),
    ...linuxParts(copy, text, true),
    link(text("downloadAllReleases"), releasesUrl(copy.repositoryUrl)),
  ]);
}

function homeMarkdown(copy: AgentCopy, locale: SiteLocale): string {
  const messages = catalog(copy, locale.code);
  const params = valuesFor(copy);
  const text = (key: string, values: Record<string, string> = {}) =>
    message(messages, key, { ...params, ...values });
  const canonical = languageHomeUrl(copy.siteUrl, locale.code);
  const privacy = markdownUrl(
    copy.siteUrl,
    `/${localizedPath(locale.code, "privacy/")}`,
  );
  return document([
    heading(
      1,
      `${text("workspaceTitle")} ${text("workspaceTitleSecond")} ${text("workspaceTitleAccent")}`,
    ),
    `Canonical: ${canonical}`,
    text("brandSlogan"),
    text("brandSloganMeaning"),
    text("workspaceIntroduction"),
    text("heroStatus"),
    text("heroAvailability"),
    link(text("heroSecondary"), copy.repositoryUrl),
    text("workspaceAgentLabel"),
    heroAgents.map((agent) => agent.name).join(", "),
    heading(2, text("featureWallTitle")),
    text("featureWallDescription"),
    ...featureGroups.flatMap((group) => [
      heading(3, text(group.key)),
      ...group.features.flatMap((key) => [
        heading(4, text(`${key}FeatureTitle`)),
        text(`${key}FeatureDescription`),
      ]),
    ]),
    heading(2, text("previewTitle")),
    text("previewDescription"),
    text("previewCaption"),
    [
      text("previewScenarioOverview"),
      text("previewScenarioFocus"),
      text("previewScenarioChanges"),
    ].join(" · "),
    heading(2, text("coordinationGoalTitle")),
    text("coordinationDescription"),
    heading(3, text("demoLabel")),
    text("demoIllustration"),
    text("demoWorkspace"),
    `**${text("demoYou")}:** ${text("demoRequest")}`,
    `**${text("demoCoordinator")}:** ${text("demoCoordinatorText")}`,
    ...workflowNodes.flatMap(({ project, path }) => [
      heading(4, `${text(`demo${project.key}`)} — ${project.agent}`),
      `\`${path}\``,
      text("demoContext"),
      text("demoRules"),
      text(`demo${project.key}Task`),
      `**${text("demoReportLabel")}:** ${text(`demo${project.key}Report`)}`,
    ]),
    ...homeSteps.flatMap((step) => [
      heading(3, text(`step${step}Title`)),
      text(`step${step}Description`),
    ]),
    heading(2, text("developerTitle")),
    text("developerIntro"),
    ...developerSections.flatMap((section) => [
      heading(3, text(`developer${section}Title`)),
      text(`developer${section}Body`),
    ]),
    text("developerPurpose"),
    text("developerInvitation"),
    text("developerByline"),
    heading(2, text("localTitle")),
    text("localDescription"),
    link(text("localLink"), privacy),
    heading(3, text("proofTitle")),
    text("proofDescription"),
    text("proofFreeCaption"),
    link(text("proofRepository"), copy.repositoryUrl),
    link(text("proofLicense"), licenseUrl(copy.repositoryUrl)),
    heading(3, text("platformsTitle")),
    text("platformsIntroduction"),
    [
      ...[...availablePlatforms, ...plannedPlatforms].map(
        (platform) => `- ${platform.name}: ${text(platform.status)}`,
      ),
    ].join("\n"),
    text("platformsDownloadPointer"),
    link(
      text("downloadCta"),
      markdownUrl(copy.siteUrl, `/${localizedPath(locale.code, "download/")}`),
    ),
    heading(2, text("faqTitle")),
    ...homeQuestions.flatMap((question) => [
      heading(3, text(`faq${question}Question`)),
      text(homeQuestionAnswerKey(question)),
    ]),
    heading(2, text("downloadCta")),
    text("downloadTeaser"),
    link(
      text("downloadCta"),
      markdownUrl(copy.siteUrl, `/${localizedPath(locale.code, "download/")}`),
    ),
    text("ctaNote"),
  ]);
}

function privacyMarkdown(copy: AgentCopy, locale: SiteLocale): string {
  const messages = catalog(copy, locale.code);
  const params = valuesFor(copy);
  const text = (key: string) => message(messages, key, params);
  const canonical = copy.siteUrl + localizedPath(locale.code, "privacy/");
  const home = markdownUrl(copy.siteUrl, `/${localizedPath(locale.code, "")}`);
  return document([
    heading(1, text("privacyTitle")),
    `Canonical: ${canonical}`,
    text("privacyEyebrow"),
    text("privacyIntro"),
    message(messages, "privacyUpdated", { ...params, date: formatPrivacyDate(locale.language) }),
    link(text("privacyBack"), home),
    ...privacySections.flatMap((section) => [
      heading(2, text(`privacy${section}Title`)),
      text(section === "Website" ? "privacyWebsiteConsentBody" : `privacy${section}Body`),
      section === "Website"
        ? link(text("privacyGithubLink"), githubPrivacyUrl)
        : undefined,
      section === "Contact"
        ? link(copy.contactEmail, `mailto:${copy.contactEmail}`)
        : undefined,
    ]),
  ]);
}

// The renderer of each page's Markdown copy.
const pageMarkdown = {
  "": homeMarkdown,
  "privacy/": privacyMarkdown,
  "download/": downloadMarkdown,
} as const;

function truncate(value: string, max: number): string {
  const flat = flatten(value);
  return flat.length > max ? `${flat.slice(0, max)}...` : flat;
}

function llmsTxt(copy: AgentCopy): string {
  const english = catalog(copy, "en");
  const params = valuesFor(copy);
  const text = (key: string) => flatten(message(english, key, params));
  const plain: Text = (key, values = {}) =>
    flatten(message(english, key, { ...params, ...values }));
  const notes = [
    text("workspaceIntroduction"),
    text("heroAvailability"),
    ...[...availablePlatforms, ...plannedPlatforms].map(
      (platform) => `${platform.name}: ${text(platform.status)}`,
    ),
    text("platformsDownloadPointer"),
    appLinks(copy, plain),
    ...linuxParts(copy, plain, false).filter((part): part is string => Boolean(part)),
    link(text("downloadAllReleases"), releasesUrl(copy.repositoryUrl)),
    text("proofFreeCaption"),
    text("localDescription"),
    ...homeFeatures.map((feature) => text(`${feature.key}FeatureDescription`)),
    ...homeQuestions.map(
      (question) =>
        `${text(`faq${question}Question`)} ${text(homeQuestionAnswerKey(question))}`,
    ),
    `Support: ${copy.contactEmail}`,
    `Repository: ${copy.repositoryUrl}`,
  ];
  const pageLink = (code: string, page: (typeof pagePaths)[number]) => {
    const messages = catalog(copy, code);
    const { title: titleKey, description: descriptionKey } = seoKeysFor(page);
    const title = flatten(message(messages, titleKey, params));
    const description = truncate(
      message(messages, descriptionKey, params),
      160,
    );
    const href = markdownUrl(copy.siteUrl, `/${localizedPath(code, page)}`);
    return `- ${link(title, href)}: ${description}`;
  };
  const languages = locales.map((locale) => {
    const href = markdownUrl(
      copy.siteUrl,
      `/${localizedPath(locale.code, "")}`,
    );
    const full = link("full content", `${copy.siteUrl}llms-full-${locale.code}.txt`);
    return `- ${link(`${locale.name} (${locale.code})`, href)}: ${pagePaths.length} pages; ${full}.`;
  });
  return document([
    heading(1, copy.appName),
    `> ${text("workspaceSeoSummary")}`,
    `Canonical Origin: ${copy.siteUrl}`,
    ["**Notes:**", ...notes].join("\n\n"),
    ["## Available Languages on Website", "", ...languages].join("\n"),
    ["## Pages", "", ...pagePaths.map((page) => pageLink("en", page))].join("\n"),
    [
      "## Full content",
      "",
      `Read ${link("English full content", `${copy.siteUrl}llms-full-en.txt`)} or choose a language above to read only that language's pages.`,
      `Read ${link("all languages", `${copy.siteUrl}llms-full.txt`)} only when you need the complete multilingual content.`,
    ].join("\n"),
    [
      "## Sitemap",
      "",
      `See the full ${link("sitemap", `${copy.siteUrl}sitemap.md`)} for all pages.`,
    ].join("\n"),
  ]);
}

function llmsFull(
  copy: AgentCopy,
  pages: Array<{
    source: string;
    title: string;
    description: string;
    markdown: string;
  }>,
  code = "en",
): string {
  const description = flatten(
    message(catalog(copy, code), "workspaceSeoSummary", valuesFor(copy)),
  );
  const header = document([
    heading(1, copy.appName),
    `> ${description}`,
    `Canonical Origin: ${copy.siteUrl}`,
  ]);
  const body = pages
    .map((page) =>
      [
        "---",
        "",
        `- **Page:** ${page.title}`,
        `- **Source:** ${page.source}`,
        `- **Description:** ${page.description}`,
        "",
        page.markdown.trim(),
        "",
      ].join("\n"),
    )
    .join("\n");
  return `${header}\n${body}`;
}

function sitemapMd(
  copy: AgentCopy,
  entries: Array<{ route: string; title: string; href: string }>,
): string {
  const groups = new Map<string, typeof entries>();
  for (const entry of [...entries].sort((a, b) =>
    a.route.localeCompare(b.route),
  )) {
    const key = localeCodeFromPath(entry.route);
    const group = groups.get(key);
    if (group) group.push(entry);
    else groups.set(key, [entry]);
  }
  // One section per language, English (the unprefixed pages) first as the root.
  const keys = [...groups.keys()].sort((a, b) => {
    if (a === "en") return -1;
    if (b === "en") return 1;
    return a.localeCompare(b);
  });
  const parts = [
    `# ${copy.appName} Sitemap`,
    "",
    "All pages in Markdown format.",
  ];
  for (const key of keys) {
    parts.push("", `## ${key === "en" ? "Root" : key}`);
    for (const entry of groups.get(key) ?? []) {
      parts.push(`- ${link(entry.title, entry.href)}`);
    }
  }
  return `${parts.join("\n")}\n`;
}

export async function readLocaleMessages(
  directory: string,
): Promise<Record<string, Record<string, string>>> {
  const messages: Record<string, Record<string, string>> = {};
  for (const locale of locales) {
    messages[locale.code] = JSON.parse(
      await readFile(resolve(directory, locale.file), "utf8"),
    ) as Record<string, string>;
  }
  return messages;
}

export function renderAgentFiles(copy: AgentCopy): Record<string, string> {
  const params = valuesFor(copy);
  const pages: Array<{
    code: string;
    source: string;
    route: string;
    file: string;
    markdown: string;
    title: string;
    description: string;
  }> = [];
  const ordered = [
    ...locales.filter((locale) => locale.code === "en"),
    ...locales.filter((locale) => locale.code !== "en"),
  ];
  for (const locale of ordered) {
    for (const page of pagePaths) {
      const route = `/${localizedPath(locale.code, page)}`;
      const markdown = pageMarkdown[page](copy, locale);
      const title = flatten(
        message(catalog(copy, locale.code), seoKeysFor(page).title, params),
      );
      const description = flatten(
        message(catalog(copy, locale.code), seoKeysFor(page).description, params),
      );
      pages.push({
        code: locale.code,
        source: copy.siteUrl + route.slice(1),
        route,
        file: markdownUrl(copy.siteUrl, route).slice(copy.siteUrl.length),
        markdown,
        title,
        description,
      });
    }
  }
  const files: Record<string, string> = {
    "llms.txt": llmsTxt(copy),
    "llms-full.txt": llmsFull(copy, pages),
    "sitemap.md": sitemapMd(
      copy,
      pages.map((page) => ({
        route: page.route,
        title: page.title,
        href: copy.siteUrl + page.file,
      })),
    ),
  };
  for (const { code } of ordered) {
    files[`llms-full-${code}.txt`] = llmsFull(copy, pages.filter((page) => page.code === code), code);
  }
  for (const page of pages) files[page.file] = page.markdown;
  for (const [name, body] of Object.entries(files)) {
    if (body.includes("{")) {
      throw new Error(`Unreplaced placeholder in ${name}`);
    }
  }
  return files;
}
