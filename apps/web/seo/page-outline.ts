// Sections the homepage, the privacy page, and the agent Markdown all walk.
// Adding a visible section means adding it here; the pages iterate these lists.

export const featureGroups = [
  { id: "feature-accounts", key: "featureAccountsTitle", icon: "terminal-window", features: [
    "managedAccounts", "switchAccount", "smartAccount", "usageMonitor", "autoSwitch", "quotaRecovery",
  ] },
  { id: "feature-context", key: "featureContextTitle", icon: "chat-circle-dots", features: [
    "contextWindow", "conversationRelay", "agentHandoff",
  ] },
  { id: "feature-collaboration", key: "featureCollaborationTitle", icon: "tree-structure", features: [
    "teamCoordination", "projectCoordination", "separateProjectContext", "existingWorkflow", "directParticipation", "sessionStatus", "reviewChanges",
  ] },
] as const;

export const homeFeatures = featureGroups.flatMap((group) => group.features.map((key) => ({ key })));

export const homeSections = [
  { id: "overview", key: "navOverview" },
  { id: "benefits", key: "navFeatures" },
  { id: "product-preview", key: "previewEyebrow" },
  { id: "how-it-works", key: "navHow" },
  { id: "developer-intent", key: "developerEyebrow" },
  { id: "local-workspace", key: "localEyebrow" },
  { id: "platforms", key: "platformsTitle" },
  { id: "questions", key: "navFaq" },
  { id: "download", key: "heroCta" },
] as const;

export const homeSteps = ["One", "Two", "Three"] as const;

export const developerSections = ["Shift", "Context", "Quality"] as const;

export const homeQuestions = [
  "Setup",
  "DataFlow",
  "Subagents",
  "Brands",
  "Participation",
  "SetupCosts",
  "RemoteExecution",
  "ParallelWork",
  "CliResume",
] as const;

export const developedPlatform = {
  name: "macOS",
  icon: "apple-logo",
} as const;

export const plannedPlatforms = [
  { name: "Linux", icon: "linux-logo", status: "platformsLinuxStatus" },
  { name: "Windows", icon: "windows-logo", status: "remoteBadge" },
  { name: "iOS", icon: "apple-logo", status: "remoteBadge" },
  { name: "Android", icon: "android-logo", status: "remoteBadge" },
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
  { key: "Admin", agent: "Codex", icon: "codex", path: "apps/admin-web" },
  {
    key: "Merchant",
    agent: "Claude Code",
    icon: "claude",
    path: "apps/merchant-web",
  },
  {
    key: "Customer",
    agent: "Grok Build",
    icon: "grok",
    path: "apps/customer-web",
  },
  { key: "Server", agent: "Codex", icon: "codex", path: "services/orders" },
  { key: "Mobile", agent: "Claude Code", icon: "claude", path: "apps/mobile" },
  { key: "Tablet", agent: "Grok Build", icon: "grok", path: "apps/waiter-pad" },
  { key: "Pos", agent: "Codex", icon: "codex", path: "apps/cashier-pos" },
  {
    key: "Kitchen",
    agent: "Claude Code",
    icon: "claude",
    path: "apps/kitchen-display",
  },
] as const;

// The constellation and its readable copy describe the same participating repositories.
export const workflowNodes = workflowProjects.map((project) => ({
  project,
  path: project.path,
}));

export const privacySections = [
  "Local",
  "Agents",
  "Sharing",
  "Website",
  "Contact",
  "RemoteExecution",
] as const;

export const githubPrivacyUrl =
  "https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement";
