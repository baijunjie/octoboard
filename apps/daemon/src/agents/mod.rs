//! What the daemon works out about the agent CLIs for any provider, apart from launching them
//! (`crate::adapter`): which agents are available, which agent a project directory is set up for,
//! the version and install of each one's CLI (`cli`), the models each offers, the model and
//! reasoning effort a console, project or session is given, and the JSON-RPC reply readers the
//! providers' JSON-RPC conversations share (Codex's app server, Grok Build's ACP stdio).

pub(crate) mod availability;
pub(crate) mod cli;
pub(crate) mod detect;
pub(crate) mod json_rpc;
pub(crate) mod launch_settings;
pub(crate) mod models;
