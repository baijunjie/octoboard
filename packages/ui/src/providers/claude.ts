import type { ProviderPresentation } from "./types";

// Brand names, the same in every language.
const names = {
  "provider.claude.label": "Claude Code",
  "provider.claude.shortName": "Claude",
} as const;

// The copy that speaks of the provider's own files and behaviour, found by the components under `provider.<id>.` (see
// `ProviderMessage`).
const en = {
  "provider.claude.trustCaution":
    "Trusting lets this project's own permission rules and hooks take effect: its {file} may pre-approve tool permissions.",
  "provider.claude.sharingNeverShared":
    "Login credentials, {claudeJson} (which holds user-scope MCP servers) and {plans} are never shared.",
  "provider.claude.setting.approval_mode.heading": "Permission approval",
  "provider.claude.setting.approval_mode.helpLabel": "About permission approval",
  "provider.claude.setting.approval_mode.label": "Permission approval for {agent}",
  "provider.claude.setting.approval_mode.description": "Choose how {claude} handles permission requests in every account. Follow account configuration leaves it to each account's own configuration.",
  "provider.claude.setting.approval_mode.saveFailedTitle": "Could not save the permission approval",
  "provider.claude.setting.approval_mode.choice.auto": "Automatic",
  "provider.claude.setting.approval_mode.choice.manual": "Ask me",
  "provider.claude.setting.approval_mode.choice.bypass": "Approve everything",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "Approve everything?",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "Every tool will run without asking you in every {agent} account's sessions, including commands that change or delete files.",
  "provider.claude.workspace_untrusted":
    "{claude} has not been trusted with this directory, so this project's own {allow} permission rules are ignored until {claude}'s trust confirmation is answered. Its {deny} rules still apply, so a session is only more restrictive, never less. {appName} presses that confirmation for you once you have agreed to it.",
} as const;

const zhHans = {
  "provider.claude.trustCaution":
    "信任后，此项目自身的权限规则和钩子会生效：其 {file} 可能会预先批准工具权限。",
  "provider.claude.sharingNeverShared":
    "登录凭据、{claudeJson}（含用户级 MCP 服务器）和 {plans} 不会共享。",
  "provider.claude.setting.approval_mode.heading": "权限审批",
  "provider.claude.setting.approval_mode.helpLabel": "关于权限审批",
  "provider.claude.setting.approval_mode.label": "{agent} 的权限审批方式",
  "provider.claude.setting.approval_mode.description": "选择 {claude} 在所有账户中处理权限请求的方式。跟随账户配置则由各账户自己的配置决定。",
  "provider.claude.setting.approval_mode.saveFailedTitle": "无法保存权限审批设置",
  "provider.claude.setting.approval_mode.choice.auto": "自动",
  "provider.claude.setting.approval_mode.choice.manual": "由我审批",
  "provider.claude.setting.approval_mode.choice.bypass": "全部批准",
  "provider.claude.setting.approval_mode.confirm.bypass.title": "确认全部批准？",
  "provider.claude.setting.approval_mode.confirm.bypass.message": "在每个 {agent} 账户的会话中，所有工具都将直接运行，不再询问你，包括修改或删除文件的命令。",
  "provider.claude.workspace_untrusted":
    "{claude} 尚未信任此目录，因此在回应 {claude} 的信任确认之前，此项目自身的 {allow} 权限规则会被忽略。其 {deny} 规则仍然生效，因此会话只会更严格，不会更宽松。你同意之后，{appName} 会替你按下该确认。",
} as const;

/** Claude Code. The mark is taken from LobeHub's icon set (`@lobehub/icons-static-svg`, MIT), in the provider colour. The
 * colour is its orange darkened to keep 3:1 against every quota track it is drawn on, lightened on the dark ones
 * (see `.quota-meter--provider` in `style.css`). */
export const claude = {
  id: "claude",
  mark: {
    fill: "#D97757",
    d: "M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z",
  },
  colour: { light: "#c2562f", dark: "#ea835d" },
  params: { claude: names["provider.claude.label"] },
  references: {
    trustCaution: { file: ".claude/settings.json" },
    sharingNeverShared: { claudeJson: ".claude.json", plans: "plans/" },
  },
  codes: {
    "provider.claude.workspace_untrusted": { origin: "daemon", codeParams: ["allow", "deny"], uiParams: { allow: "allow", deny: "deny" } },
  },
  messages: {
    en: { ...names, ...en },
    "zh-Hans": { ...names, ...zhHans },
  },
} as const satisfies ProviderPresentation;
