//! What the daemon knows and works out about each agent CLI apart from launching it
//! (`crate::adapter`): where each keeps its files on disk, which agents are available, the models
//! each offers, the model and reasoning effort a console, project or session is given, and what
//! every conversation with Codex's app server shares.

pub(crate) mod availability;
pub(crate) mod codex_app_server;
pub(crate) mod launch_settings;
pub(crate) mod layout;
pub(crate) mod models;
