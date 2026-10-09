// Sections the homepage, the privacy page, and the agent Markdown all walk.
// Adding a visible section means adding it here; the pages iterate these lists.

export const homeFeatures = [
  { key: "One", icon: "tree-structure" },
  { key: "Two", icon: "terminal-window" },
  { key: "Three", icon: "plugs-connected" },
  { key: "Four", icon: "chat-circle-dots" },
] as const;

export const homeSteps = ["One", "Two", "Three"] as const;

export const homeQuestions = [
  "One",
  "Two",
  "Three",
  "Four",
  "Five",
  "Six",
  "Seven",
  "Eight",
  "Nine",
] as const;

export const developedPlatform = {
  name: "macOS",
  icon: "apple-logo",
} as const;

export const plannedPlatforms = [
  { name: "Linux", icon: "linux-logo" },
  { name: "Windows", icon: "windows-logo" },
  { name: "iOS", icon: "apple-logo" },
  { name: "Android", icon: "android-logo" },
] as const;

// Names shown beside the mascot. They are product names, not translated labels.
// icon is the file in public/agents/, without the extension.
export const heroAgents = [
  { name: "Claude Code", icon: "claude" },
  { name: "Codex", icon: "codex" },
  { name: "Grok Build", icon: "grok" },
] as const;

// The illustrative workflow in WorkflowDemo.vue. Paths and agent names are
// example content, not a live session.
export const workflowProjects = [
  { key: "Frontend", agent: "Codex", icon: "codex", path: "apps/web" },
  {
    key: "Backend",
    agent: "Claude Code",
    icon: "claude",
    path: "services/api",
  },
  { key: "Docs", agent: "Grok Build", icon: "grok", path: "docs" },
] as const;

export const privacySections = [
  "Local",
  "Agents",
  "Sharing",
  "Website",
  "Contact",
  "Future",
] as const;

export const githubPrivacyUrl =
  "https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement";
