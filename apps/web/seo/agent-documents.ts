import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import {
  developedPlatform,
  githubPrivacyUrl,
  heroAgents,
  homeFeatures,
  homeQuestions,
  homeSteps,
  plannedPlatforms,
  privacySections,
  workflowProjects,
} from "./page-outline.ts";
import {
  languageHomeUrl,
  locales,
  localizedPath,
  markdownUrl,
  pagePaths,
  type SiteLocale,
} from "./site-seo.ts";

// GitHub Pages cannot send X-Robots-Tag, Link, or Vary, and it cannot
// negotiate Accept. Each Markdown file therefore names the canonical HTML
// URL in its body, and the HTML head points at the file. The copies stay
// crawlable, so one can be indexed beside its HTML page. Header, footer,
// decorative art, section eyebrows, section numbers, the scroll cue, the
// MIT badge, and controls (the replay button among them) are omitted. The
// badge repeats the license line; the other labels are section chrome.

export type AgentCopy = {
  siteUrl: string;
  appName: string;
  operatorName: string;
  contactEmail: string;
  repositoryUrl: string;
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
  return `${parts.filter((part): part is string => Boolean(part)).join("\n\n").trim()}\n`;
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

function releasesUrl(repositoryUrl: string): string {
  return `${repositoryUrl.replace(/\/$/, "")}/releases`;
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

function homeMarkdown(copy: AgentCopy, locale: SiteLocale): string {
  const messages = catalog(copy, locale.code);
  const params = valuesFor(copy);
  const text = (key: string) => message(messages, key, params);
  const canonical = languageHomeUrl(copy.siteUrl, locale.code);
  const privacy = markdownUrl(
    copy.siteUrl,
    `/${localizedPath(locale.code, "privacy/")}`,
  );
  return document([
    heading(
      1,
      `${text("heroTitle")} ${text("heroTitleSecond")} ${text("heroTitleAccent")}`,
    ),
    `Canonical: ${canonical}`,
    text("heroEyebrow"),
    text("heroDescription"),
    text("heroStatus"),
    text("heroNote"),
    link(text("heroCta"), releasesUrl(copy.repositoryUrl)),
    text("heroAgentLabel"),
    heroAgents.map((agent) => agent.name).join(", "),
    heading(2, text("principlesTitle")),
    ...homeFeatures.flatMap((feature) => [
      heading(3, text(`feature${feature.key}Title`)),
      text(`feature${feature.key}Description`),
    ]),
    text("revealTagline"),
    heading(2, text("howTitle")),
    text("howDescription"),
    heading(3, text("demoLabel")),
    text("demoIllustration"),
    text("demoWorkspace"),
    `**${text("demoYou")}:** ${text("demoRequest")}`,
    `**${text("demoCoordinator")}:** ${text("demoCoordinatorText")}`,
    `${text("demoStatusWorking")} · ${text("demoStatusIdle")}`,
    ...workflowProjects.flatMap((project) => [
      heading(4, `${text(`demo${project.key}`)} — ${project.agent}`),
      `\`${project.path}\``,
      `${text("demoStatusWorking")} · ${text("demoStatusIdle")}`,
      text("demoContext"),
      text("demoRules"),
      text(`demo${project.key}Task`),
      `**${text("demoReportLabel")}:** ${text(`demo${project.key}Report`)}`,
    ]),
    heading(3, text("demoReportsTitle")),
    text("demoReportsDescription"),
    heading(3, text("demoDeliveryTitle")),
    text("demoDeliveryDescription"),
    text("demoDirect"),
    ...homeSteps.flatMap((step) => [
      heading(3, text(`step${step}Title`)),
      text(`step${step}Description`),
    ]),
    heading(2, text("proofTitle")),
    text("proofDescription"),
    text("proofCaption"),
    link(text("proofRepository"), copy.repositoryUrl),
    link(text("proofLicense"), licenseUrl(copy.repositoryUrl)),
    heading(2, text("localTitle")),
    text("localDescription"),
    link(text("localLink"), privacy),
    heading(3, text("remoteTitle")),
    text("remoteBadge"),
    text("remoteDescription"),
    heading(2, text("platformsTitle")),
    text("platformsDescription"),
    [
      `- ${developedPlatform.name}: ${text("platformsMacStatus")}`,
      ...plannedPlatforms.map(
        (platform) => `- ${platform.name}: ${text("remoteBadge")}`,
      ),
    ].join("\n"),
    heading(2, text("faqTitle")),
    ...homeQuestions.flatMap((question) => [
      heading(3, text(`faq${question}Question`)),
      text(`faq${question}Answer`),
    ]),
    heading(2, text("ctaTitle")),
    text("ctaDescription"),
    text("ctaNote"),
    link(text("heroCta"), releasesUrl(copy.repositoryUrl)),
  ]);
}

function privacyMarkdown(copy: AgentCopy, locale: SiteLocale): string {
  const messages = catalog(copy, locale.code);
  const params = valuesFor(copy);
  const text = (key: string) => message(messages, key, params);
  const canonical =
    copy.siteUrl + localizedPath(locale.code, "privacy/");
  const home = markdownUrl(copy.siteUrl, `/${localizedPath(locale.code, "")}`);
  return document([
    heading(1, text("privacyTitle")),
    `Canonical: ${canonical}`,
    text("privacyEyebrow"),
    text("privacyIntro"),
    text("privacyUpdated"),
    link(text("privacyBack"), home),
    ...privacySections.flatMap((section) => [
      heading(2, text(`privacy${section}Title`)),
      text(`privacy${section}Body`),
      section === "Website"
        ? link(text("privacyGithubLink"), githubPrivacyUrl)
        : undefined,
      section === "Contact"
        ? link(copy.contactEmail, `mailto:${copy.contactEmail}`)
        : undefined,
    ]),
  ]);
}

function truncate(value: string, max: number): string {
  const flat = flatten(value);
  return flat.length > max ? `${flat.slice(0, max)}...` : flat;
}

function llmsTxt(copy: AgentCopy): string {
  const english = catalog(copy, "en");
  const params = valuesFor(copy);
  const text = (key: string) => flatten(message(english, key, params));
  const notes = [
    text("localDescription"),
    text("remoteDescription"),
    ...homeQuestions.map(
      (question) =>
        `${text(`faq${question}Question`)} ${text(`faq${question}Answer`)}`,
    ),
    `Support: ${copy.contactEmail}`,
    `Repository: ${copy.repositoryUrl}`,
  ];
  const pageLink = (code: string, page: "" | "privacy/") => {
    const messages = catalog(copy, code);
    const titleKey = page === "" ? "seoTitle" : "privacySeoTitle";
    const descriptionKey = page === "" ? "seoDescription" : "privacySeoDescription";
    const title = flatten(message(messages, titleKey, params));
    const description = truncate(message(messages, descriptionKey, params), 160);
    const href = markdownUrl(copy.siteUrl, `/${localizedPath(code, page)}`);
    return `- ${link(title, href)}: ${description}`;
  };
  const languages = locales.map((locale) => {
    const href = markdownUrl(copy.siteUrl, `/${localizedPath(locale.code, "")}`);
    const suffix =
      locale.code === "en"
        ? "content included below"
        : "visit this language for content";
    return `- ${link(`${locale.name} (${locale.code})`, href)}: ${pagePaths.length} pages; ${suffix}.`;
  });
  return document([
    heading(1, copy.appName),
    `> ${text("seoDescription")}`,
    `Canonical Origin: ${copy.siteUrl}`,
    ["**Notes:**", ...notes].join("\n\n"),
    ["## Available Languages on Website", "", ...languages].join("\n"),
    ["## Pages", "", pageLink("en", ""), pageLink("en", "privacy/")].join("\n"),
    [
      "## Sitemap",
      "",
      `See the full ${link("sitemap", `${copy.siteUrl}sitemap.md`)} for all pages.`,
    ].join("\n"),
  ]);
}

function llmsFull(
  copy: AgentCopy,
  pages: Array<{ source: string; title: string; description: string; markdown: string }>,
): string {
  const description = flatten(
    message(catalog(copy, "en"), "seoDescription", valuesFor(copy)),
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
  for (const entry of [...entries].sort((a, b) => a.route.localeCompare(b.route))) {
    const key = entry.route.split("/").filter(Boolean)[0] ?? "";
    const group = groups.get(key);
    if (group) group.push(entry);
    else groups.set(key, [entry]);
  }
  const keys = [...groups.keys()].sort((a, b) => {
    if (a === "") return -1;
    if (b === "") return 1;
    return a.localeCompare(b);
  });
  const parts = [`# ${copy.appName} Sitemap`, "", "All pages in Markdown format."];
  for (const key of keys) {
    parts.push("", `## ${key || "Root"}`);
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
    for (const page of ["", "privacy/"] as const) {
      const route = `/${localizedPath(locale.code, page)}`;
      const markdown =
        page === "" ? homeMarkdown(copy, locale) : privacyMarkdown(copy, locale);
      const title = flatten(
        message(
          catalog(copy, locale.code),
          page === "" ? "seoTitle" : "privacySeoTitle",
          params,
        ),
      );
      const description = flatten(
        message(
          catalog(copy, locale.code),
          page === "" ? "seoDescription" : "privacySeoDescription",
          params,
        ),
      );
      pages.push({
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
    "llms-full.txt": llmsFull(
      copy,
      pages.map((page) => ({
        source: copy.siteUrl + page.route.slice(1),
        title: page.title,
        description: page.description,
        markdown: page.markdown,
      })),
    ),
    "sitemap.md": sitemapMd(
      copy,
      pages.map((page) => ({
        route: page.route,
        title: page.title,
        href: copy.siteUrl + page.file,
      })),
    ),
  };
  for (const page of pages) files[page.file] = page.markdown;
  for (const [name, body] of Object.entries(files)) {
    if (body.includes("{")) {
      throw new Error(`Unreplaced placeholder in ${name}`);
    }
  }
  return files;
}
