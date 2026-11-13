#[cfg(unix)]
use super::write_hook_script;
#[cfg(unix)]
use crate::adapter::tests::spec_fixture;

#[cfg(unix)]
#[test]
fn the_hook_script_is_executable_by_its_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = spec_fixture();
    let scratch = fixture.scratch.join("hook-script");
    std::fs::create_dir_all(&scratch).expect("scratch directory");
    let key_file = scratch.join("key");
    let path =
        write_hook_script(&scratch, "/bin/octoboardd", "s", 1, &key_file).expect("hook script");
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

    super::add_guard_group(&mut hooks, "Edit|Write", "the-guard");

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
