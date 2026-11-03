//! What the daemon knows and works out about each agent CLI apart from launching it
//! (`crate::adapter`): where each keeps its files on disk, which agents are available, which agent
//! a project directory is set up for, the models each offers, the model and reasoning effort a
//! console, project or session is given, what every conversation with Codex's app server shares,
//! and the JSON-RPC reply readers that app server and Grok Build's ACP stdio share.

pub(crate) mod availability;
pub(crate) mod codex_app_server;
pub(crate) mod detect;
pub(crate) mod json_rpc;
pub(crate) mod launch_settings;
pub(crate) mod layout;
pub(crate) mod models;
