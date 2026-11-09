//! What a Grok launch records for the daemon that adopts it after the one that started it is gone:
//! its trust carrier and its writeback of the settings a session changed are objects of that
//! process, so the data they are made of is kept with the launch and they are made again from it.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::trust::CarriedTrust;
use super::writeback::SharedConfigWriteback;
use crate::adapter::RebuiltLaunch;

/// What a Grok launch keeps in its terminal's meta to rebuild its trust carrier and write-back at
/// adoption, with the types it embeds ([`CarriedTrust`], [`SharedConfigWriteback`]).
/// It crosses builds: a daemon of an older build in the holder's compatible range writes it and a
/// newer one reads it, so a field added later needs `#[serde(default)]`, and it is kept readable
/// by every build in that range: `crate::holder::compat::FLOOR` only moves by the
/// previous-stable release rule.
#[derive(Serialize)]
struct Record<'a> {
    carried_trust: &'a CarriedTrust,
    writeback: Option<&'a SharedConfigWriteback>,
}

#[derive(Deserialize)]
struct Recorded {
    carried_trust: CarriedTrust,
    writeback: Option<SharedConfigWriteback>,
}

/// The record of a launch with `carried_trust` and, where its account shares its settings,
/// `writeback`.
pub(super) fn record(
    carried_trust: &CarriedTrust,
    writeback: Option<&SharedConfigWriteback>,
) -> Option<serde_json::Value> {
    serde_json::to_value(Record {
        carried_trust,
        writeback,
    })
    .map_err(|err| tracing::warn!(%err, "the Grok launch's adoption record could not be made"))
    .ok()
}

/// What a launch of this record had: nothing, when the record does not read.
pub(super) fn rebuild(record: &serde_json::Value) -> RebuiltLaunch {
    match serde_json::from_value::<Recorded>(record.clone()) {
        Ok(recorded) => RebuiltLaunch {
            carried_trust: Some(Arc::new(recorded.carried_trust)),
            after_exit: recorded
                .writeback
                .map(|writeback| Box::new(writeback) as Box<dyn crate::session::term::AfterExit>),
        },
        Err(err) => {
            tracing::warn!(%err, "the Grok launch's adoption record does not read");
            RebuiltLaunch::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use super::*;

    fn carried() -> CarriedTrust {
        CarriedTrust {
            session_store: "/session/trusted_folders.toml".into(),
            user_store: "/user/trusted_folders.toml".into(),
            folder: "/work".to_string(),
            record_wait: Duration::from_secs(5),
        }
    }

    #[test]
    fn the_trust_carrier_is_rebuilt_from_its_record() {
        let record = record(&carried(), None).expect("a record");
        let rebuilt = rebuild(&record);
        let carrier = rebuilt.carried_trust.expect("the trust is carried");
        assert_eq!(
            carrier.user_store(),
            Path::new("/user/trusted_folders.toml")
        );
        assert!(rebuilt.after_exit.is_none());
    }

    #[test]
    fn a_record_that_does_not_read_rebuilds_nothing() {
        let rebuilt = rebuild(&serde_json::json!({"carried_trust": 3}));
        assert!(rebuilt.carried_trust.is_none() && rebuilt.after_exit.is_none());
    }
}
