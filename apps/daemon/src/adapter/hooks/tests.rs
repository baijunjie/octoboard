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
