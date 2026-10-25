//! In-app updates: looking for a newer version on the user's chosen channel, downloading and
//! installing it on request, and the state that keeps those from overlapping.
//!
//! The UI owns when to look and what to say (`packages/ui/src/updates/`); this module only does
//! what needs the shell. Two commands cross IPC from here, both carrying display values:
//! `check_for_update` answers the version and notes of what it found, and `install_update` reports
//! download progress through a channel. Relaunching is not here: it goes through the exit flow's
//! `confirm_quit` (`exit.rs`), so the user is asked first about the sessions a restart ends.
//!
//! On macOS the installer replaces the `.app` in place while the application keeps running; the new
//! version takes effect on the next launch. The sidecar is spawned once at startup
//! (`sidecar.rs`), so until then the old UI stays paired with the old daemon, which is why nothing
//! here ever restarts the application by itself.
//!
//! Each channel is one manifest on a fixed-tag release of the public repository (see "In-app updates
//! and publishing a release" in `apps/desktop/README.md`). The endpoint is set per check from the chosen channel
//! rather than from `tauri.conf.json`, which carries only the signing key's public half.

use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

/// The public repository's address, from `config/app.json` (`build.rs`).
const REPOSITORY_URL: &str = env!("OCTOBOARD_REPOSITORY_URL");

/// The release tag the manifests live under, whatever the version they announce.
const MANIFEST_RELEASE_TAG: &str = "updater";

/// How long a check's request may take before it fails with a timeout rather than leaving the
/// Settings row on "Checking" for good.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// How long the download of what a check found may take as a whole. An update carries its check's
/// timeout over to the download, which a slow connection needs far longer than a manifest.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// The least a download reports between two progress messages, so a large archive arriving in
/// small chunks does not turn into thousands of IPC messages.
const PROGRESS_STEP_BYTES: u64 = 256 * 1024;

/// The release stream the user follows. Every release updates the beta manifest and only one
/// without a prerelease suffix also updates the stable one, so a beta user is offered stable
/// releases too.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    Stable,
    Beta,
}

impl UpdateChannel {
    fn manifest_name(self) -> &'static str {
        match self {
            Self::Stable => "stable.json",
            Self::Beta => "beta.json",
        }
    }
}

/// Where `channel`'s manifest is, under the repository at `repository_url`.
fn manifest_url(repository_url: &str, channel: UpdateChannel) -> String {
    format!(
        "{}/releases/download/{MANIFEST_RELEASE_TAG}/{}",
        repository_url.trim_end_matches('/'),
        channel.manifest_name()
    )
}

/// What a check found, in the words the UI shows.
#[derive(Serialize)]
pub struct AvailableUpdate {
    version: String,
    /// The release notes, when the manifest has any.
    notes: Option<String>,
}

/// How far a download has got.
#[derive(Clone, Serialize)]
pub struct DownloadProgress {
    downloaded: u64,
    /// The archive's size, when the server said.
    total: Option<u64>,
}

/// Why a check or an install did not go through. `Busy`, `Installed` and `NothingToInstall` are
/// refusals the UI prevents by what it offers, named so it can tell them from a real failure;
/// `Failed` carries the updater's own text, or what the update server answered
/// (`answer_to_missing_release`), which cannot be localized.
#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum UpdateError {
    /// A check or an install is already running.
    Busy,
    /// An update is installed and waits for a restart; nothing more is offered until then.
    Installed,
    /// `install_update` without a check that found something.
    NothingToInstall,
    Failed(String),
}

impl UpdateError {
    fn failed(err: impl ToString) -> Self {
        Self::Failed(err.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Checking,
    Installing,
    /// The new version is on disk; it runs after the next launch.
    Installed,
}

#[derive(Clone, Copy)]
enum Operation {
    Check,
    Install,
}

/// The phase `operation` moves `phase` to, or why it may not start now.
fn begin(phase: Phase, operation: Operation) -> Result<Phase, UpdateError> {
    match (phase, operation) {
        (Phase::Idle, Operation::Check) => Ok(Phase::Checking),
        (Phase::Idle, Operation::Install) => Ok(Phase::Installing),
        (Phase::Installed, _) => Err(UpdateError::Installed),
        (Phase::Checking | Phase::Installing, _) => Err(UpdateError::Busy),
    }
}

/// Whether a check's error only means there is nothing to update to: the release has no build for
/// this Mac's architecture. `ReleaseNotFound` is not one: the updater reports every non-success
/// status that way, and `answer_to_missing_release` tells a missing manifest from a failing server.
fn means_no_update(err: &tauri_plugin_updater::Error) -> bool {
    use tauri_plugin_updater::Error;
    matches!(err, Error::TargetNotFound(_) | Error::TargetsNotFound(_))
}

/// What a check answers when the manifest's address answered `status` and the updater found no
/// release in it: only a missing manifest (no release on that channel yet) means there is nothing to
/// update to; any other status is a server that failed, which the user must not read as "up to date".
fn answer_to_missing_release(status: reqwest::StatusCode) -> Result<Option<Update>, UpdateError> {
    if status == reqwest::StatusCode::NOT_FOUND {
        Ok(None)
    } else {
        Err(UpdateError::failed(format!(
            "the update server answered {status}"
        )))
    }
}

/// The status `url` answers a plain request with, following the redirect a release asset's address
/// goes through to the object storage.
async fn manifest_status(url: &tauri::Url) -> Result<reqwest::StatusCode, UpdateError> {
    let response = reqwest::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(UpdateError::failed)?
        .get(url.clone())
        .send()
        .await
        .map_err(UpdateError::failed)?;
    Ok(response.status())
}

struct Inner {
    phase: Phase,
    /// What the last check found, kept for `install_update`.
    found: Option<Update>,
}

pub struct UpdaterState(Mutex<Inner>);

impl Default for UpdaterState {
    fn default() -> Self {
        Self(Mutex::new(Inner {
            phase: Phase::Idle,
            found: None,
        }))
    }
}

impl UpdaterState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Puts the phase back to idle when an operation ends without having set the phase it ends in,
/// which only a panic does: otherwise it would stay on `Checking` or `Installing`, and every later
/// operation would be refused as busy.
struct PhaseGuard<'a>(&'a UpdaterState);

impl Drop for PhaseGuard<'_> {
    fn drop(&mut self) {
        let mut inner = self.0.lock();
        if matches!(inner.phase, Phase::Checking | Phase::Installing) {
            inner.phase = Phase::Idle;
        }
    }
}

/// Looks for an update on `channel`: answers what it found, or nothing when the version running is
/// the newest or the channel has no release yet.
#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
    state: tauri::State<'_, UpdaterState>,
    channel: UpdateChannel,
) -> Result<Option<AvailableUpdate>, UpdateError> {
    {
        let mut inner = state.lock();
        inner.phase = begin(inner.phase, Operation::Check)?;
    }
    let _guard = PhaseGuard(&state);
    let result = look_for_update(&app, channel).await;
    let mut inner = state.lock();
    inner.phase = Phase::Idle;
    match result {
        Ok(Some(update)) => {
            let found = AvailableUpdate {
                version: update.version.clone(),
                notes: update.body.clone().filter(|notes| !notes.trim().is_empty()),
            };
            inner.found = Some(update);
            Ok(Some(found))
        }
        Ok(None) => {
            inner.found = None;
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

async fn look_for_update(
    app: &AppHandle,
    channel: UpdateChannel,
) -> Result<Option<Update>, UpdateError> {
    let url: tauri::Url = manifest_url(REPOSITORY_URL, channel)
        .parse()
        .map_err(UpdateError::failed)?;
    let updater = app
        .updater_builder()
        .endpoints(vec![url.clone()])
        .map_err(UpdateError::failed)?
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(UpdateError::failed)?;
    match updater.check().await {
        Ok(update) => Ok(update.map(|mut update| {
            update.timeout = Some(DOWNLOAD_TIMEOUT);
            update
        })),
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => {
            // After the check, which is what installs the TLS provider this request relies on.
            answer_to_missing_release(manifest_status(&url).await?)
        }
        Err(err) if means_no_update(&err) => Ok(None),
        Err(err) => Err(UpdateError::failed(err)),
    }
}

/// Downloads and installs what the last check found, reporting progress through `on_progress`.
/// Resolves once the new version is in place; it runs after the next launch, which the UI offers
/// but never forces.
#[tauri::command]
pub async fn install_update(
    state: tauri::State<'_, UpdaterState>,
    on_progress: Channel<DownloadProgress>,
) -> Result<(), UpdateError> {
    let update = {
        let mut inner = state.lock();
        let next = begin(inner.phase, Operation::Install)?;
        let Some(update) = inner.found.clone() else {
            return Err(UpdateError::NothingToInstall);
        };
        inner.phase = next;
        update
    };
    let _guard = PhaseGuard(&state);

    let mut downloaded = 0u64;
    let mut reported = 0u64;
    let result = update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                if downloaded - reported >= PROGRESS_STEP_BYTES || Some(downloaded) == total {
                    reported = downloaded;
                    let _ = on_progress.send(DownloadProgress { downloaded, total });
                }
            },
            || {},
        )
        .await;

    let mut inner = state.lock();
    match result {
        Ok(()) => {
            inner.phase = Phase::Installed;
            inner.found = None;
            Ok(())
        }
        Err(err) => {
            // The found update stays, so an install that failed can be tried again.
            inner.phase = Phase::Idle;
            Err(UpdateError::failed(err))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_channel_reads_its_own_manifest_on_the_updater_release() {
        let repository = "https://github.com/baijunjie/octoboard";
        assert_eq!(
            manifest_url(repository, UpdateChannel::Stable),
            "https://github.com/baijunjie/octoboard/releases/download/updater/stable.json"
        );
        assert_eq!(
            manifest_url(repository, UpdateChannel::Beta),
            "https://github.com/baijunjie/octoboard/releases/download/updater/beta.json"
        );
    }

    #[test]
    fn a_repository_address_with_a_trailing_slash_gives_the_same_url() {
        assert_eq!(
            manifest_url("https://example.test/a/b/", UpdateChannel::Beta),
            manifest_url("https://example.test/a/b", UpdateChannel::Beta)
        );
    }

    #[test]
    fn the_built_in_repository_address_is_a_valid_https_endpoint() {
        for channel in [UpdateChannel::Stable, UpdateChannel::Beta] {
            let url: tauri::Url = manifest_url(REPOSITORY_URL, channel).parse().unwrap();
            assert_eq!(url.scheme(), "https");
        }
    }

    #[test]
    fn a_channel_is_named_in_lower_case_over_ipc() {
        let stable: UpdateChannel = serde_json::from_str("\"stable\"").unwrap();
        let beta: UpdateChannel = serde_json::from_str("\"beta\"").unwrap();
        assert_eq!((stable, beta), (UpdateChannel::Stable, UpdateChannel::Beta));
        assert!(serde_json::from_str::<UpdateChannel>("\"nightly\"").is_err());
    }

    #[test]
    fn an_operation_starts_only_while_idle() {
        assert_eq!(begin(Phase::Idle, Operation::Check), Ok(Phase::Checking));
        assert_eq!(
            begin(Phase::Idle, Operation::Install),
            Ok(Phase::Installing)
        );
        for phase in [Phase::Checking, Phase::Installing] {
            assert_eq!(begin(phase, Operation::Check), Err(UpdateError::Busy));
            assert_eq!(begin(phase, Operation::Install), Err(UpdateError::Busy));
        }
    }

    #[test]
    fn nothing_more_is_offered_once_an_update_is_installed() {
        assert_eq!(
            begin(Phase::Installed, Operation::Check),
            Err(UpdateError::Installed)
        );
        assert_eq!(
            begin(Phase::Installed, Operation::Install),
            Err(UpdateError::Installed)
        );
    }

    #[test]
    fn a_missing_build_means_no_update() {
        use tauri_plugin_updater::Error;
        assert!(means_no_update(&Error::TargetNotFound(
            "darwin-aarch64".into()
        )));
        assert!(means_no_update(&Error::TargetsNotFound(vec![])));
        assert!(!means_no_update(&Error::ReleaseNotFound));
        assert!(!means_no_update(&Error::EmptyEndpoints));
        assert!(!means_no_update(&Error::Network("503".into())));
    }

    #[test]
    fn only_a_missing_manifest_means_no_update_to_a_status() {
        use reqwest::StatusCode;
        assert!(matches!(
            answer_to_missing_release(StatusCode::NOT_FOUND),
            Ok(None)
        ));
        for status in [
            StatusCode::FORBIDDEN,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::SERVICE_UNAVAILABLE,
        ] {
            assert_eq!(
                answer_to_missing_release(status).err(),
                Some(UpdateError::failed(format!(
                    "the update server answered {status}"
                )))
            );
        }
    }

    #[test]
    fn a_panicking_operation_does_not_leave_the_phase_busy() {
        let state = UpdaterState::default();
        state.lock().phase = Phase::Checking;
        drop(PhaseGuard(&state));
        assert_eq!(state.lock().phase, Phase::Idle);
    }

    #[test]
    fn an_operation_that_ended_normally_keeps_its_phase() {
        let state = UpdaterState::default();
        state.lock().phase = Phase::Installed;
        drop(PhaseGuard(&state));
        assert_eq!(state.lock().phase, Phase::Installed);
    }

    #[test]
    fn an_error_reaches_the_page_as_a_kind_and_its_detail() {
        assert_eq!(
            serde_json::to_string(&UpdateError::Busy).unwrap(),
            r#"{"kind":"busy"}"#
        );
        assert_eq!(
            serde_json::to_string(&UpdateError::failed("offline")).unwrap(),
            r#"{"kind":"failed","detail":"offline"}"#
        );
    }
}
