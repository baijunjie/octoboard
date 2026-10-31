//! Tests of the sandbox refusal.

use std::path::Path;

use super::sandbox::{sandbox_setting, saved_sandbox_profile};
use super::tests::{plan_isolated, with_default_home};
use super::*;
use crate::adapter::files::SMALL_FILE_LIMIT;
use crate::adapter::tests::{spec_fixture, SpecFixture};

/// Plans the launch and returns what refused it, `(source, profile)`, or `None` when it was not
/// refused. A refusal is also asserted to carry `GROK_SANDBOX_UNSUPPORTED` and to leave no farm.
fn refused_profile(fixture: &SpecFixture, resume: Option<&str>) -> Option<(String, String)> {
    let mut spec = fixture.spec();
    spec.resume_agent_session_id = resume;
    let farm = spec.scratch.join("grok-home");
    std::fs::remove_dir_all(&farm).ok();
    let result = plan_isolated(&spec);
    match result {
        Ok(_) => None,
        Err(err) => {
            let coded = err.downcast_ref::<CodedError>().expect("coded");
            assert_eq!(coded.code, error_code::GROK_SANDBOX_UNSUPPORTED);
            assert!(!farm.exists(), "a refusal builds no farm");
            Some((
                coded.params["source"].clone(),
                coded.params["profile"].clone(),
            ))
        }
    }
}

/// Writes the `summary.json` Grok keeps for conversation `id` with `profile` as its saved
/// sandbox profile, under a working-directory-named folder of the home's `sessions/`.
fn save_session_profile(home: &Path, id: &str, profile: &str) {
    let dir = home.join("sessions/%2Fsome%2Fcwd").join(id);
    std::fs::create_dir_all(&dir).expect("session directory");
    std::fs::write(
        dir.join("summary.json"),
        format!("{{\"sandbox_profile\":\"{profile}\"}}"),
    )
    .expect("summary");
}

const SAVED_ON: &str = "11111111-1111-4111-8111-111111111111";
const SAVED_OFF: &str = "22222222-2222-4222-8222-222222222222";
const NEVER_SAVED: &str = "33333333-3333-4333-8333-333333333333";

#[test]
fn a_launch_with_no_sandbox_anywhere_is_not_refused() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    assert_eq!(refused_profile(&fixture, None), None);
}

#[test]
fn a_sandbox_in_the_launch_environment_refuses_the_launch() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    fixture
        .shell_env
        .insert("GROK_SANDBOX".to_string(), "workspace".to_string());
    assert_eq!(
        refused_profile(&fixture, None),
        Some(("GROK_SANDBOX".to_string(), "workspace".to_string()))
    );
    fixture
        .shell_env
        .insert("GROK_SANDBOX".to_string(), "off".to_string());
    assert_eq!(refused_profile(&fixture, None), None);
}

#[test]
fn the_sandbox_profile_of_the_users_config_refuses_unless_it_is_off() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    std::fs::write(
        home.join("config.toml"),
        "[sandbox]\nprofile = \"strict\"\n",
    )
    .expect("config");
    assert_eq!(
        refused_profile(&fixture, None),
        Some((
            home.join("config.toml").to_string_lossy().into_owned(),
            "strict".to_string()
        ))
    );
    std::fs::write(home.join("config.toml"), "[sandbox]\nprofile = \"off\"\n").expect("config");
    assert_eq!(refused_profile(&fixture, None), None);
}

#[test]
fn an_off_environment_outranks_a_sandbox_in_the_users_config() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    std::fs::write(
        home.join("config.toml"),
        "[sandbox]\nprofile = \"workspace\"\n",
    )
    .expect("config");
    fixture
        .shell_env
        .insert("GROK_SANDBOX".to_string(), "off".to_string());
    assert_eq!(refused_profile(&fixture, None), None);
}

/// A FIFO at the user's or a system-wide config path counts as unreadable instead of holding the
/// launch until a writer appears.
#[test]
fn a_fifo_at_a_config_path_does_not_hold_the_launch() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let system = fixture.scratch.join("etc-grok");
    std::fs::create_dir_all(&system).expect("system directory");
    std::fs::remove_file(home.join("config.toml")).expect("config removed");
    for path in [
        home.join("config.toml"),
        system.join("requirements.toml"),
        system.join("managed_config.toml"),
    ] {
        let c = std::ffi::CString::new(path.into_os_string().into_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    }
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(plan_isolated(&fixture.spec()).is_ok());
    });
    assert_eq!(
        finished.recv_timeout(std::time::Duration::from_secs(10)),
        Ok(true),
        "the launch was held by a FIFO or did not plan"
    );
}

/// A config above its size bound counts as unreadable, so its sandbox profile is not read.
#[test]
fn an_oversized_config_is_ignored() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    std::fs::write(
        home.join("config.toml"),
        format!(
            "[sandbox]\nprofile = \"strict\"\n# {}\n",
            "x".repeat(SMALL_FILE_LIMIT)
        ),
    )
    .expect("config");
    assert_eq!(refused_profile(&fixture, None), None);
}

#[test]
fn a_config_that_is_not_toml_is_ignored() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    std::fs::write(home.join("config.toml"), "[sandbox\nprofile = ").expect("config");
    assert_eq!(refused_profile(&fixture, None), None);
}

#[test]
fn the_environment_outranks_the_users_config_and_the_config_outranks_the_managed_default() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let system = fixture.scratch.join("etc-grok");
    std::fs::create_dir_all(&system).expect("system directory");
    std::fs::write(
        system.join("managed_config.toml"),
        "[sandbox]\nprofile = \"workspace\"\n",
    )
    .expect("managed config");
    let check = |fixture: &SpecFixture| {
        sandbox_setting(&fixture.spec(), &home, &system).map(|found| found.profile)
    };
    assert_eq!(
        check(&fixture).as_deref(),
        Some("workspace"),
        "a managed default with no user setting"
    );
    std::fs::write(home.join("config.toml"), "[sandbox]\nprofile = \"off\"\n").expect("config");
    assert_eq!(check(&fixture), None, "the user's config wins");
    fixture
        .shell_env
        .insert("GROK_SANDBOX".to_string(), "strict".to_string());
    assert_eq!(
        check(&fixture).as_deref(),
        Some("strict"),
        "the environment wins"
    );
}

/// The home's own `requirements.toml` and `managed_config.toml` are excluded from the farm, so
/// Grok never loads them and they refuse nothing.
#[test]
fn the_homes_own_requirements_and_managed_config_are_not_read() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    for name in ["requirements.toml", "managed_config.toml"] {
        std::fs::write(home.join(name), "[sandbox]\nprofile = \"workspace\"\n").expect(name);
    }
    assert_eq!(refused_profile(&fixture, None), None);
}

#[test]
fn a_system_wide_requirements_pin_refuses_whatever_else_is_set() {
    let mut fixture = spec_fixture();
    with_default_home(&mut fixture);
    let system = fixture.scratch.join("etc-grok");
    std::fs::create_dir_all(&system).expect("system directory");
    std::fs::write(
        system.join("requirements.toml"),
        "[sandbox]\nprofile = \"workspace\"\n",
    )
    .expect("requirements");
    fixture
        .shell_env
        .insert("GROK_SANDBOX".to_string(), "off".to_string());
    assert_eq!(
        refused_profile(&fixture, None),
        Some((
            system
                .join("requirements.toml")
                .to_string_lossy()
                .into_owned(),
            "workspace".to_string()
        ))
    );
}

#[test]
fn a_system_wide_requirements_pin_and_managed_default_are_read() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let system = fixture.scratch.join("etc-grok");
    std::fs::create_dir_all(&system).expect("system directory");
    let check = |fixture: &SpecFixture| {
        sandbox_setting(&fixture.spec(), &home, &system).map(|found| found.profile)
    };
    assert_eq!(check(&fixture), None);

    std::fs::write(
        system.join("managed_config.toml"),
        "[sandbox]\nprofile = \"read-only\"\n",
    )
    .expect("managed config");
    assert_eq!(check(&fixture).as_deref(), Some("read-only"));

    // A requirements pin of `off` outranks a sandbox set anywhere below it.
    std::fs::write(
        system.join("requirements.toml"),
        "[sandbox]\nprofile = \"off\"\n",
    )
    .expect("requirements");
    assert_eq!(check(&fixture), None);
    std::fs::write(
        system.join("requirements.toml"),
        "[sandbox]\nprofile = \"custom\"\n",
    )
    .expect("requirements");
    assert_eq!(check(&fixture).as_deref(), Some("custom"));
}

#[test]
fn a_resumed_session_follows_the_profile_saved_with_it() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    save_session_profile(&home, SAVED_ON, "workspace");
    save_session_profile(&home, SAVED_OFF, "off");

    let saved = home
        .join("sessions/%2Fsome%2Fcwd")
        .join(SAVED_ON)
        .join("summary.json");
    assert_eq!(
        refused_profile(&fixture, Some(SAVED_ON)),
        Some((
            saved.to_string_lossy().into_owned(),
            "workspace".to_string()
        ))
    );
    assert_eq!(refused_profile(&fixture, Some(SAVED_OFF)), None);
    // A conversation with nothing saved falls back to the rules for a new session.
    assert_eq!(refused_profile(&fixture, Some(NEVER_SAVED)), None);

    // The saved profile decides over a configuration that says otherwise ...
    std::fs::write(
        home.join("config.toml"),
        "[sandbox]\nprofile = \"workspace\"\n",
    )
    .expect("config");
    assert_eq!(refused_profile(&fixture, Some(SAVED_OFF)), None);
    assert!(refused_profile(&fixture, Some(NEVER_SAVED)).is_some());
    // ... but not over a requirements pin.
    let system = fixture.scratch.join("etc-grok");
    std::fs::create_dir_all(&system).expect("system directory");
    std::fs::write(
        system.join("requirements.toml"),
        "[sandbox]\nprofile = \"strict\"\n",
    )
    .expect("requirements");
    assert!(refused_profile(&fixture, Some(SAVED_OFF)).is_some());
}

/// Grok refuses a resume whose requested profile differs from the saved one, so the launch
/// environment counts on a resume even when the saved profile is `off`.
#[test]
fn on_a_resume_either_the_saved_profile_or_the_environment_refuses() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    save_session_profile(&home, SAVED_ON, "workspace");
    save_session_profile(&home, SAVED_OFF, "off");

    fixture
        .shell_env
        .insert("GROK_SANDBOX".to_string(), "workspace".to_string());
    assert_eq!(
        refused_profile(&fixture, Some(SAVED_OFF)),
        Some(("GROK_SANDBOX".to_string(), "workspace".to_string()))
    );
    fixture
        .shell_env
        .insert("GROK_SANDBOX".to_string(), "off".to_string());
    assert_eq!(
        refused_profile(&fixture, Some(SAVED_ON)).map(|found| found.1),
        Some("workspace".to_string())
    );
    assert_eq!(refused_profile(&fixture, Some(SAVED_OFF)), None);
}

/// The id becomes a path component, so one that is not the shape of a Grok conversation id is
/// not looked up at all.
#[test]
fn an_implausible_resume_id_is_ignored_not_read() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let outside = fixture.scratch.join("outside");
    std::fs::create_dir_all(&outside).expect("outside directory");
    std::fs::write(
        outside.join("summary.json"),
        "{\"sandbox_profile\":\"workspace\"}",
    )
    .expect("summary");
    // Reached from a folder under `sessions/` as `../x`, and by an absolute path.
    std::fs::create_dir_all(home.join("sessions/%2Fsome%2Fcwd")).expect("sessions folder");
    std::fs::create_dir_all(home.join("sessions/x")).expect("x");
    std::fs::write(
        home.join("sessions/x/summary.json"),
        "{\"sandbox_profile\":\"workspace\"}",
    )
    .expect("summary");
    for id in ["../x".to_string(), outside.to_string_lossy().into_owned()] {
        assert_eq!(saved_sandbox_profile(&home, &id), None, "{id}");
        assert_eq!(refused_profile(&fixture, Some(&id)), None, "{id}");
    }
}

#[test]
fn a_summary_that_is_a_link_is_not_read() {
    let mut fixture = spec_fixture();
    let home = with_default_home(&mut fixture);
    let target = fixture.scratch.join("elsewhere.json");
    std::fs::write(&target, "{\"sandbox_profile\":\"workspace\"}").expect("target");
    let dir = home.join("sessions/%2Fsome%2Fcwd").join(SAVED_ON);
    std::fs::create_dir_all(&dir).expect("session directory");
    std::os::unix::fs::symlink(&target, dir.join("summary.json")).expect("link");
    assert_eq!(refused_profile(&fixture, Some(SAVED_ON)), None);
}
