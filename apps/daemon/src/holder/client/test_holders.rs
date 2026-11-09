//! Holders for the tests: ones that run in this process, each in a thread of its own, registering
//! in a directory of the test's own.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use super::{ClientConfig, Holders, OnResolved, RelayHandler, RelayReply, RelayRequest};
use crate::holder::compat::Range;

/// Holders that run in this process, registering in `registry_dir`: what the tests of the held
/// terminals run against.
pub(crate) fn in_process(registry_dir: PathBuf) -> Result<Holders> {
    in_process_relaying(registry_dir, Arc::new(Unavailable))
}

/// What a test that looks only at what a start takes over at once has the holders that could not
/// be linked to resolve to: nothing.
pub(crate) fn unresolved() -> OnResolved {
    Arc::new(|_| {})
}

/// A relay handler that answers every request with 503, for the tests that relay nothing.
pub(crate) struct Unavailable;

impl RelayHandler for Unavailable {
    fn report(&self, _: RelayRequest) {}

    fn relay(&self, _: RelayRequest) -> tokio::task::JoinHandle<RelayReply> {
        tokio::spawn(async {
            RelayReply {
                status: 503,
                body: String::new(),
            }
        })
    }
}

/// [`in_process`], with the requests the holder relays handled by `relay`.
pub(crate) fn in_process_relaying(
    registry_dir: PathBuf,
    relay: Arc<dyn RelayHandler>,
) -> Result<Holders> {
    in_process_with(
        registry_dir,
        relay,
        env!("CARGO_PKG_VERSION"),
        Range::current(),
    )
}

/// [`in_process_relaying`], with the holders it starts reporting `holder_version` as theirs and
/// the daemon adopting those of `range`.
pub(crate) fn in_process_with(
    registry_dir: PathBuf,
    relay: Arc<dyn RelayHandler>,
    holder_version: &str,
    range: Range,
) -> Result<Holders> {
    let holder_version = holder_version.to_string();
    Holders::new(ClientConfig {
        registry_dir,
        exe: std::env::current_exe()?,
        builds: None,
        range,
        relay,
        launch: Box::new(move |_, holder_id, registry_dir| {
            let config = crate::holder::server::HolderConfig {
                registry_dir: registry_dir.to_path_buf(),
                holder_id: holder_id.to_string(),
                version: holder_version.clone(),
                build_id: "test-build".to_string(),
                exe: std::env::current_exe()?,
                first_link_deadline: Duration::from_secs(30),
                relay_deadline: Duration::from_secs(30),
            };
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("the holder's runtime");
                if let Err(err) = runtime.block_on(crate::holder::server::run(config)) {
                    tracing::warn!(%err, "the test holder failed");
                }
            });
            Ok(())
        }),
    })
}
