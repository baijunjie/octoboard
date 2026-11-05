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
    let path =
        write_hook_script(&scratch, "/bin/octoboardd", "s", 1, "feedface").expect("hook script");
    let mode = std::fs::metadata(path)
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}
