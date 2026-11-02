//! The width of a pass of blocking per-entity work, and the runner that holds a pass to it.
//! [`stalled`] keeps the blocking work that outran its deadline and whose thread is still running;
//! [`within`] runs a pass for a synchronous caller, waited for only until a deadline.

use std::sync::Arc;

mod stalled;
mod within;

pub use stalled::StalledPasses;
pub use within::{within_deadline, WithinDeadline};

/// How many blocking per-entity items run at once, in every pass that starts one blocking thread
/// per project, directory or path. The blocking pool is one pool for the whole daemon and every
/// user-visible path shares it, and such an item usually also waits on `env_shell`'s snapshot lock,
/// held for a whole login-shell fork while the cache is cold. A handful keeps a pass quick when its
/// items are fast, and an item that stalls holds one of these places and no more.
pub const BLOCKING_WIDTH: usize = 4;

/// Runs `work` over `items` on blocking threads, at most [`BLOCKING_WIDTH`] at a time, and
/// returns what each returned; an item whose thread panicked is absent. The next item starts as
/// soon as any one finishes, so one that hangs holds back only its own place.
pub async fn each_blocking<I, T>(
    items: Vec<I>,
    work: impl Fn(I) -> T + Send + Sync + 'static,
) -> Vec<T>
where
    I: Send + 'static,
    T: Send + 'static,
{
    let work = Arc::new(work);
    let mut running = tokio::task::JoinSet::new();
    let mut done = Vec::new();
    for item in items {
        if running.len() >= BLOCKING_WIDTH {
            if let Some(Ok(result)) = running.join_next().await {
                done.push(result);
            }
        }
        let work = work.clone();
        running.spawn_blocking(move || work(item));
    }
    while let Some(result) = running.join_next().await {
        if let Ok(result) = result {
            done.push(result);
        }
    }
    done
}
