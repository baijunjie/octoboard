import type { ProviderPresentation } from "./types";

// The presentation module of the test-only provider `test_agent`, registered only under the test runner (see
// `registry.ts`). It carries the copy of its one setting, `mode`, but no colour, references or codes, so a test finds
// what a provider with the least presentation does.
const en = {
  "provider.test_agent.label": "Test Agent",
  "provider.test_agent.shortName": "Test",
  "provider.test_agent.setting.mode.heading": "Test mode",
  "provider.test_agent.setting.mode.helpLabel": "About the test mode",
  "provider.test_agent.setting.mode.label": "Mode for {agent}",
  "provider.test_agent.setting.mode.description": "Choose how the test agent behaves.",
  "provider.test_agent.setting.mode.saveFailedTitle": "Could not save the test mode",
  "provider.test_agent.setting.mode.choice.careful": "Careful",
  "provider.test_agent.setting.mode.choice.reckless": "Reckless",
  "provider.test_agent.setting.mode.confirm.reckless.title": "Use reckless mode?",
  "provider.test_agent.setting.mode.confirm.reckless.message": "The test agent will skip every safeguard.",
} as const;

const zhHans = {
  "provider.test_agent.label": "Test Agent",
  "provider.test_agent.shortName": "Test",
  "provider.test_agent.setting.mode.heading": "测试模式",
  "provider.test_agent.setting.mode.helpLabel": "关于测试模式",
  "provider.test_agent.setting.mode.label": "{agent} 的模式",
  "provider.test_agent.setting.mode.description": "选择测试智能体的行为方式。",
  "provider.test_agent.setting.mode.saveFailedTitle": "无法保存测试模式",
  "provider.test_agent.setting.mode.choice.careful": "谨慎",
  "provider.test_agent.setting.mode.choice.reckless": "鲁莽",
  "provider.test_agent.setting.mode.confirm.reckless.title": "确认使用鲁莽模式？",
  "provider.test_agent.setting.mode.confirm.reckless.message": "测试智能体将跳过所有保护措施。",
} as const;

export const testAgent = {
  id: "test_agent",
  mark: { fill: "currentColor", d: "M4 4h16v16H4z" },
  messages: { en, "zh-Hans": zhHans },
} as const satisfies ProviderPresentation;
