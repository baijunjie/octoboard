//! What an agent's terminal shows when its turn stopped on a usage limit, for the tests of what the
//! daemon makes of such a screen (a session shown waiting for the user only for it is at rest).

/// Codex's workspace credit-limit prompt, as the agent draws it.
pub(crate) const CODEX_CREDIT_LIMIT: &str = "You've reached your workspace credit limit\r\nYour workspace is out of credits. Ask your workspace owner to add more. Your included usage resets at 10:09 on 14 Nov.\r\n\u{203a} 1. Request credits\r\nPress a number to choose\r\n";

/// Grok Build's weekly-limit screen, as the agent draws it.
pub(crate) const GROK_USAGE_LIMIT: &str = "You hit your weekly limit.\n1 (\u{25cb}) Upgrade tier\n2 (\u{25cb}) Buy more credits\n3 (\u{25cb}) Try Again\n\u{2191}/\u{2193} navigate \u{b7} y copy Enter :submit / Shift+x :dismiss\n";
