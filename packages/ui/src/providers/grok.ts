import type { ProviderPresentation } from "./types";

// Brand names, the same in every language.
const names = {
  "provider.grok.label": "Grok Build",
  "provider.grok.shortName": "Grok",
} as const;

// The copy that speaks of the provider's own files and behaviour, found by the components under `provider.<id>.` (see
// `ProviderMessage`).
const en = {
  "provider.grok.trustCaution":
    "Trusting lets {agent} load this project's own instructions, hooks and MCP servers.",
  "provider.grok.sharingNeverShared":
    "Login credentials, {trustedFolders} and {hooksPaths} are never shared.",
  "provider.grok.dirHint":
    "The directory does not need to exist yet. To retain login and conversations, sessions need a {grok} home initialized by running the CLI with that directory. This is checked when a session launches.",
  "provider.grok.setting.claude_compatibility.heading": "Claude compatibility",
  "provider.grok.setting.claude_compatibility.helpLabel": "About Claude compatibility",
  "provider.grok.setting.claude_compatibility.label": "Claude compatibility for {agent}",
  "provider.grok.setting.claude_compatibility.description": "When on, {agent} sessions load {claude} instructions ({claudeMd} and rules), skills, agents, commands and MCP servers, and follow {claude} hooks and permissions; when off, they follow only their own configuration. {appName} turns on the Claude support built into {agent}. A change takes effect when a session next starts or resumes.",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "Could not save Claude compatibility",
  "provider.grok.home_not_initialized": "{path} is not a {grok} home yet. Run {grok} against it directly at least once so it has a session history, before pointing an account at it.",
  "provider.grok.home_not_initialized.summary": "{path} is not a {grok} home yet.",
  "provider.grok.home_not_initialized.recovery": "Run {grok} against it directly at least once so it has a session history, before pointing an account at it.",
  "provider.grok.sandbox_unsupported":
    "The {agent} sandbox ({profile}, set by {source}) is incompatible with the home {appName} gives each session, so the session was not launched.",
  "provider.grok.sandbox_unsupported.remedy":
    "To launch this session through {appName}, turn the sandbox off where it is set, or ask whoever manages that setting to. A conversation started under the sandbox keeps it, so continue that one in {agent} itself.",
} as const;

const zhHans = {
  "provider.grok.trustCaution":
    "信任后，{agent} 会加载此项目自身的指令、钩子和 MCP 服务器。",
  "provider.grok.sharingNeverShared":
    "登录凭据、{trustedFolders} 和 {hooksPaths} 不会共享。",
  "provider.grok.dirHint":
    "该目录不必已存在。会话要保留登录信息和对话，必须使用已通过 CLI 初始化的 {grok} 主目录。这一点会在会话启动时检查。",
  "provider.grok.setting.claude_compatibility.heading": "兼容 Claude",
  "provider.grok.setting.claude_compatibility.helpLabel": "关于兼容 Claude",
  "provider.grok.setting.claude_compatibility.label": "{agent} 兼容 Claude",
  "provider.grok.setting.claude_compatibility.description": "开启后，{agent} 的会话会加载 {claude} 的指令（{claudeMd} 与规则）、技能、智能体、命令和 MCP 服务器，并遵循 {claude} 的钩子与权限；关闭后，会话只遵循 {agent} 自己的配置。{appName} 会打开 {agent} 自带的 Claude 支持。更改在会话下次启动或恢复时生效。",
  "provider.grok.setting.claude_compatibility.saveFailedTitle": "无法保存兼容 Claude 设置",
  "provider.grok.home_not_initialized": "{path} 还不是 {grok} 主目录。请先直接对它运行一次 {grok}，使其具有会话记录，再将账户指向它。",
  "provider.grok.home_not_initialized.summary": "{path} 还不是 {grok} 主目录。",
  "provider.grok.home_not_initialized.recovery": "请先直接对它运行一次 {grok}，使其具有会话记录，再将账户指向它。",
  "provider.grok.sandbox_unsupported":
    "{agent} 的沙箱（{profile}，由 {source} 设置）与 {appName} 为每个会话提供的主目录不兼容，因此未启动此会话。",
  "provider.grok.sandbox_unsupported.remedy":
    "请在设置沙箱的位置将其关闭，或联系管理该设置的人，以便通过 {appName} 启动此会话。在沙箱下开始的对话会一直沿用沙箱，请直接在 {agent} 中继续该对话。",
} as const;

/** Grok Build. The mark is taken from LobeHub's icon set (`@lobehub/icons-static-svg`, MIT), monochrome in the text
 * colour as its maker draws it. It has no colour of its own: its quota bars draw in the foreground, as the mark does. */
export const grok = {
  id: "grok",
  mark: {
    fill: "currentColor",
    d: "M9.27 15.29l7.978-5.897c.391-.29.95-.177 1.137.272.98 2.369.542 5.215-1.41 7.169-1.951 1.954-4.667 2.382-7.149 1.406l-2.711 1.257c3.889 2.661 8.611 2.003 11.562-.953 2.341-2.344 3.066-5.539 2.388-8.42l.006.007c-.983-4.232.242-5.924 2.75-9.383.06-.082.12-.164.179-.248l-3.301 3.305v-.01L9.267 15.292M7.623 16.723c-2.792-2.67-2.31-6.801.071-9.184 1.761-1.763 4.647-2.483 7.166-1.425l2.705-1.25a7.808 7.808 0 00-1.829-1A8.975 8.975 0 005.984 5.83c-2.533 2.536-3.33 6.436-1.962 9.764 1.022 2.487-.653 4.246-2.34 6.022-.599.63-1.199 1.259-1.682 1.925l7.62-6.815",
  },
  params: { grok: names["provider.grok.shortName"] },
  references: {
    sharingNeverShared: { trustedFolders: "trusted_folders.toml", hooksPaths: "hooks-paths" },
  },
  settingReferences: {
    claude_compatibility: { claudeMd: "CLAUDE.md" },
  },
  codes: {
    "provider.grok.home_not_initialized": { origin: "daemon", recovery: "account", recoveryLine: true, codeParams: ["path"] },
    "provider.grok.sandbox_unsupported": { origin: "daemon", recovery: "provider.grok.sandbox_unsupported.remedy", codeParams: ["source", "profile"], uiParams: { agent: names["provider.grok.label"] } },
  },
  messages: {
    en: { ...names, ...en },
    "zh-Hans": { ...names, ...zhHans },
  },
} as const satisfies ProviderPresentation;
