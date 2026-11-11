//! The Octoboard MCP server: the orchestration and settings tools the console session drives
//! Octoboard with, the narrower set a project session that can own sessions drives its own project
//! with, the reporting tool a bound project session answers through, the reads every session has
//! with one scope, and the tools every session shares information and asks the user for a session
//! with no owner through.
//!
//! **Transport is a stdio child process, not an HTTP endpoint the agent connects to.** Each adapter
//! registers `octoboardd mcp --session … --kind … --port … --key-file … --token …` as a
//! `command`-type MCP server, and that child ([`stdio`]) speaks MCP on its stdin/stdout while
//! forwarding every call to the daemon over loopback HTTP, where [`dispatch`] runs it against the
//! real state. One mechanism covers every provider, which is why it was chosen over pointing them
//! at a Streamable HTTP endpoint in the daemon: Codex's verified injection takes a `command` and
//! `args` ("Injecting Octoboard into each agent" in `docs/agent-cli-reference.md`), and Grok's SSRF
//! protection rejects loopback HTTP outright, which is already why its hooks cannot be HTTP either.
//!
//! What the server announces and what sessions are told of it — the tool catalogue, the role
//! descriptions and the console session's instruction text — is static data in
//! [`crate::session_tools`], which the adapters and messaging read too; this module announces the
//! catalogue and runs its calls: [`dispatch`] checks and routes a call, [`tools`] holds one file
//! per family of tools, and [`lookup`] and [`views`] are what the tools share, reading arguments
//! and finding the records they name, and describing a record back to the agent.

pub(crate) mod dispatch;
mod lookup;
pub mod stdio;
#[cfg(test)]
mod test_support;
mod tools;
mod views;
