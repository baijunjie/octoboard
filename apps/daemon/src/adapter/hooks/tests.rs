#[cfg(unix)]
use super::write_hook_script;
use crate::adapter::tests::spec_fixture;

#[cfg(unix)]
#[test]
fn the_hook_script_is_executable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = spec_fixture();
    let scratch = fixture.scratch.join("hook-script");
    std::fs::create_dir_all(&scratch).expect("scratch directory");
    let key_file = scratch.join("key");
    let path = write_hook_script(&scratch, "/bin/octoboardd", "s", "l", 1, &key_file, None)
        .expect("hook script");
    let mode = std::fs::metadata(&path)
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
    // The script names the file holding the key, never the key.
    let script = std::fs::read_to_string(&path).expect("script");
    assert!(
        script.contains(&format!("--key-file '{}'", key_file.display())),
        "{script}"
    );
    assert!(!script.contains("--key '"), "{script}");
    // And the launch it reports for.
    assert!(script.contains("--launch 'l'"), "{script}");
}

/// The guard's inline command line and its script both run the daemon's `guard-hook` mode over the
/// list and then exit 0 whatever it did: an agent reads exit status 2, which a command line the
/// mode cannot parse gives, as a block.
#[cfg(unix)]
#[test]
fn the_guard_command_and_script_exit_zero_whatever_the_mode_did() {
    use super::{guard_command, write_guard_script};
    use crate::adapter::GuardHook;
    use std::os::unix::fs::PermissionsExt;

    let fixture = spec_fixture();
    let scratch = fixture.scratch.join("guard-script");
    std::fs::create_dir_all(&scratch).expect("scratch directory");
    // Stands in for the daemon's executable: it prints what it was given and fails with 2.
    let exe = scratch.join("octoboardd with 'quote");
    std::fs::write(&exe, "#!/bin/sh\necho \"$@\"\necho noise >&2\nexit 2\n").expect("stand-in");
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let list = scratch.join("list.json");
    let exe_text = exe.to_string_lossy().into_owned();
    let guard = GuardHook {
        exe: &exe_text,
        list: &list,
        script: &scratch.join("guard-hook"),
    };
    let expected = format!("guard-hook --list {}\n", list.display());

    let line = std::process::Command::new("sh")
        .arg("-c")
        .arg(guard_command(&guard))
        .output()
        .unwrap();
    assert_eq!(line.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&line.stdout), expected);
    assert_eq!(line.stderr, b"", "stderr is dropped");

    let script = write_guard_script(&scratch, &exe_text, &list).expect("script");
    assert_eq!(script, scratch.join("guard-hook"));
    assert_eq!(
        std::fs::metadata(&script).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let run = std::process::Command::new(&script).output().unwrap();
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&run.stdout), expected);
    assert_eq!(run.stderr, b"", "stderr is dropped");
}

/// The marked guard command passes the mark as `--fired-by` after the list, quoted as one word,
/// and exits 0 whatever the mode did, as the plain one does.
#[cfg(unix)]
#[test]
fn the_marked_guard_command_names_its_mark_and_exits_zero() {
    use super::guard_command_fired_by;
    use crate::adapter::GuardHook;
    use std::os::unix::fs::PermissionsExt;

    let fixture = spec_fixture();
    let scratch = fixture.scratch.join("guard-marked");
    std::fs::create_dir_all(&scratch).expect("scratch directory");
    let exe = scratch.join("octoboardd");
    std::fs::write(&exe, "#!/bin/sh\nprintf '%s\\n' \"$@\"\nexit 2\n").expect("stand-in");
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let list = scratch.join("list.json");
    let exe_text = exe.to_string_lossy().into_owned();
    let guard = GuardHook {
        exe: &exe_text,
        list: &list,
        script: &scratch.join("guard-hook"),
    };

    let run = std::process::Command::new("sh")
        .arg("-c")
        .arg(guard_command_fired_by(
            &guard,
            "/run/it's mcp.json",
            super::Dialect::Posix,
        ))
        .output()
        .unwrap();

    assert_eq!(run.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        format!(
            "guard-hook\n--list\n{}\n--fired-by\n/run/it's mcp.json\n",
            list.display()
        )
    );
}

/// The Windows script is a batch file next to the path asked for, which ends with an exit of 0.
#[cfg(windows)]
#[test]
fn the_guard_batch_file_ends_with_an_exit_of_zero() {
    let fixture = crate::adapter::tests::spec_fixture();
    let scratch = fixture.scratch.join("guard-script");
    std::fs::create_dir_all(&scratch).expect("scratch directory");

    let script = super::write_guard_script(
        &scratch,
        "C:\\octoboard\\octoboardd.exe",
        &scratch.join("list.json"),
    )
    .expect("script");

    assert_eq!(script, scratch.join("guard-hook.cmd"));
    let text = std::fs::read_to_string(&script).expect("script");
    assert!(text.contains("guard-hook --list"), "{text}");
    assert!(text.trim_end().ends_with("exit /b 0"), "{text}");
}

/// The group the JSON-hook agents share: a synchronous command of its own after the status hooks'
/// group, with the guard's timeout and no `failClosed`.
#[test]
fn the_guard_group_is_added_after_the_status_group_of_pre_tool_use() {
    let mut hooks = serde_json::Map::new();
    hooks.insert(
        "PreToolUse".to_string(),
        serde_json::json!([{ "hooks": [{ "type": "command", "command": "status" }] }]),
    );
    hooks.insert("Stop".to_string(), serde_json::json!([{ "hooks": [] }]));

    super::add_guard_group(&mut hooks, "PreToolUse", "Edit|Write", "the-guard");

    assert_eq!(
        hooks["PreToolUse"],
        serde_json::json!([
            { "hooks": [{ "type": "command", "command": "status" }] },
            {
                "matcher": "Edit|Write",
                "hooks": [{ "type": "command", "command": "the-guard", "timeout": 10 }],
            },
        ])
    );
    assert_eq!(hooks["Stop"], serde_json::json!([{ "hooks": [] }]));
}

#[test]
fn the_guard_group_uses_the_event_the_provider_supplies() {
    let mut hooks = serde_json::Map::from_iter([
        ("BeforeEdit".to_string(), serde_json::json!([])),
        ("AfterEdit".to_string(), serde_json::json!([])),
    ]);
    super::add_guard_group(&mut hooks, "BeforeEdit", "Edit", "guard");
    assert_eq!(hooks["BeforeEdit"].as_array().unwrap().len(), 1);
    assert_eq!(hooks["BeforeEdit"][0]["hooks"][0]["command"], "guard");
    assert!(hooks["AfterEdit"].as_array().unwrap().is_empty());
}

/// The command registered for an agent whose payload names its event has no `--event`; one
/// registered per event names it, after the arguments every hook command has.
#[test]
fn a_hook_command_names_its_event_only_when_given_one() {
    let key_file = std::path::Path::new("/scratch/key");
    let without = super::hook_command("/bin/octoboardd", "s", "l", 1234, key_file, None);
    assert!(without.contains("hook"), "{without}");
    assert!(without.contains("--launch"), "{without}");
    assert!(!without.contains("--event"), "{without}");

    let with = super::hook_command(
        "/bin/octoboardd",
        "s",
        "l",
        1234,
        key_file,
        Some("PreToolUse"),
    );
    let (key_at, event_at) = (with.find("--key-file"), with.find("--event"));
    assert!(key_at.is_some() && event_at > key_at, "{with}");
    assert!(with.contains("PreToolUse"), "{with}");
    assert!(with.contains("--session"), "{with}");
}

/// A script of one event lives at `hook-<event>` with every character that is not an ASCII letter,
/// a digit, `-` or `_` replaced by `_`, one for one, so it is a safe file name.
#[test]
fn an_event_script_path_keeps_letters_digits_dashes_and_underscores_only() {
    let scratch = std::path::Path::new("/scratch");
    for (event, name) in [
        ("PreToolUse", "hook-PreToolUse"),
        ("agent-stop_2", "hook-agent-stop_2"),
        ("tool.use", "hook-tool_use"),
        ("a b/c\\d", "hook-a_b_c_d"),
        ("../x", "hook-___x"),
        ("caf\u{e9}", "hook-caf_"),
        ("", "hook-"),
    ] {
        assert_eq!(
            super::hook_event_script_path(scratch, event),
            scratch.join(name),
            "{event:?}"
        );
    }
    // The one script of an agent that names its event in every payload is not any event's.
    assert_eq!(super::hook_script_path(scratch), scratch.join("hook"));
}

/// With no event the script is the session's one `hook` and passes no `--event`; with one it is
/// the event's own file, which passes `--event`, so the scripts of several events of one session
/// sit side by side.
#[cfg(unix)]
#[test]
fn a_hook_script_of_an_event_is_its_own_file_and_passes_the_event() {
    let fixture = spec_fixture();
    let scratch = fixture.scratch.join("event-scripts");
    std::fs::create_dir_all(&scratch).expect("scratch directory");
    let key_file = scratch.join("key");
    let write = |event| {
        write_hook_script(&scratch, "/bin/octoboardd", "s", "l", 1, &key_file, event)
            .expect("script")
    };

    let plain = write(None);
    assert_eq!(plain, scratch.join("hook"));
    let plain_text = std::fs::read_to_string(&plain).expect("script");
    assert!(!plain_text.contains("--event"), "{plain_text}");

    let pre = write(Some("PreToolUse"));
    let odd = write(Some("tool use"));
    assert_eq!(pre, scratch.join("hook-PreToolUse"));
    assert_eq!(odd, scratch.join("hook-tool_use"));
    let pre_text = std::fs::read_to_string(&pre).expect("script");
    assert!(pre_text.contains("--event"), "{pre_text}");
    assert!(pre_text.contains("PreToolUse"), "{pre_text}");
    let odd_text = std::fs::read_to_string(&odd).expect("script");
    assert!(
        odd_text.contains("tool use"),
        "the name is passed as given: {odd_text}"
    );
    for script in [&plain, &pre, &odd] {
        assert!(script.exists(), "{}", script.display());
    }
    assert_eq!(
        std::fs::read_to_string(&plain).expect("script"),
        plain_text,
        "an event's script does not overwrite the session's one"
    );
}

/// The hook command of an agent whose hooks also run for an agent started inside the session
/// names the mark to the `hook` mode, and is the plain command otherwise; the plain command names
/// none.
#[test]
fn a_hook_command_names_its_mark_only_when_given_one() {
    use super::{hook_command, hook_command_fired_by};
    use std::path::Path;

    let key_file = Path::new("/run/s/key");
    let plain = hook_command("/bin/octoboardd", "s", "l", 1, key_file, Some("Stop"));
    let fixture = spec_fixture();
    let mut spec = fixture.spec();
    spec.key_file = key_file;
    let marked = hook_command_fired_by(&spec, "/run/s/mcp.json", super::Dialect::Posix);
    assert!(!plain.contains("--fired-by"), "{plain}");
    assert!(marked.contains("--fired-by"), "{marked}");
    assert!(marked.contains("/run/s/mcp.json"), "{marked}");
    assert!(marked.contains("/run/s/key"), "{marked}");
}
