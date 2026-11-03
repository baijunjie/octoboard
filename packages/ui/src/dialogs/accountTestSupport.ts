// The accounts the dialog test files share.
import { accountOf } from "../fixtures/builders";
import type { Account } from "../protocol";

export const work: Account = accountOf("a-work", "claude", "Work", "/home/dev/.claude-work");
export const personal: Account = accountOf("a-personal", "claude", "Personal", "/home/dev/.claude-personal");
