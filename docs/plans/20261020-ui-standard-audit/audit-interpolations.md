# Interpolation index

Baseline: `3a65c903ee040533c898069e96ba5216f296bc39`. 425 messages, 565 key/parameter pairs (including plural-selection-only count), 382 literal production references. Full contexts, narrow dynamic routes and every dynamic expression are in audit-interpolations.json. A reference to a table declaration is not itself a render; follow its dynamic route described in audit-copy.md.

The 95-file quota integration at `7a7a2f34e` occurred after collection. These source locations and counts are
historical baseline evidence, not the current catalog/render inventory. Milestones 04/05 must refresh quota
accountUsage/ReasonText/quotaFormat, recovery and refusal routes before closing their copy scope.

| Key (English catalog line) | Parameters and classification | Literal production references; additional narrow dynamic render sites |
| --- | --- | --- |
| agents.noneFound:49 | pathVar: technical-literal | sidebar/Sidebar.tsx:634; terminal/TerminalPane.tsx:492 |
| agents.notInstalled:55 | agent: referenced-agent-name-or-plain-agent-label | components/AgentTabList.tsx:172 |
| startup.noAddress.shell:67 | port: technical-literal | StartupScreen.tsx:80 |
| startup.noAddress.dev:68 | dev: technical-literal | StartupScreen.tsx:83 |
| startup.noAddress.query:69 | port: technical-literal | StartupScreen.tsx:87 |
| startup.noAddress.env:70 | portVar: technical-literal | StartupScreen.tsx:90 |
| startup.noAddress.proxy:71 | proxyVar: technical-literal | StartupScreen.tsx:93 |
| error.recovery.diskSpace:80 | appName: global-product-name | Global parameter via format/Message; native-menu or PlainMessageKey/table route (audit-copy.md) |
| error.recovery.access:81 | appName: global-product-name | Global parameter via format/Message; native-menu or PlainMessageKey/table route (audit-copy.md) |
| error.recovery.connection:82 | retry: control-label-or-confirmation-literal | components/ErrorGuidance.tsx:24 |
| error.recovery.path:83 | browse: control-label-or-confirmation-literal | components/ErrorGuidance.tsx:25 |
| error.recovery.tool:89 | appName: global-product-name | Global parameter via format/Message; native-menu or PlainMessageKey/table route (audit-copy.md) |
| error.recovery.unlock:91 | unlock: control-label-or-confirmation-literal | components/ErrorGuidance.tsx:26 |
| error.recovery.availability:93 | appName: global-product-name | Global parameter via format/Message; native-menu or PlainMessageKey/table route (audit-copy.md) |
| error.recovery.configuration:104 | appName: global-product-name | Global parameter via format/Message; native-menu or PlainMessageKey/table route (audit-copy.md) |
| error.recovery.projectBranches:111 | worktreeList: technical-literal; branchList: technical-literal | components/ErrorGuidance.tsx:27 |
| error.unexpected.manualAddressGuidance:119 | address: technical-literal | dialogs/ErrorAlerts.tsx:107 |
| error.unexpected.reportGuidance:121 | report: control-label-or-confirmation-literal | dialogs/ErrorAlerts.tsx:108 |
| error.crashed:127 | appName: global-product-name | ErrorBoundary.tsx:46; ErrorBoundary.tsx:53 |
| menu.about:132 | appName: global-product-name | Global parameter via format/Message; native-menu or PlainMessageKey/table route (audit-copy.md) |
| menu.hide:135 | appName: global-product-name | Global parameter via format/Message; native-menu or PlainMessageKey/table route (audit-copy.md) |
| menu.quit:138 | appName: global-product-name | lifecycle/statusItemMenu.ts:64 |
| app.quit.title:156 | appName: global-product-name | App.tsx:1048 |
| exit.failed:161 | error: composed-prose-or-diagnostic | lifecycle/useAppExit.ts:116 |
| exit.daemonExited:162 | detail: composed-prose-or-diagnostic; appName: global-product-name | lifecycle/useAppExit.ts:157 |
| daemon.unreadable_request:171 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.internal_error:172 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.unknown_console:173 | console: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.unknown_project:174 | project: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.unknown_session:175 | session: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.unknown_shell_terminal:176 | shell_terminal: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.unknown_page:177 | page: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.unknown_account:178 | account: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.account_name_taken:179 | agent: referenced-agent-name-or-plain-agent-label; name: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.field_too_long:180 | field: technical-literal; limit: count-or-formatted-quantity | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.field_too_long.name:181 | limit: count-or-formatted-quantity | dialogs/ProjectDialog.tsx:153; dialogs/RequestedDialog.tsx:382; daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.field_too_long.description:182 | limit: count-or-formatted-quantity | dialogs/ProjectDialog.tsx:155; daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.field_required:183 | field: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.conflicting_fields:184 | first: technical-literal; second: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.invalid_launch_setting:185 | value: technical-literal; field: technical-literal; agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.project_directory_shared:190 | project: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.project_directory_trash_failed:193 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.path_not_absolute:194 | path: technical-literal; tilde: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.path_not_found:195 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.path_not_a_directory:196 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.path_already_exists:197 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.directory_unreadable:198 | path: technical-literal; detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.directory_unreachable:200 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.no_repositories_found:202 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.repository_name_missing:204 | url: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.git_clone_failed:205 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.git_remote_unreachable:206 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.models_binary_not_found:207 | binary: technical-literal; pathVar: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.models_shell_environment_failed:208 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.models_timed_out:209 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.models_command_failed:210 | agent: referenced-agent-name-or-plain-agent-label; detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.models_unreadable_output:211 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.quota_binary_not_found:212 | binary: technical-literal; pathVar: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.quota_shell_environment_failed:213 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.quota_timed_out:214 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.quota_command_failed:215 | agent: referenced-agent-name-or-plain-agent-label; detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.quota_unreadable_reply:216 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.quota_no_windows:217 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.login_failed:220 | agent: referenced-agent-name-or-plain-agent-label; detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.login_not_confirmed:221 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.login_already_running:222 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.agent_not_installed:225 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.agent_not_added:226 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.agent_already_added:227 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.agent_availability_pending:228 | appName: global-product-name; agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.added_agents_undecided:230 | appName: global-product-name | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.config_dir_taken:231 | agent: referenced-agent-name-or-plain-agent-label; name: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.config_dir_not_absolute:232 | agent: referenced-agent-name-or-plain-agent-label; tilde: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.config_dir_unreachable:233 | agent: referenced-agent-name-or-plain-agent-label; path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.grok_home_not_initialized:235 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.icon_too_large:238 | limit_kib: count-or-formatted-quantity | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.configuration_not_recognised:239 | appName: global-product-name | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.configuration_version_unsupported:240 | version: count-or-formatted-quantity; appName: global-product-name; supported: count-or-formatted-quantity | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.configuration_unreadable:242 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.handoff_to_same_agent:254 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.conversation_not_found:255 | agent: referenced-agent-name-or-plain-agent-label; path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.conversation_unresumable:257 | agent: referenced-agent-name-or-plain-agent-label | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.codex_unarchive_failed:259 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.conversation_to_resume_not_found:260 | agent: referenced-agent-name-or-plain-agent-label; path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.resume_id_invalid:262 | example: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.conversation_in_session:264 | session: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.relocation_failed:267 | path: technical-literal; detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.session_did_not_stop:269 | session: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.switch_did_not_come_up:271 | session: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.session_has_running_sessions:275 | count: count-or-formatted-quantity; sessions: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.queued_messages_lost:282 | appName: global-product-name | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.invalid_path:285 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.source_unavailable:286 | path: technical-literal; detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.file_not_found:287 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.permission_denied:288 | appName: global-product-name; path: technical-literal; detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.outside_scope:289 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.unsupported_file_type.unmerged:291 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.unsupported_file_type:292 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.source_changed:293 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.limit_exceeded:294 | appName: global-product-name; max: count-or-formatted-quantity | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.not_a_git_repository:295 | project: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.git_unavailable:296 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.invalid_branch_name:298 | branch: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.unknown_branch:299 | branch: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.invalid_commit:300 | commit: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.unknown_commit:301 | commit: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.no_merge_base:302 | left: technical-literal; right: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.no_merge_base_shallow:303 | left: technical-literal; right: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.clean_filtered:306 | path: technical-literal; filter: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.git_failed:308 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.terminal_unavailable:309 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.path_open_failed:310 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.directory_unavailable:311 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.git_push_failed:316 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.trust_directory_too_broad:318 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.trust_path_not_absolute:320 | path: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.console_session_trust_not_asked:323 | appName: global-product-name | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.binary_not_found:326 | binary: technical-literal; pathVar: technical-literal | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.shell_environment_timeout:327 | shell: technical-literal; command: technical-literal; timeout: count-or-formatted-quantity; appName: global-product-name | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.shell_environment_timeout.command:330 | command: technical-literal; timeout: count-or-formatted-quantity | components/DaemonErrorMessage.tsx:107; daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.shell_environment_timeout.consequence:333 | appName: global-product-name | components/DaemonErrorMessage.tsx:112; daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.agent_not_available:335 | agent: referenced-agent-name-or-plain-agent-label; pathVar: technical-literal; appName: global-product-name | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.claude_workspace_untrusted:337 | allow: technical-literal; deny: technical-literal; appName: global-product-name | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.queued_messages_dropped:339 | appName: global-product-name | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.handoff_messages_dropped:341 | appName: global-product-name | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.message_not_sent:343 | agent: referenced-agent-name-or-plain-agent-label; appName: global-product-name | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.launch_failed:345 | title: name-or-title; reason: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.launch_failed.title:347 | title: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.trust_answer_failed:348 | appName: global-product-name; agent: referenced-agent-name-or-plain-agent-label; reason: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.trust_not_carried_over:349 | appName: global-product-name; agent: referenced-agent-name-or-plain-agent-label; path: technical-literal; reason: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.trust_reason.terminal_write_failed:361 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.trust_reason.store_io_failed:366 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| statusItem.section:375 | status: composed-prose-or-diagnostic; count: count-or-formatted-quantity | lifecycle/statusItemMenu.ts:51 |
| statusItem.more:376 | count: count-or-formatted-quantity | lifecycle/statusItemMenu.ts:50 |
| statusItem.open:381 | appName: global-product-name | lifecycle/statusItemMenu.ts:63 |
| statusItem.quota:382 | agent: referenced-agent-name-or-plain-agent-label | lifecycle/statusItemQuota.ts:76 |
| statusItem.quota.account:384 | account: name-or-title; plan: composed-prose-or-diagnostic | lifecycle/statusItemQuota.ts:33 |
| statusItem.quota.detail:385 | percent: count-or-formatted-quantity; resets: count-or-formatted-quantity | lifecycle/statusItemQuota.ts:47 |
| statusItem.quota.detailNoReset:386 | percent: count-or-formatted-quantity | lifecycle/statusItemQuota.ts:47 |
| statusItem.quota.window:387 | window: composed-prose-or-diagnostic; percent: count-or-formatted-quantity; resets: count-or-formatted-quantity | lifecycle/statusItemQuota.ts:49 |
| statusItem.quota.windowNoReset:388 | window: composed-prose-or-diagnostic; percent: count-or-formatted-quantity | lifecycle/statusItemQuota.ts:50 |
| session.agentAccount:390 | agent: referenced-agent-name-or-plain-agent-label; account: name-or-title | sessionLabel.ts:91 |
| session.modelEffort:391 | model: name-or-title; effort: technical-literal | sessionModel.ts:47 |
| session.defaultName:394 | name: name-or-title | dialogs/SessionDialog.tsx:184; dialogs/openSessionRequest.ts:27 |
| session.displayTitle:395 | title: name-or-title; name: name-or-title | sessionLabel.ts:14 |
| session.withLocation:396 | session: name-or-title; location: composed-prose-or-diagnostic | sessionLabel.ts:49 |
| session.location.consoleSession:398 | console: name-or-title | sessionLabel.ts:33 |
| rail.waiting:421 | count: count-or-formatted-quantity | components/Rail.tsx:217 |
| rail.quota.selected:428 | agent: referenced-agent-name-or-plain-agent-label; account: name-or-title; quota: composed-prose-or-diagnostic | quota/sessionQuota.ts:37 |
| rail.quota.noneInstalledHint:434 | pathVar: technical-literal | quota/RailQuotaEntry.tsx:266 |
| pane.resizePanels:441 | upper: control-label-or-confirmation-literal; lower: control-label-or-confirmation-literal | panels/PanelStack.tsx:231 |
| panel.terminal.label:445 | project: name-or-title | panels/panelRegistry.tsx:93 |
| sidebar.projects.headingWithCount:453 | count: count-or-formatted-quantity | sidebar/Sidebar.tsx:536 |
| sidebar.session.unlockItem:469 | name: name-or-title | sidebar/rows.tsx:215 |
| sidebar.unpinItem:473 | name: name-or-title | sidebar/rows.tsx:188 |
| sidebar.activity.waiting:478 | name: name-or-title | sessionLabel.ts:139; components/Rail.tsx:97 (dynamic); sidebar/FocusView.tsx:339 (dynamic); sidebar/Sidebar.tsx:760 (dynamic) |
| sidebar.activity.working:479 | name: name-or-title | sessionLabel.ts:141; components/Rail.tsx:97 (dynamic); sidebar/FocusView.tsx:339 (dynamic); sidebar/Sidebar.tsx:760 (dynamic) |
| sidebar.activity.running:480 | name: name-or-title | sessionLabel.ts:143; components/Rail.tsx:97 (dynamic); sidebar/FocusView.tsx:339 (dynamic); sidebar/Sidebar.tsx:760 (dynamic) |
| sidebar.activity.waitingPinned:481 | name: name-or-title | sessionLabel.ts:139; components/Rail.tsx:97 (dynamic); sidebar/FocusView.tsx:339 (dynamic); sidebar/Sidebar.tsx:760 (dynamic) |
| sidebar.activity.workingPinned:482 | name: name-or-title | sessionLabel.ts:141; components/Rail.tsx:97 (dynamic); sidebar/FocusView.tsx:339 (dynamic); sidebar/Sidebar.tsx:760 (dynamic) |
| sidebar.activity.runningPinned:483 | name: name-or-title | sessionLabel.ts:143; components/Rail.tsx:97 (dynamic); sidebar/FocusView.tsx:339 (dynamic); sidebar/Sidebar.tsx:760 (dynamic) |
| sidebar.console.actions:484 | name: name-or-title | components/Rail.tsx:125; sidebar/Sidebar.tsx:432 |
| sidebar.consoleSessions.headingWithCount:488 | count: count-or-formatted-quantity | sidebar/Sidebar.tsx:629 |
| sidebar.project.ariaLabelPinned:495 | name: name-or-title | sidebar/Sidebar.tsx:762 |
| sidebar.project.actions:496 | name: name-or-title | sidebar/FocusView.tsx:140; sidebar/FocusView.tsx:415; sidebar/Sidebar.tsx:796 |
| dialog.restoreProjectChanges.title:504 | name: name-or-title | dialogs/RequestedDialog.tsx:334 |
| daemon.git_restore_failed:509 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.project_branches_not_determined:510 | name: name-or-title | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.session_branches_not_removed:511 | items: composed-prose-or-diagnostic; count: count-or-formatted-quantity | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| daemon.session_branches_not_removed.list:515 | count: count-or-formatted-quantity | components/DaemonErrorMessage.tsx:45; daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| menu.loadFailed:525 | detail: composed-prose-or-diagnostic | components/ActionMenu.tsx:255; components/ActionMenu.tsx:257 |
| daemon.git_switch_failed:530 | detail: composed-prose-or-diagnostic | daemonMessage.ts:265 (dynamic); components/DaemonErrorMessage.tsx:74 (dynamic) |
| sidebar.session.ariaLabel:537 | title: name-or-title; agent: referenced-agent-name-or-plain-agent-label; status: composed-prose-or-diagnostic | sessionLabel.ts:109; sessionLabel.ts:109 (dynamic) |
| sidebar.session.ariaLabelPinned:538 | title: name-or-title; agent: referenced-agent-name-or-plain-agent-label; status: composed-prose-or-diagnostic | sessionLabel.ts:109; sessionLabel.ts:109 (dynamic) |
| sidebar.session.boundTo:539 | name: name-or-title | sessionLabel.ts:114; sidebar/BindingBadge.tsx:45; sidebar/FocusView.tsx:592 |
| sidebar.session.actions:540 | title: name-or-title | sidebar/SessionActions.tsx:67 |
| sidebar.session.archiveItem:547 | title: name-or-title | sidebar/SessionActions.tsx:60; sidebar/SessionActions.tsx:60 (dynamic) |
| sidebar.session.deleteItem:548 | title: name-or-title | sidebar/SessionActions.tsx:60; sidebar/SessionActions.tsx:60 (dynamic) |
| sidebar.focus.newSessionIn:558 | name: name-or-title | sidebar/FocusView.tsx:408 |
| sidebar.focus.newSessionInQuick:559 | name: name-or-title | sidebar/FocusView.tsx:409 |
| sidebar.focus.enterConsoleSession:563 | name: name-or-title; count: count-or-formatted-quantity | sidebar/FocusView.tsx:454 |
| sidebar.focus.boundElsewhere:567 | count: count-or-formatted-quantity | sidebar/FocusView.tsx:444 |
| sidebar.focus.sessions:573 | count: count-or-formatted-quantity | sidebar/FocusView.tsx:173; sidebar/FocusView.tsx:260 |
| sidebar.focus.archived:574 | count: count-or-formatted-quantity | sidebar/FocusView.tsx:535 |
| sidebar.git.branch:576 | branch: technical-literal | gitStatusLabel.ts:19 |
| sidebar.git.detachedBranch:577 | branch: technical-literal | gitStatusLabel.ts:19 |
| sidebar.git.pushing:582 | branch: technical-literal | gitStatusLabel.ts:16 |
| sidebar.git.checking:584 | branch: technical-literal | gitStatusLabel.ts:17 |
| sidebar.git.pulling:586 | branch: technical-literal | gitStatusLabel.ts:18 |
| sidebar.git.ahead:588 | count: count-or-formatted-quantity | gitStatusLabel.ts:32 |
| sidebar.git.behind:592 | count: count-or-formatted-quantity | gitStatusLabel.ts:33 |
| sidebar.git.error:596 | error: composed-prose-or-diagnostic | gitStatusLabel.ts:34 |
| archive.title.project:601 | name: name-or-title | archive/ArchiveView.tsx:148 |
| archive.title.consoleSessions:602 | name: name-or-title | archive/ArchiveView.tsx:151 |
| archive.title.boundSessions:603 | name: name-or-title | archive/ArchiveView.tsx:150 |
| archive.count:604 | count: count-or-formatted-quantity | archive/ArchiveView.tsx:176 |
| archive.delete:609 | title: name-or-title | archive/ArchiveView.tsx:286; archive/ArchiveView.tsx:292 |
| session.meta.created:610 | time: count-or-formatted-quantity | components/SessionMetadata.tsx:72 |
| session.meta.archived:611 | time: count-or-formatted-quantity | components/SessionMetadata.tsx:71 |
| session.usage.total:612 | tokens: count-or-formatted-quantity; count: count-or-formatted-quantity | components/SessionMetadata.tsx:48 |
| session.usage.entry:620 | label: composed-prose-or-diagnostic; tokens: count-or-formatted-quantity | components/SessionMetadata.tsx:50 |
| welcome.toolsHint:633 | pathVar: technical-literal | WelcomeScreen.tsx:110; terminal/TerminalPane.tsx:492 |
| shellTerminal.actions:658 | title: name-or-title | terminal/ShellTerminalPanel.tsx:238; terminal/ShellTerminalPanel.tsx:311 |
| shellTerminal.tab.exited:661 | title: name-or-title | terminal/ShellTerminalPanel.tsx:217 |
| shellTerminal.closeBusy.title:666 | title: name-or-title | terminal/ShellTerminalPanel.tsx:315 |
| report.position:675 | index: count-or-formatted-quantity; total: count-or-formatted-quantity | report/ReportPanel.tsx:160 |
| browser.label:678 | project: name-or-title | panels/panelRegistry.tsx:86 |
| browser.error.fileTooLarge:692 | size: count-or-formatted-quantity; max: count-or-formatted-quantity | browser/browseError.ts:68 |
| browser.error.fileTooLargeNoSize:693 | max: count-or-formatted-quantity | browser/browseError.ts:69 |
| browser.error.patchTooLarge:697 | max: count-or-formatted-quantity | browser/browseError.ts:73 |
| git.row:720 | name: name-or-title; status: composed-prose-or-diagnostic | browser/changeRow.tsx:80; browser/changeRow.tsx:80 (dynamic) |
| git.row.selected:721 | name: name-or-title; status: composed-prose-or-diagnostic | browser/changeRow.tsx:80; browser/changeRow.tsx:80 (dynamic) |
| git.row.detail:722 | name: name-or-title; status: composed-prose-or-diagnostic; detail: composed-prose-or-diagnostic | browser/changeRow.tsx:81; browser/changeRow.tsx:81 (dynamic) |
| git.row.detail.selected:723 | name: name-or-title; status: composed-prose-or-diagnostic; detail: composed-prose-or-diagnostic | browser/changeRow.tsx:81; browser/changeRow.tsx:81 (dynamic) |
| git.row.lines:724 | added: count-or-formatted-quantity; removed: count-or-formatted-quantity | browser/changeRow.tsx:76 |
| git.row.detailLines:725 | detail: composed-prose-or-diagnostic; lines: composed-prose-or-diagnostic | browser/changeRow.tsx:77 |
| git.row.from:726 | path: technical-literal | browser/changeRow.tsx:29; browser/changeRow.tsx:35; browser/changeRow.tsx:46 (dynamic); browser/changeRow.tsx:92 (dynamic) |
| git.row.fromOutside:727 | path: technical-literal | browser/changeRow.tsx:29; browser/changeRow.tsx:33; browser/changeRow.tsx:46 (dynamic); browser/changeRow.tsx:92 (dynamic) |
| git.row.toOutside:728 | path: technical-literal | browser/changeRow.tsx:29; browser/changeRow.tsx:34; browser/changeRow.tsx:46 (dynamic); browser/changeRow.tsx:92 (dynamic) |
| git.changes.base:753 | base: technical-literal; baseCommit: technical-literal; tip: technical-literal | browser/GitView.tsx:147 |
| git.changes.noRemote:754 | branch: technical-literal | browser/GitView.tsx:141 |
| git.changes.comparing:757 | base: technical-literal | browser/GitView.tsx:87; browser/GitView.tsx:145 |
| git.changes.compareError:760 | message: composed-prose-or-diagnostic | browser/changesStatus.ts:29; browser/changesStatus.ts:92; browser/GitView.tsx:224 (dynamic) |
| git.changes.commitsError:762 | message: composed-prose-or-diagnostic | browser/changesStatus.ts:29; browser/changesStatus.ts:103; browser/GitView.tsx:224 (dynamic) |
| git.changes.listError:763 | message: composed-prose-or-diagnostic | browser/changesStatus.ts:29; browser/changesStatus.ts:80; browser/GitView.tsx:224 (dynamic) |
| git.changes.worktree:766 | name: technical-file-or-path-value | browser/GitView.tsx:126 |
| git.commit.source:768 | commit: technical-literal; parent: technical-literal | browser/useChangeReader.ts:433; browser/useChangeReader.ts:437 |
| git.commit.sourceRoot:769 | commit: technical-literal | browser/useChangeReader.ts:432; browser/useChangeReader.ts:436 |
| git.changes.commitsPartial:770 | count: count-or-formatted-quantity | browser/GitView.tsx:269 |
| git.branch.gone:775 | branch: technical-literal | browser/BranchSelect.tsx:46; browser/BranchSelect.tsx:47 |
| git.branch.responsible:776 | title: name-or-title | browser/BranchOriginBadge.tsx:39; browser/BranchOriginBadge.tsx:40 (dynamic); browser/BranchOriginBadge.tsx:44 (dynamic) |
| git.branch.responsibleSelected:777 | title: name-or-title | browser/BranchOriginBadge.tsx:39; browser/BranchOriginBadge.tsx:40 (dynamic); browser/BranchOriginBadge.tsx:44 (dynamic) |
| git.branch.responsibleGone:778 | title: name-or-title | browser/BranchOriginBadge.tsx:39; browser/BranchOriginBadge.tsx:40 (dynamic); browser/BranchOriginBadge.tsx:44 (dynamic) |
| git.branch.branchesError:780 | message: composed-prose-or-diagnostic | browser/GitView.tsx:232 |
| git.compare.source:782 | to: technical-literal; from: technical-literal; fromCommit: technical-literal; toCommit: technical-literal | browser/useChangeReader.ts:418; browser/useChangeReader.ts:419; browser/useChangeReader.ts:422 |
| git.compare.sourceRoot:783 | to: technical-literal; toCommit: technical-literal | browser/useChangeReader.ts:415; browser/useChangeReader.ts:416 |
| git.branch.deleted:784 | branch: technical-literal | browser/GitView.tsx:253 |
| browser.tree.error:791 | message: composed-prose-or-diagnostic | browser/FileTree.tsx:260; browser/FileTree.tsx:276 |
| browser.tree.row.selected:794 | name: technical-file-or-path-value | browser/FileTree.tsx:229 |
| browser.tree.row.removed:795 | name: technical-file-or-path-value | browser/FileTree.tsx:225 |
| browser.tree.row.unlisted:796 | name: technical-file-or-path-value | browser/FileTree.tsx:227 |
| browser.row.actions:797 | name: technical-file-or-path-value | browser/RowActions.tsx:44; browser/RowActions.tsx:44 (dynamic) |
| browser.directory.actions:798 | name: technical-file-or-path-value | browser/RowActions.tsx:44; browser/RowActions.tsx:44 (dynamic) |
| viewer.contents:819 | name: technical-file-or-path-value | viewer/CodeSurface.tsx:151; viewer/CodeSurface.tsx:239; viewer/MarkdownSurface.tsx:88 |
| viewer.markdown.footnoteBack:829 | reference: count-or-formatted-quantity | viewer/MarkdownDocument.tsx:343 |
| viewer.expand.unmodified:839 | count: count-or-formatted-quantity | viewer/CodeSurface.tsx:245 |
| viewer.expand.hiddenAll:847 | count: count-or-formatted-quantity | viewer/CodeSurface.tsx:250 |
| viewer.expand.failed:854 | reason: composed-prose-or-diagnostic | viewer/CodeSurface.tsx:263 |
| viewer.change.renamedFrom:872 | path: technical-literal | viewer/FileViewer.tsx:391 |
| viewer.change.outsideOld:873 | path: technical-literal | viewer/FileViewer.tsx:565; viewer/FileViewer.tsx:564 (dynamic) |
| viewer.change.outsideNew:874 | path: technical-literal | viewer/FileViewer.tsx:565; viewer/FileViewer.tsx:564 (dynamic) |
| viewer.change.contentFailed:880 | reason: composed-prose-or-diagnostic | viewer/FileViewer.tsx:688 |
| dialog.characterCount:901 | count: count-or-formatted-quantity; limit: count-or-formatted-quantity | dialogs/TextFieldDescription.tsx:9 |
| dialog.agentSettings.account:904 | agent: referenced-agent-name-or-plain-agent-label | dialogs/ConsoleAccountsField.tsx:100; dialogs/ProjectDialog.tsx:333; dialogs/useLaunchChoice.ts:67 |
| dialog.agentSettings.inheritValue:909 | value: technical-literal | dialogs/LaunchSettingsFields.tsx:49; dialogs/LaunchSettingsFields.tsx:49 (dynamic) |
| dialog.agentSettings.unlisted:911 | value: technical-literal | agentLaunchSettings.ts:31 |
| dialog.console.projectAgentSame:921 | agent: referenced-agent-name-or-plain-agent-label | dialogs/ConsoleDialog.tsx:196 |
| dialog.project.removeTag:960 | tag: name-or-title | dialogs/TagsInput.tsx:120 |
| dialog.project.agentInherit:962 | agent: referenced-agent-name-or-plain-agent-label | dialogs/ProjectDialog.tsx:225 |
| dialog.project.accountInherit:964 | account: name-or-title | dialogs/ProjectDialog.tsx:337 |
| dialog.project.agentDetectMechanism:968 | detect: control-label-or-confirmation-literal; claudeFile: technical-literal; agentsFile: technical-literal; claudeDir: technical-literal; codexDir: technical-literal | dialogs/ProjectDialog.tsx:316 |
| dialog.consoleSession.title:975 | console: name-or-title | dialogs/SessionDialog.tsx:226 |
| dialog.session.title:977 | project: name-or-title | dialogs/SessionDialog.tsx:225 |
| dialog.session.resumeInvalid:988 | example: technical-literal | dialogs/SessionDialog.tsx:142 |
| dialog.session.resumeExample:990 | example: technical-literal | dialogs/SessionDialog.tsx:265 |
| dialog.session.ownerFixed:993 | name: name-or-title | dialogs/SessionDialog.tsx:198; dialogs/SessionDialog.tsx:199 |
| dialog.session.ownerGone:995 | name: name-or-title | dialogs/SessionDialog.tsx:233; dialogs/useQuickCreateSession.ts:41 |
| dialog.typeToConfirm.label:1002 | word: control-label-or-confirmation-literal | dialogs/ConfirmDialog.tsx:115 |
| dialog.deleteConsole.title:1006 | name: name-or-title | dialogs/RequestedDialog.tsx:315 |
| dialog.removeProject.title:1008 | name: name-or-title | dialogs/RequestedDialog.tsx:95 |
| dialog.removeProject.running:1010 | count: count-or-formatted-quantity | dialogs/RequestedDialog.tsx:97 |
| dialog.removeProject.alongLockedRemove:1019 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:230 |
| dialog.removeProject.alongLockedHint:1023 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:231 |
| dialog.archiveSession.title:1027 | title: name-or-title | dialogs/RequestedDialog.tsx:137 |
| dialog.archiveSession.boundAndEnded:1030 | count: count-or-formatted-quantity | dialogs/RequestedDialog.tsx:139 |
| dialog.switchAccount.title:1034 | title: name-or-title; account: name-or-title | dialogs/SwitchAccountDialog.tsx:93 |
| dialog.switchAccount.all:1041 | agent: referenced-agent-name-or-plain-agent-label | dialogs/SwitchAccountDialog.tsx:107 |
| dialog.switchAccount.scope:1042 | count: count-or-formatted-quantity | dialogs/SwitchAccountDialog.tsx:109 |
| dialog.switchAccount.progress:1048 | completed: count-or-formatted-quantity; count: count-or-formatted-quantity | dialogs/SwitchAccountDialog.tsx:49 |
| dialog.switchAccount.current:1052 | title: name-or-title | dialogs/SwitchAccountDialog.tsx:50 |
| dialog.switchAccount.progressCurrent:1054 | completed: count-or-formatted-quantity; count: count-or-formatted-quantity; title: name-or-title | dialogs/SwitchAccountDialog.tsx:52 |
| dialog.switchAccount.failed:1058 | count: count-or-formatted-quantity | dialogs/SwitchAccountDialog.tsx:84 |
| dialog.handOff.title:1062 | title: name-or-title | dialogs/HandoffDialog.tsx:67 |
| dialog.handOff.movesBranches:1067 | count: count-or-formatted-quantity | dialogs/HandoffDialog.tsx:118 |
| dialog.handOff.movesSessions:1071 | count: count-or-formatted-quantity | dialogs/HandoffDialog.tsx:119 |
| dialog.handOff.movesOwner:1076 | owner: name-or-title | dialogs/HandoffDialog.tsx:120 |
| dialog.deleteSession.title:1083 | title: name-or-title | dialogs/RequestedDialog.tsx:187 |
| branchCleanup.worktree:1094 | path: technical-literal; branch: technical-literal | daemonMessage.ts:137 |
| branchCleanup.worktreeDetached:1095 | path: technical-literal | daemonMessage.ts:136 |
| branchCleanup.onBranch:1098 | branch: technical-literal | components/BranchGroups.tsx:60 |
| branchCleanup.withTag:1099 | name: composed-prose-or-diagnostic; tag: composed-prose-or-diagnostic | components/BranchGroups.tsx:84; daemonMessage.ts:138 |
| branchCleanup.failed:1100 | name: name-or-title; detail: composed-prose-or-diagnostic | daemonMessage.ts:139 |
| branchCleanup.plain.group:1101 | lead: composed-prose-or-diagnostic; items: composed-prose-or-diagnostic | daemonMessage.ts:228 |
| branchCleanup.plain.items:1102 | before: composed-prose-or-diagnostic; after: composed-prose-or-diagnostic | daemonMessage.ts:226; daemonMessage.ts:230; daemonMessage.ts:227 (dynamic) |
| branchCleanup.plain.groups:1103 | before: composed-prose-or-diagnostic; after: composed-prose-or-diagnostic | daemonMessage.ts:226; daemonMessage.ts:232; daemonMessage.ts:227 (dynamic) |
| branchCleanup.plain.end:1104 | groups: composed-prose-or-diagnostic | daemonMessage.ts:232 |
| branchCleanup.group.notRemoved:1105 | count: count-or-formatted-quantity | daemonMessage.ts:102; daemonMessage.ts:106; components/DaemonErrorMessage.tsx:33 (dynamic); daemonMessage.ts:229 (dynamic) |
| branchCleanup.group.uncommitted:1109 | count: count-or-formatted-quantity | daemonMessage.ts:102; daemonMessage.ts:107; dialogs/BranchCleanupOptions.tsx:146; components/DaemonErrorMessage.tsx:33 (dynamic); daemonMessage.ts:229 (dynamic) |
| branchCleanup.group.submodules:1113 | count: count-or-formatted-quantity | daemonMessage.ts:102; daemonMessage.ts:108; dialogs/BranchCleanupOptions.tsx:147; components/DaemonErrorMessage.tsx:33 (dynamic); daemonMessage.ts:229 (dynamic) |
| branchCleanup.group.locked:1117 | count: count-or-formatted-quantity | daemonMessage.ts:103; daemonMessage.ts:109; dialogs/BranchCleanupOptions.tsx:158; dialogs/BranchCleanupOptions.tsx:229; components/DaemonErrorMessage.tsx:33 (dynamic); daemonMessage.ts:229 (dynamic) |
| branchCleanup.group.unmerged:1121 | count: count-or-formatted-quantity | daemonMessage.ts:103; daemonMessage.ts:110; dialogs/BranchCleanupOptions.tsx:148; components/DaemonErrorMessage.tsx:33 (dynamic); daemonMessage.ts:229 (dynamic) |
| branchCleanup.group.checkedOut:1125 | count: count-or-formatted-quantity | daemonMessage.ts:103; daemonMessage.ts:111; components/DaemonErrorMessage.tsx:33 (dynamic); daemonMessage.ts:229 (dynamic) |
| branchCleanup.group.projectFolder:1129 | count: count-or-formatted-quantity | daemonMessage.ts:104; daemonMessage.ts:112; components/DaemonErrorMessage.tsx:33 (dynamic); daemonMessage.ts:229 (dynamic) |
| branchCleanup.consequence.force:1135 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:138 |
| branchCleanup.consequence.locked:1139 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:139 |
| branchCleanup.consequence.trashed:1143 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:200 |
| branchCleanup.consequence.deleted:1147 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:201 |
| branchCleanup.consequence.lockedTrash:1151 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:202 |
| branchCleanup.along:1156 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:220 |
| dialog.sessionBranches.force:1160 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:150 |
| dialog.lockedWorktrees.remove:1164 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:159 |
| dialog.sessionBranches.forceHint:1168 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:151 |
| dialog.lockedWorktrees.removeHint:1172 | count: count-or-formatted-quantity | dialogs/BranchCleanupOptions.tsx:160 |
| dialog.deleteSession.bound:1176 | count: count-or-formatted-quantity | dialogs/RequestedDialog.tsx:191 |
| dialog.deleteArchived.message:1180 | count: count-or-formatted-quantity | dialogs/RequestedDialog.tsx:244 |
| dialog.deleteArchived.bound:1184 | count: count-or-formatted-quantity | dialogs/RequestedDialog.tsx:247 |
| dialog.deleteArchived.title:1188 | count: count-or-formatted-quantity | dialogs/RequestedDialog.tsx:243 |
| dialog.trust.question:1193 | agent: referenced-agent-name-or-plain-agent-label; path: technical-literal | dialogs/TrustPromptDialog.tsx:55 |
| dialog.trust.questionForSession:1194 | agent: referenced-agent-name-or-plain-agent-label; path: technical-literal; session: name-or-title | dialogs/TrustPromptDialog.tsx:54 |
| dialog.trust.behavior:1195 | appName: global-product-name; agent: referenced-agent-name-or-plain-agent-label | dialogs/TrustPromptDialog.tsx:57 |
| dialog.trust.codexRoot:1197 | agent: referenced-agent-name-or-plain-agent-label | dialogs/TrustPromptDialog.tsx:58 |
| dialog.trust.caution.claude:1200 | file: technical-literal | dialogs/TrustPromptDialog.tsx:26 |
| dialog.trust.caution.codex:1201 | agent: referenced-agent-name-or-plain-agent-label | dialogs/TrustPromptDialog.tsx:28 |
| dialog.trust.caution.grok:1203 | agent: referenced-agent-name-or-plain-agent-label | dialogs/TrustPromptDialog.tsx:30 |
| dialog.trust.parent:1206 | directory: technical-literal | dialogs/TrustPromptDialog.tsx:99 |
| dialog.sessionRequest.requester.project:1212 | session: name-or-title; project: name-or-title | dialogs/SessionRequestDialog.tsx:54 |
| dialog.sessionRequest.requester.projectElsewhere:1213 | session: name-or-title; project: name-or-title; console: name-or-title | dialogs/SessionRequestDialog.tsx:53 |
| dialog.sessionRequest.requester.console:1214 | session: name-or-title | dialogs/SessionRequestDialog.tsx:58 |
| dialog.sessionRequest.requester.consoleElsewhere:1215 | session: name-or-title; console: name-or-title | dialogs/SessionRequestDialog.tsx:57 |
| dialog.sessionRequest.requester.session:1216 | session: name-or-title | dialogs/SessionRequestDialog.tsx:46 |
| dialog.sessionRequest.question.project:1217 | requester: composed-prose-or-diagnostic; agent: referenced-agent-name-or-plain-agent-label; project: name-or-title; console: name-or-title | dialogs/SessionRequestDialog.tsx:64 |
| dialog.sessionRequest.question.console:1219 | requester: composed-prose-or-diagnostic; agent: referenced-agent-name-or-plain-agent-label; console: name-or-title | dialogs/SessionRequestDialog.tsx:63 |
| dialog.sessionRequest.withBranches:1221 | question: composed-prose-or-diagnostic; branches: technical-literal | dialogs/SessionRequestDialog.tsx:68 |
| dialog.sessionRequest.branches:1222 | branches: technical-literal; count: count-or-formatted-quantity | dialogs/SessionRequestDialog.tsx:68 |
| configuration.export.scope.console:1238 | name: name-or-title | configuration/ExportConfigurationDialog.tsx:89 |
| configuration.export.paths.description:1243 | appName: global-product-name | configuration/ExportConfigurationDialog.tsx:112 |
| configuration.export.saved:1246 | name: technical-file-or-path-value | configuration/ExportConfigurationDialog.tsx:65; configuration/ExportConfigurationDialog.tsx:66 |
| configuration.export.failed:1248 | detail: composed-prose-or-diagnostic | configuration/ExportConfigurationDialog.tsx:61; configuration/failureText.ts:29; configuration/failureText.ts:33 (dynamic) |
| configuration.import.choose.intro:1250 | appName: global-product-name | configuration/ImportConfigurationDialog.tsx:197 |
| configuration.import.readFailed:1253 | detail: composed-prose-or-diagnostic | configuration/ImportConfigurationDialog.tsx:99; configuration/failureText.ts:29; configuration/failureText.ts:33 (dynamic) |
| configuration.import.notJson:1256 | name: technical-file-or-path-value | configuration/ImportConfigurationDialog.tsx:114; configuration/ImportConfigurationDialog.tsx:116 |
| configuration.import.review.intro:1257 | file: technical-literal; skip: control-label-or-confirmation-literal | configuration/ImportReview.tsx:299 |
| configuration.import.all.exportedPath:1264 | count: count-or-formatted-quantity | configuration/ImportReview.tsx:51; configuration/ImportReview.tsx:56; configuration/ImportReview.tsx:317 (dynamic) |
| configuration.import.all.defaultCloneDir:1265 | count: count-or-formatted-quantity | configuration/ImportReview.tsx:52; configuration/ImportReview.tsx:57; configuration/ImportReview.tsx:317 (dynamic) |
| configuration.import.all.clone:1266 | count: count-or-formatted-quantity | configuration/ImportReview.tsx:53; configuration/ImportReview.tsx:58; configuration/ImportReview.tsx:317 (dynamic) |
| configuration.import.all.skip:1267 | count: count-or-formatted-quantity | configuration/ImportReview.tsx:54; configuration/ImportReview.tsx:59; configuration/ImportReview.tsx:317 (dynamic) |
| configuration.import.all.applied:1268 | choice: control-label-or-confirmation-literal; count: count-or-formatted-quantity | configuration/ImportReview.tsx:314 |
| configuration.import.settings.turnsOn:1273 | setting: control-label-or-confirmation-literal; appName: global-product-name | configuration/ImportReview.tsx:63; configuration/ImportReview.tsx:287; configuration/ImportReview.tsx:87 (dynamic) |
| configuration.import.settings.archiveCleanup.turnsOn:1275 | setting: control-label-or-confirmation-literal; appName: global-product-name | configuration/ImportReview.tsx:64; configuration/ImportReview.tsx:289; configuration/ImportReview.tsx:87 (dynamic) |
| configuration.import.settings.turnsOff:1277 | setting: control-label-or-confirmation-literal | configuration/ImportReview.tsx:65; configuration/ImportReview.tsx:86; configuration/ImportReview.tsx:87 (dynamic) |
| configuration.import.settings.alreadyOn:1278 | setting: control-label-or-confirmation-literal | configuration/ImportReview.tsx:66; configuration/ImportReview.tsx:85; configuration/ImportReview.tsx:87 (dynamic) |
| configuration.import.settings.alreadyOff:1280 | setting: control-label-or-confirmation-literal | configuration/ImportReview.tsx:67; configuration/ImportReview.tsx:85; configuration/ImportReview.tsx:87 (dynamic) |
| configuration.import.console.projects:1282 | count: count-or-formatted-quantity | configuration/ImportReview.tsx:222 |
| configuration.import.fallback:1287 | agent: referenced-agent-name-or-plain-agent-label; name: name-or-title; appName: global-product-name | configuration/ImportReview.tsx:96 |
| configuration.import.project.exportedPath:1288 | path: technical-literal | configuration/ImportReview.tsx:145; configuration/ImportReview.tsx:141 (dynamic) |
| configuration.import.project.exportedPathUsed:1289 | path: technical-literal | configuration/ImportReview.tsx:144; configuration/ImportReview.tsx:141 (dynamic) |
| configuration.import.project.state.none:1293 | appName: global-product-name | configuration/ImportReview.tsx:45 |
| configuration.import.project.choice:1294 | project: name-or-title | configuration/ImportReview.tsx:129 |
| configuration.import.target.place:1300 | path: technical-literal | configuration/ImportReview.tsx:180 |
| configuration.import.target.clone:1301 | url: technical-literal; path: technical-literal | configuration/ImportReview.tsx:174 |
| configuration.import.directory.label:1302 | project: name-or-title | configuration/ImportReview.tsx:159 |
| configuration.import.directory.notAbsolute:1305 | tilde: technical-literal | configuration/ImportReview.tsx:243 |
| configuration.import.result.consolesCreated:1307 | count: count-or-formatted-quantity | configuration/ImportResult.tsx:164 |
| configuration.import.result.projectsCreated:1311 | count: count-or-formatted-quantity | configuration/ImportResult.tsx:165 |
| configuration.import.result.projectsLeftOut:1315 | count: count-or-formatted-quantity | configuration/ImportResult.tsx:166 |
| configuration.import.result.failedCount:1319 | count: count-or-formatted-quantity | configuration/ImportResult.tsx:155; configuration/ImportResult.tsx:167 |
| configuration.import.result.notAttempted:1323 | count: count-or-formatted-quantity | configuration/ImportResult.tsx:169 |
| configuration.import.result.fallbacks.intro:1345 | appName: global-product-name | configuration/ImportResult.tsx:256 |
| configuration.import.result.fallback.console:1347 | agent: referenced-agent-name-or-plain-agent-label; name: name-or-title; console: name-or-title | configuration/ImportResult.tsx:264 |
| configuration.import.result.fallback.project:1348 | agent: referenced-agent-name-or-plain-agent-label; name: name-or-title; project: name-or-title; console: name-or-title | configuration/ImportResult.tsx:269 |
| configuration.import.result.settings.on:1350 | setting: control-label-or-confirmation-literal | configuration/ImportResult.tsx:100; configuration/ImportResult.tsx:99 (dynamic) |
| configuration.import.result.settings.off:1351 | setting: control-label-or-confirmation-literal | configuration/ImportResult.tsx:100; configuration/ImportResult.tsx:99 (dynamic) |
| configuration.import.result.settings.failed:1352 | reason: composed-prose-or-diagnostic | configuration/ImportResult.tsx:157; configuration/ImportResult.tsx:185 |
| settings.language.description:1371 | appName: global-product-name | settings/LanguageSetting.tsx:17 |
| settings.archiveCleanup.description.off:1378 | appName: global-product-name | settings/ArchiveCleanupSetting.tsx:15; settings/ArchiveCleanupSetting.tsx:15 (dynamic) |
| settings.archiveCleanup.description.on:1380 | appName: global-product-name | settings/ArchiveCleanupSetting.tsx:15; settings/ArchiveCleanupSetting.tsx:15 (dynamic) |
| settings.trusted.description:1398 | appName: global-product-name | settings/TrustedFoldersSection.tsx:37 |
| settings.trusted.removeTitle:1402 | path: technical-literal | settings/TrustedFoldersSection.tsx:71 |
| settings.trusted.removeLabel:1405 | path: technical-literal | settings/TrustedFoldersSection.tsx:59 |
| settings.accounts.description:1406 | pathVar: technical-literal | settings/AgentAccountsSection.tsx:127 |
| settings.accounts.agentActions:1413 | agent: referenced-agent-name-or-plain-agent-label | settings/AgentListMenu.tsx:98 |
| settings.accounts.agentMoreActions:1414 | agent: referenced-agent-name-or-plain-agent-label | settings/AgentListMenu.tsx:115 |
| settings.accounts.removeAgentTitle:1418 | agent: referenced-agent-name-or-plain-agent-label | settings/AgentListMenu.tsx:140 |
| settings.accounts.removeAgentMessage:1419 | agent: referenced-agent-name-or-plain-agent-label | settings/AgentListMenu.tsx:143 |
| settings.accounts.removeAgentInUse:1420 | agent: referenced-agent-name-or-plain-agent-label | settings/AgentListMenu.tsx:146 |
| settings.accounts.removeAgentConsoles:1421 | count: count-or-formatted-quantity | settings/AgentListMenu.tsx:134 |
| settings.accounts.removeAgentProjects:1425 | count: count-or-formatted-quantity | settings/AgentListMenu.tsx:135 |
| settings.accounts.removeAgentSessions:1429 | count: count-or-formatted-quantity | settings/AgentListMenu.tsx:136 |
| settings.accounts.removeAgentNamed:1433 | what: composed-prose-or-diagnostic; names: name-or-title | settings/AgentListMenu.tsx:131 |
| settings.addAgent.addLabel:1439 | agent: referenced-agent-name-or-plain-agent-label | settings/AddAgentDialog.tsx:67 |
| settings.addAgent.installLabel:1440 | agent: referenced-agent-name-or-plain-agent-label | settings/AddAgentDialog.tsx:58 |
| settings.accounts.dialog.renameDefault:1447 | agent: referenced-agent-name-or-plain-agent-label | settings/AccountDialog.tsx:93 |
| settings.accounts.login.waitingCode:1451 | code: technical-literal | settings/AccountLoginDialog.tsx:154; settings/AccountLoginDialog.tsx:161 |
| settings.accounts.login.succeeded:1452 | name: name-or-title | settings/AccountLogin.tsx:108 |
| settings.accounts.login.dialogTitle:1453 | agent: referenced-agent-name-or-plain-agent-label; name: name-or-title | settings/AccountLoginDialog.tsx:124 |
| settings.accounts.login.failed:1455 | name: name-or-title | settings/AccountLoginDialog.tsx:145 |
| settings.accounts.login.timedOut:1456 | name: name-or-title | settings/AccountLoginDialog.tsx:144 |
| settings.accounts.approval.label:1462 | name: name-or-title; agent: referenced-agent-name-or-plain-agent-label | settings/AccountApprovalSelect.tsx:86; settings/AccountApprovalSelect.tsx:109 |
| settings.accounts.approval.bypassTitle:1470 | name: name-or-title | settings/AccountApprovalSelect.tsx:120 |
| settings.accounts.notInstalledTip:1473 | agent: referenced-agent-name-or-plain-agent-label; pathVar: technical-literal | settings/AgentAccountsSection.tsx:160 |
| settings.accounts.notUsable:1474 | agent: referenced-agent-name-or-plain-agent-label; pathVar: technical-literal | settings/AgentAccountsSection.tsx:254 |
| settings.accounts.actionsLabel:1475 | name: name-or-title; agent: referenced-agent-name-or-plain-agent-label | settings/AgentAccountsSection.tsx:311; settings/AgentAccountsSection.tsx:336 |
| settings.accounts.removeTitle:1476 | name: name-or-title | settings/AgentAccountsSection.tsx:215 |
| settings.accounts.removeMessage:1477 | agent: referenced-agent-name-or-plain-agent-label | settings/AgentAccountsSection.tsx:216 |
| settings.accounts.dialog.edit:1479 | agent: referenced-agent-name-or-plain-agent-label | settings/AccountDialog.tsx:95 |
| settings.accounts.dirHint:1483 | appName: global-product-name | settings/AccountDialog.tsx:133 |
| settings.accounts.dirHintGrok:1484 | appName: global-product-name | settings/AccountDialog.tsx:133 |
| settings.accounts.dirNotAbsolute:1486 | tilde: technical-literal | settings/AccountDialog.tsx:136 |
| settings.accounts.nameTaken:1487 | agent: referenced-agent-name-or-plain-agent-label; name: name-or-title | settings/AccountDialog.tsx:22; settings/AccountDialog.tsx:61 (dynamic); settings/AccountDialog.tsx:123 (dynamic) |
| settings.accounts.nameTakenDefault:1488 | name: name-or-title; agent: referenced-agent-name-or-plain-agent-label | settings/AccountDialog.tsx:23; settings/AccountDialog.tsx:61 (dynamic); settings/AccountDialog.tsx:123 (dynamic) |
| settings.accounts.nameReserved:1489 | name: name-or-title; agent: referenced-agent-name-or-plain-agent-label | settings/AccountDialog.tsx:24; settings/AccountDialog.tsx:61 (dynamic); settings/AccountDialog.tsx:123 (dynamic) |
| quota.window.weeklyModel:1492 | model: model-name-in-window-label | quota/quotaFormat.ts:51 |
| quota.remaining:1495 | window: composed-prose-or-diagnostic; percent: count-or-formatted-quantity | quota/quotaFormat.ts:63 |
| quota.resetsIn:1496 | time: count-or-formatted-quantity | quota/quotaFormat.ts:83 |
| quota.duration.days:1498 | days: count-or-formatted-quantity; hours: count-or-formatted-quantity | quota/quotaFormat.ts:71 |
| quota.duration.hours:1499 | hours: count-or-formatted-quantity; minutes: count-or-formatted-quantity | quota/quotaFormat.ts:73 |
| quota.duration.minutes:1500 | minutes: count-or-formatted-quantity | quota/quotaFormat.ts:74 |
| quota.meterValue:1501 | percent: count-or-formatted-quantity | quota/QuotaMeter.tsx:27 |
| quota.meterValueResets:1502 | percent: count-or-formatted-quantity; resets: count-or-formatted-quantity | quota/QuotaMeter.tsx:27 |
| quota.stale:1505 | age: count-or-formatted-quantity | lifecycle/statusItemQuota.ts:39; quota/AccountQuotaSummary.tsx:93 |
| settings.notifications.system:1511 | appName: global-product-name | settings/NotificationsSection.tsx:25 |
| settings.permissions.unreadable:1514 | appName: global-product-name | settings/PermissionsSection.tsx:92 |
| settings.permissions.lead:1515 | appName: global-product-name | settings/PermissionsSection.tsx:97 |
| settings.permissions.accessibility.refused:1523 | appName: global-product-name | settings/PermissionsSection.tsx:28; settings/PermissionsSection.tsx:64 (dynamic) |
| settings.permissions.restart:1524 | appName: global-product-name | settings/PermissionsSection.tsx:72 |
