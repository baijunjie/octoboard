use std::collections::HashMap;
use std::time::Duration;

use zbus::proxy::MethodFlags;
use zbus::zvariant::OwnedObjectPath;

use super::Presence;

const SERVICE: &str = "org.freedesktop.secrets";
const PATH: &str = "/org/freedesktop/secrets";
const INTERFACE: &str = "org.freedesktop.Secret.Service";

type Matches = (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>);

pub(super) fn lookup(service: &str, account: &str, timeout: Duration) -> Presence {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Presence::CannotTell;
    };
    runtime.block_on(async {
        let reply = tokio::time::timeout(timeout, search(service, account)).await;
        presence_of(reply.ok().and_then(Result::ok).flatten())
    })
}

async fn search(service: &str, account: &str) -> zbus::Result<Option<Matches>> {
    let connection = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(&connection, SERVICE, PATH, INTERFACE).await?;
    let attributes = HashMap::from([("service", service), ("username", account)]);
    proxy
        .call_with_flags("SearchItems", MethodFlags::NoAutoStart.into(), &attributes)
        .await
}

fn presence_of(matches: Option<Matches>) -> Presence {
    match matches {
        Some((unlocked, locked)) if unlocked.is_empty() && locked.is_empty() => Presence::Absent,
        Some(_) => Presence::Present,
        None => Presence::CannotTell,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_items_counts_both_unlocked_and_locked_matches() {
        let item = || OwnedObjectPath::try_from("/org/freedesktop/secrets/item").unwrap();
        for (matches, expected) in [
            (Some((vec![item()], vec![])), Presence::Present),
            (Some((vec![], vec![item()])), Presence::Present),
            (Some((vec![], vec![])), Presence::Absent),
            (None, Presence::CannotTell),
        ] {
            assert_eq!(presence_of(matches), expected);
        }
    }
}
