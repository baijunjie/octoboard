# 09 Final Confirmation

> Goal: Confirm what earlier milestones implemented but could not verify at the time, none of which blocks a later
> milestone.
> Completion criteria: every item below is confirmed, and any defect it turns up is fixed here.

## Items

- [x] The gallery quick check passes over the scenarios that show provider names, marks, quota meters, the add-agent
      dialog's install links and the account dialog's config-directory placeholder, now read from the UI presentation
      modules and the served `providers`.
- [ ] The quota meters, the rail and the menu bar icon's menu draw each provider's colour as before in both light and
      dark (the colours moved from per-agent CSS tokens into the presentation modules' colour pairs), and Grok Build
      and a provider without a colour draw in the foreground.
- [ ] The website preview, which runs on the fixture daemon, still shows its agents' install links and names.
- [ ] A Claude Code session launched before the status-line command took `--provider` keeps its status line fed by
      the new daemon binary (the old command line parses to the sole provider declaring the status line).
- [x] The gallery quick check passes over the scenarios that show quota window names, reset credits, the account
      settings dialog's sharing switch and help, the account dialog's directory hint, the trust prompt's caution and
      scope, and session model names, now read from the served descriptors, capabilities and presentation modules;
      the file names in the trust caution and sharing help now render left-to-right in monospace.
- [x] The gallery quick check passes over the scenarios that show the provider settings rows (Claude Code's approval
      select and its approve-everything confirmation, Codex's reviewer select, the Claude compatibility switches, the
      failed save of a provider setting), the configuration import review and result's provider settings lines, and the
      alerts and notices of the renamed provider error codes (`provider.claude.workspace_untrusted`,
      `provider.codex.login_busy`, `provider.codex.unarchive_failed`), now rendered from the served settings schema and
      the provider modules' copy and code records.
- [ ] Both Rust crates compile on Linux (`cargo check`, the CI job no local check covers) with the provider id open:
      the registries' `#[cfg(test)]` entries and the const registry lookups.
