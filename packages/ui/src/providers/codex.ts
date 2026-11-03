import type { ProviderPresentation } from "./types";

// Brand names, the same in every language.
const names = {
  "provider.codex.label": "Codex",
  "provider.codex.shortName": "Codex",
} as const;

// The copy that speaks of the provider's own files and behaviour, found by the components under `provider.<id>.` (see
// `ProviderMessage`).
const en = {
  "provider.codex.trustCaution":
    "Trusting lets {agent} load this project's own configuration, hooks and command rules, which can run code even without a model request. This confirmation does not change {agent}'s sandbox or approval policy, though the project's own configuration can.",
  "provider.codex.trustScope":
    "{agent} records trust for the root of the {git} repository containing this folder. For a subfolder or linked worktree, this covers more than this project.",
  "provider.codex.setting.approval_reviewer.heading": "Permission approval",
  "provider.codex.setting.approval_reviewer.helpLabel": "About permission approval",
  "provider.codex.setting.approval_reviewer.label": "Permission approval for {agent}",
  "provider.codex.setting.approval_reviewer.description": "Choose how {codex} reviews permission requests in every account. Automatic review may use additional tokens. Follow account configuration leaves it to each account's own configuration.",
  "provider.codex.setting.approval_reviewer.saveFailedTitle": "Could not save the permission approval",
  "provider.codex.setting.approval_reviewer.choice.auto_review": "Automatic review",
  "provider.codex.setting.approval_reviewer.choice.user": "Ask me",
  "provider.codex.setting.claude_compatibility.heading": "Claude compatibility",
  "provider.codex.setting.claude_compatibility.helpLabel": "About Claude compatibility",
  "provider.codex.setting.claude_compatibility.label": "Claude compatibility for {agent}",
  "provider.codex.setting.claude_compatibility.description": "When on, {agent} sessions load {claude} instructions ({claudeMd} and rules), skills, agents, commands and MCP servers, and follow {claude} hooks and permissions; when off, they follow only their own configuration. {appName} passes these in when a session starts or resumes, so a change takes effect then.",
  "provider.codex.setting.claude_compatibility.saveFailedTitle": "Could not save Claude compatibility",
  "provider.codex.login_busy": "Another {codex} account is logging in. Wait for it to finish, then try again.",
  "provider.codex.login_busy.summary": "Another {codex} account is logging in.",
  "provider.codex.login_busy.recovery": "Wait for it to finish, then try again.",
  "provider.codex.unarchive_failed": "{codex} could not restore this session's archived conversation: {detail}",
  "provider.codex.unarchive_failed.summary": "{codex} could not restore this session's archived conversation.",
  "provider.codex.unarchive_failed.remedy": "Check that {agent} runs in the login shell and can read its account directory, then reopen the session. If it still fails, delete the session and start a new one.",
} as const;

const zhHans = {
  "provider.codex.trustCaution":
    "信任后，{agent} 会加载此项目自身的配置、钩子和命令规则，即使没有模型请求，它们也可能运行代码。此确认本身不会改变 {agent} 的沙盒和审批策略，但项目自身的配置可以更改这些设置。",
  "provider.codex.trustScope":
    "{agent} 会为此文件夹所在 {git} 仓库的根目录记录信任。如果此项目是子文件夹或链接的工作树，范围会超出此项目。",
  "provider.codex.setting.approval_reviewer.heading": "权限审批",
  "provider.codex.setting.approval_reviewer.helpLabel": "关于权限审批",
  "provider.codex.setting.approval_reviewer.label": "{agent} 的权限审批方式",
  "provider.codex.setting.approval_reviewer.description": "选择 {codex} 在所有账户中审批权限请求的方式。自动审批可能额外消耗 token。跟随账户配置则由各账户自己的配置决定。",
  "provider.codex.setting.approval_reviewer.saveFailedTitle": "无法保存权限审批设置",
  "provider.codex.setting.approval_reviewer.choice.auto_review": "自动审批",
  "provider.codex.setting.approval_reviewer.choice.user": "由我审批",
  "provider.codex.setting.claude_compatibility.heading": "兼容 Claude",
  "provider.codex.setting.claude_compatibility.helpLabel": "关于兼容 Claude",
  "provider.codex.setting.claude_compatibility.label": "{agent} 兼容 Claude",
  "provider.codex.setting.claude_compatibility.description": "开启后，{agent} 的会话会加载 {claude} 的指令（{claudeMd} 与规则）、技能、智能体、命令和 MCP 服务器，并遵循 {claude} 的钩子与权限；关闭后，会话只遵循 {agent} 自己的配置。{appName} 会在会话启动或恢复时把这些传给 {agent}，更改也在那时生效。",
  "provider.codex.setting.claude_compatibility.saveFailedTitle": "无法保存兼容 Claude 设置",
  "provider.codex.login_busy": "另一个 {codex} 账户正在登录。请等它完成后再试。",
  "provider.codex.login_busy.summary": "另一个 {codex} 账户正在登录。",
  "provider.codex.login_busy.recovery": "请等它完成后再试。",
  "provider.codex.unarchive_failed": "{codex} 无法恢复此会话已归档的对话：{detail}",
  "provider.codex.unarchive_failed.summary": "{codex} 无法恢复此会话已归档的对话。",
  "provider.codex.unarchive_failed.remedy": "请确认 {agent} 能在登录 shell 中运行且可读取其账户目录，然后重新打开该会话；仍然失败时，请删除该会话并新建一个。",
} as const;

/** Codex. The mark is taken from LobeHub's icon set (`@lobehub/icons-static-svg`, MIT), monochrome in the text colour as
 * its maker draws it. The colour is a blue that keeps 3:1 against every quota track it is drawn on (see
 * `.quota-meter--provider` in `style.css`). */
export const codex = {
  id: "codex",
  mark: {
    fill: "currentColor",
    d: "M8.086.457a6.105 6.105 0 013.046-.415c1.333.153 2.521.72 3.564 1.7a.117.117 0 00.107.029c1.408-.346 2.762-.224 4.061.366l.063.03.154.076c1.357.703 2.33 1.77 2.918 3.198.278.679.418 1.388.421 2.126a5.655 5.655 0 01-.18 1.631.167.167 0 00.04.155 5.982 5.982 0 011.578 2.891c.385 1.901-.01 3.615-1.183 5.14l-.182.22a6.063 6.063 0 01-2.934 1.851.162.162 0 00-.108.102c-.255.736-.511 1.364-.987 1.992-1.199 1.582-2.962 2.462-4.948 2.451-1.583-.008-2.986-.587-4.21-1.736a.145.145 0 00-.14-.032c-.518.167-1.04.191-1.604.185a5.924 5.924 0 01-2.595-.622 6.058 6.058 0 01-2.146-1.781c-.203-.269-.404-.522-.551-.821a7.74 7.74 0 01-.495-1.283 6.11 6.11 0 01-.017-3.064.166.166 0 00.008-.074.115.115 0 00-.037-.064 5.958 5.958 0 01-1.38-2.202 5.196 5.196 0 01-.333-1.589 6.915 6.915 0 01.188-2.132c.45-1.484 1.309-2.648 2.577-3.493.282-.188.55-.334.802-.438.286-.12.573-.22.861-.304a.129.129 0 00.087-.087A6.016 6.016 0 015.635 2.31C6.315 1.464 7.132.846 8.086.457zm-.804 7.85a.848.848 0 00-1.473.842l1.694 2.965-1.688 2.848a.849.849 0 001.46.864l1.94-3.272a.849.849 0 00.007-.854l-1.94-3.393zm5.446 6.24a.849.849 0 000 1.695h4.848a.849.849 0 000-1.696h-4.848z",
  },
  colour: { light: "#2a6df0", dark: "#6ea0ff" },
  params: { codex: names["provider.codex.label"] },
  planNames: { self_serve_business_prolite: "Business Pro Lite" },
  settingReferences: {
    claude_compatibility: { claudeMd: "CLAUDE.md" },
  },
  codes: {
    "provider.codex.login_busy": { origin: "daemon", recovery: "none", recoveryLine: true },
    "provider.codex.unarchive_failed": { origin: "daemon", recovery: "provider.codex.unarchive_failed.remedy", detailBlock: true },
  },
  messages: {
    en: { ...names, ...en },
    "zh-Hans": { ...names, ...zhHans },
  },
} as const satisfies ProviderPresentation;
