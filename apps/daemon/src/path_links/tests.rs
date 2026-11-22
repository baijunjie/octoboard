#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use super::resolve::{absolute_spelling, names_directory, reach, refused_as_network, Whereabouts};
use super::*;
use crate::hostfs::lexically_normalise;
use crate::protocol::PathLinkView;
use crate::test_support::{symlink, ScratchDir};

fn canonical(dir: &Path) -> PathBuf {
    crate::hostfs::canonical(dir).unwrap()
}

/// Makes [`temp_roots`] answer `dirs` instead of the host's, on the test's own thread, until dropped.
struct TempRootsOverride;

fn with_temp_roots(dirs: &[&Path]) -> TempRootsOverride {
    TEMP_ROOTS_OVERRIDE.set(Some(dirs.iter().map(|dir| dir.to_path_buf()).collect()));
    TempRootsOverride
}

impl Drop for TempRootsOverride {
    fn drop(&mut self) {
        TEMP_ROOTS_OVERRIDE.set(None);
    }
}

fn project_roots(dirs: &[&Path]) -> ProjectRoots {
    let canonical: Vec<PathBuf> = dirs.iter().map(|dir| self::canonical(dir)).collect();
    ProjectRoots {
        lexical: canonical.clone(),
        worktrees: vec![None; canonical.len()],
        canonical,
    }
}

/// A temp root that is `dir` alone.
fn temp_of(dir: &Path) -> TempRoots {
    TempRoots {
        canonical: vec![canonical(dir)],
        lexical: vec![canonical(dir)],
        check_owner: true,
    }
}

fn no_temp() -> TempRoots {
    TempRoots {
        canonical: vec![],
        lexical: vec![],
        check_owner: true,
    }
}

fn kind(
    raw: &str,
    home: Option<&Path>,
    temp: &TempRoots,
    roots: &ProjectRoots,
) -> Option<PathLinkKind> {
    resolve(raw, home, temp, &|| roots.clone()).map(|resolved| resolved.kind)
}

#[test]
fn project_paths_resolve_absolute_relative_and_home_relative() {
    let project = ScratchDir::new("path-links-project");
    std::fs::create_dir(project.join("src")).unwrap();
    std::fs::write(project.join("src/a.ts"), "x").unwrap();
    let roots = project_roots(&[&project]);
    let base = roots.canonical[0].clone();
    let at = |raw: &str| kind(raw, Some(&base), &no_temp(), &roots);
    assert_eq!(at("src/a.ts"), Some(PathLinkKind::File));
    assert_eq!(
        at(base.join("src/a.ts").to_str().unwrap()),
        Some(PathLinkKind::File)
    );
    assert_eq!(at("~/src/a.ts"), Some(PathLinkKind::File));
    assert_eq!(at("src"), Some(PathLinkKind::Directory));
    assert_eq!(at("src/missing.ts"), None);
}

#[test]
fn a_relative_path_needs_a_project() {
    let project = ScratchDir::new("path-links-no-project");
    std::fs::write(project.join("a.txt"), "x").unwrap();
    assert_eq!(
        kind("a.txt", None, &no_temp(), &ProjectRoots::default()),
        None
    );
}

#[test]
fn a_relative_path_found_in_two_roots_is_ambiguous() {
    let first = ScratchDir::new("path-links-first");
    let second = ScratchDir::new("path-links-second");
    std::fs::write(first.join("both.ts"), "x").unwrap();
    std::fs::write(second.join("both.ts"), "x").unwrap();
    std::fs::write(second.join("only.ts"), "x").unwrap();
    let roots = project_roots(&[&first, &second]);
    assert_eq!(kind("both.ts", None, &no_temp(), &roots), None);
    // Alone in one root it is that root's, and an absolute path is never ambiguous.
    assert_eq!(
        kind("only.ts", None, &no_temp(), &roots),
        Some(PathLinkKind::File)
    );
    let absolute = canonical(&first).join("both.ts");
    assert_eq!(
        kind(absolute.to_str().unwrap(), None, &no_temp(), &roots),
        Some(PathLinkKind::File)
    );
}

#[test]
fn a_path_outside_every_root_is_reachable_where_it_resolves_even_through_a_link() {
    let project = ScratchDir::new("path-links-outside");
    let elsewhere = ScratchDir::new("path-links-elsewhere");
    std::fs::write(elsewhere.join("secret.txt"), "x").unwrap();
    std::fs::create_dir(elsewhere.join("folder")).unwrap();
    symlink(elsewhere.join("secret.txt"), project.join("link.txt")).unwrap();
    symlink(elsewhere.join("folder"), project.join("folder-link")).unwrap();
    let roots = project_roots(&[&project]);
    let outside = canonical(&elsewhere);
    let inside = canonical(&project);
    // Written as it lies, through the project's own link to it, written absolute or relative, and
    // spelled inside the project: it is judged by where it resolves to, so it is not the project's.
    for (raw, kind, target) in [
        (
            outside.join("secret.txt"),
            PathLinkKind::File,
            outside.join("secret.txt"),
        ),
        (
            outside.join("folder"),
            PathLinkKind::Directory,
            outside.join("folder"),
        ),
        (
            inside.join("link.txt"),
            PathLinkKind::File,
            outside.join("secret.txt"),
        ),
        (
            inside.join("folder-link"),
            PathLinkKind::Directory,
            outside.join("folder"),
        ),
        (
            PathBuf::from("link.txt"),
            PathLinkKind::File,
            outside.join("secret.txt"),
        ),
        (
            PathBuf::from("folder-link"),
            PathLinkKind::Directory,
            outside.join("folder"),
        ),
    ] {
        let raw = raw.to_str().unwrap();
        let resolved = reach(raw, None, &no_temp(), &|| roots.clone())
            .unwrap_or_else(|unreached| panic!("{raw}: {unreached:?}"));
        assert_eq!(
            (resolved.kind, resolved.path, resolved.project_root),
            (kind, target, None),
            "{raw}"
        );
    }
}

#[test]
fn a_path_anywhere_needs_no_project() {
    let elsewhere = ScratchDir::new("path-links-anywhere");
    std::fs::write(elsewhere.join("a.txt"), "x").unwrap();
    let home = canonical(&elsewhere);
    // As written, and from a `~/` path of the host's home directory.
    for raw in [home.join("a.txt").to_str().unwrap(), "~/a.txt"] {
        let resolved = reach(raw, Some(&home), &no_temp(), &ProjectRoots::default).unwrap();
        assert_eq!(
            (resolved.kind, resolved.launchable, resolved.project_root),
            (PathLinkKind::File, false, None),
            "{raw}"
        );
    }
}

fn refusal(raw: &str, temp: &TempRoots, roots: &ProjectRoots) -> Refusal {
    match reach(raw, None, temp, &|| roots.clone()) {
        Err(Unreached::Refused(refusal)) => refusal,
        other => panic!("{raw}: expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_refusal_says_why_the_path_is_not_shown() {
    let project = ScratchDir::new("path-links-refusal");
    let elsewhere = ScratchDir::new("path-links-refusal-elsewhere");
    let second = ScratchDir::new("path-links-refusal-second");
    std::fs::write(elsewhere.join("secret.txt"), "x").unwrap();
    symlink(elsewhere.join("secret.txt"), project.join("link.txt")).unwrap();
    std::fs::write(project.join("both.ts"), "x").unwrap();
    std::fs::write(second.join("both.ts"), "x").unwrap();
    #[cfg(unix)]
    {
        std::fs::write(project.join("tool"), "x").unwrap();
        std::fs::set_permissions(project.join("tool"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        crate::test_support::mkfifo(&project.join("pipe"));
    }
    std::fs::create_dir(project.join("Thing.app")).unwrap();
    let roots = project_roots(&[&project, &second]);
    let at = |raw: &str| refusal(raw, &no_temp(), &roots);
    // A relative path with no root to look from names nothing.
    assert_eq!(
        refusal("a.txt", &no_temp(), &ProjectRoots::default()),
        Refusal::Other
    );
    assert_eq!(at("both.ts"), Refusal::Ambiguous);
    // A launchable file is reachable anywhere, a launchable folder nowhere.
    assert_eq!(at("Thing.app"), Refusal::Launchable);
    std::fs::create_dir(elsewhere.join("Other.app")).unwrap();
    assert_eq!(
        refusal(
            canonical(&elsewhere).join("Other.app").to_str().unwrap(),
            &no_temp(),
            &ProjectRoots::default()
        ),
        Refusal::Launchable
    );
    assert_eq!(
        refusal(
            canonical(&project).join("Thing.app").to_str().unwrap(),
            &temp_of(&project),
            &ProjectRoots::default()
        ),
        Refusal::Launchable
    );
    #[cfg(unix)]
    {
        let absolute = canonical(&project);
        assert_eq!(
            refusal(absolute.join("pipe").to_str().unwrap(), &no_temp(), &roots),
            Refusal::NotAFileOrFolder
        );
    }
}

#[cfg(unix)]
#[test]
fn a_temp_path_others_can_change_is_refused_as_not_private() {
    let temp = ScratchDir::new("path-links-not-private");
    std::fs::write(temp.join("writable.txt"), "x").unwrap();
    std::fs::set_permissions(
        temp.join("writable.txt"),
        std::fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    let path = canonical(&temp).join("writable.txt");
    assert_eq!(
        refusal(
            path.to_str().unwrap(),
            &temp_of(&temp),
            &ProjectRoots::default()
        ),
        Refusal::NotPrivate
    );
}

#[cfg(unix)]
#[test]
fn a_denied_read_is_refused_with_the_systems_text() {
    // SAFETY: a plain query of this process's effective user.
    if unsafe { libc::geteuid() } == 0 {
        // Permissions do not stop the superuser, so there is nothing to refuse.
        return;
    }
    let project = ScratchDir::new("path-links-denied");
    std::fs::create_dir(project.join("locked")).unwrap();
    std::fs::write(project.join("locked/a.txt"), "x").unwrap();
    let chmod = |mode| {
        std::fs::set_permissions(
            project.join("locked"),
            std::fs::Permissions::from_mode(mode),
        )
        .unwrap()
    };
    chmod(0o000);
    let roots = project_roots(&[&project]);
    let absolute = canonical(&project).join("locked/a.txt");
    // Looked at without panicking, so the mode is restored (and the scratch directory deletable) whatever it finds.
    let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        [
            refusal("locked/a.txt", &no_temp(), &roots),
            refusal(absolute.to_str().unwrap(), &no_temp(), &roots),
            // Outside every root the system's refusal is still the answer.
            refusal(
                absolute.to_str().unwrap(),
                &no_temp(),
                &ProjectRoots::default(),
            ),
        ]
    }));
    chmod(0o755);
    for refusal in found.expect("looking at the path panicked") {
        let Refusal::Denied(detail) = &refusal else {
            panic!("{refusal:?}");
        };
        assert!(!detail.is_empty());
    }
}

#[test]
fn a_file_in_a_worktree_below_the_project_is_found_in_the_innermost_root() {
    let outer = ScratchDir::new("path-links-nested");
    std::fs::create_dir_all(outer.join("tree")).unwrap();
    std::fs::write(outer.join("tree/a.ts"), "x").unwrap();
    let mut roots = project_roots(&[&outer, &outer.join("tree")]);
    roots.worktrees[1] = Some("wt".into());
    let absolute = canonical(&outer).join("tree/a.ts");
    let resolved = resolve(absolute.to_str().unwrap(), None, &no_temp(), &|| {
        roots.clone()
    })
    .unwrap();
    assert_eq!(resolved.project_root, Some(1));
}

#[test]
fn what_could_be_launched_is_never_opened_and_only_a_file_is_kept_to_be_viewed() {
    let dir = ScratchDir::new("path-links-launchable");
    for name in [
        "sheet.png",
        "run.command",
        "Loud.COMMAND",
        #[cfg(unix)]
        "tool",
        "x.fileloc",
        "tool.py",
        "setup.EXE",
        "App.AppImage",
        "pkg.deb",
        "start.jnlp",
        "run.sh.bak.run",
    ] {
        std::fs::write(dir.join(name), "x").unwrap();
    }
    #[cfg(unix)]
    std::fs::set_permissions(dir.join("tool"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(dir.join("Thing.app/Contents")).unwrap();
    std::fs::write(dir.join("Thing.app/Contents/info.txt"), "x").unwrap();
    let roots = project_roots(&[&dir]);
    let launchable = [
        "run.command",
        "Loud.COMMAND",
        #[cfg(unix)]
        "tool",
        "x.fileloc",
        "tool.py",
        "setup.EXE",
        "App.AppImage",
        "pkg.deb",
        "start.jnlp",
        "run.sh.bak.run",
        "Thing.app/Contents/info.txt",
    ];
    // In a temp root alone the directory is scratch space; as the project's it is the project's.
    // Anywhere else on the host every folder from the top of the file system counts.
    for (temp, roots, scope) in [
        (no_temp(), roots, "project"),
        (temp_of(&dir), ProjectRoots::default(), "temp"),
        (no_temp(), ProjectRoots::default(), "host"),
    ] {
        let at = |name: &str| {
            let path = canonical(&dir).join(name);
            resolve(path.to_str().unwrap(), None, &temp, &|| roots.clone())
        };
        assert_eq!(
            at("sheet.png").map(|r| (r.kind, r.launchable)),
            Some((PathLinkKind::File, false)),
            "{scope}"
        );
        for name in launchable {
            // A file is kept for the viewer, in a project, a temp root or anywhere else, marked so that
            // nothing opens it.
            assert_eq!(
                at(name).map(|r| (r.kind, r.launchable)),
                Some((PathLinkKind::File, true)),
                "{scope}: {name}"
            );
        }
        // A bundle is a folder to open, so it is never reachable.
        assert_eq!(at("Thing.app").map(|r| r.kind), None, "{scope}");
    }
}

/// The lists moved out of this crate's constants into `config/launchable-extensions.json`; the
/// move must not have added or lost an entry.
#[test]
fn the_shared_lists_hold_what_the_constants_held() {
    let lists = crate::platform::open::launchable_extensions();
    let ends = |list: &[String]| (list.len(), list[0].clone(), list[list.len() - 1].clone());
    assert_eq!(
        ends(&lists.any_component),
        (41, "app".into(), "shortcut".into())
    );
    assert_eq!(ends(&lists.final_file), (35, "py".into(), "pif".into()));
    assert_eq!(
        ends(&lists.windows_final_file),
        (46, "lnk".into(), "vsmacros".into())
    );
    assert_eq!(
        crate::platform::open::launched_file_extensions().len(),
        if cfg!(windows) { 46 } else { 0 }
    );
}

#[test]
fn an_interpreter_or_installer_extension_counts_only_on_a_regular_files_own_name() {
    let dir = ScratchDir::new("path-links-final-extension");
    for folder in ["vendor/github.com", "pkg.py/inner", "setup.exe/inner"] {
        std::fs::create_dir_all(dir.join(folder)).unwrap();
        std::fs::write(dir.join(folder).join("notes.txt"), "x").unwrap();
    }
    let files = [
        "a.jse", "a.WSH", "a.hta", "a.reg", "a.msp", "a.cpl", "a.pif",
        // What follows the last dot counts even when the dot leads the name: the opener takes it for the type.
        ".bat", ".EXE", ".cmd", ".hta",
    ];
    for name in files {
        std::fs::write(dir.join(name), "x").unwrap();
    }
    std::fs::write(dir.join("a.js"), "x").unwrap();
    std::fs::write(dir.join("a.lnk"), "x").unwrap();
    let roots = project_roots(&[&dir]);
    let at = |name: &str| {
        let path = canonical(&dir).join(name);
        resolve(path.to_str().unwrap(), None, &no_temp(), &|| roots.clone())
    };
    for folder in ["vendor/github.com", "pkg.py", "setup.exe"] {
        assert_eq!(at(folder).map(|r| r.kind), Some(PathLinkKind::Directory));
    }
    for file in [
        "vendor/github.com/notes.txt",
        "pkg.py/inner/notes.txt",
        "setup.exe/inner/notes.txt",
    ] {
        assert_eq!(
            at(file).map(|r| (r.kind, r.launchable)),
            Some((PathLinkKind::File, false)),
            "{file}"
        );
    }
    // Windows runs a `.js` file through Windows Script Host (`PATHEXT` lists it); elsewhere it is
    // text an editor opens.
    assert_eq!(
        at("a.js").map(|r| (r.kind, r.launchable)),
        Some((PathLinkKind::File, cfg!(windows)))
    );
    for file in files {
        assert_eq!(at(file).map(|r| r.launchable), Some(true), "{file}");
    }
    // Explorer follows a shortcut; elsewhere a `.lnk` file is data.
    assert_eq!(
        at("a.lnk").map(|r| (r.kind, r.launchable)),
        Some((PathLinkKind::File, cfg!(windows)))
    );
}

/// The types the host's shell launches are refused like any other: a file of one is kept for the
/// viewer in a project and unreachable in a temp root, and as a folder name one launches nothing.
#[test]
fn the_hosts_own_launched_types_are_refused_on_a_files_own_name() {
    let dir = ScratchDir::new("path-links-host-launched");
    for extension in crate::platform::open::launched_file_extensions() {
        std::fs::write(
            dir.join(format!("a.{}", extension.to_ascii_uppercase())),
            "x",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join(format!("d.{extension}"))).unwrap();
    }
    let roots = project_roots(&[&dir]);
    for extension in crate::platform::open::launched_file_extensions() {
        let file = canonical(&dir).join(format!("a.{}", extension.to_ascii_uppercase()));
        let folder = canonical(&dir).join(format!("d.{extension}"));
        let in_project =
            |path: &Path| resolve(path.to_str().unwrap(), None, &no_temp(), &|| roots.clone());
        assert_eq!(
            in_project(&file).map(|r| (r.kind, r.launchable)),
            Some((PathLinkKind::File, true)),
            "{extension}"
        );
        assert_eq!(
            in_project(&folder).map(|r| (r.kind, r.launchable)),
            Some((PathLinkKind::Directory, false)),
            "{extension}"
        );
        let in_temp = resolve(
            file.to_str().unwrap(),
            None,
            &temp_of(&dir),
            &ProjectRoots::default,
        );
        assert_eq!(in_temp, None, "{extension}");
        // Anywhere else on the host it is kept for the viewer like a project's.
        let elsewhere = resolve(
            file.to_str().unwrap(),
            None,
            &no_temp(),
            &ProjectRoots::default,
        );
        assert_eq!(
            elsewhere.map(|r| (r.kind, r.launchable)),
            Some((PathLinkKind::File, true)),
            "{extension}"
        );
    }
}

#[test]
fn a_directory_shows_its_readme_else_its_index_file() {
    let entry = |names: &[&str]| {
        let dir = ScratchDir::new("path-links-entry");
        for name in names {
            std::fs::write(dir.join(name), "x").unwrap();
        }
        entry_file(&dir).map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
    };
    assert_eq!(entry(&[]), None);
    assert_eq!(entry(&["a.md", "READMEFIRST.md", "indexer.js"]), None);
    // A README beats an index, and case does not matter.
    assert_eq!(
        entry(&["index.md", "readme.TXT"]).as_deref(),
        Some("readme.TXT")
    );
    // `.md` first, then `.markdown`, then by name, ignoring case.
    assert_eq!(
        entry(&["README.txt", "README", "README.markdown", "Readme.MD"]).as_deref(),
        Some("Readme.MD")
    );
    assert_eq!(
        entry(&["README.txt", "README", "README.markdown"]).as_deref(),
        Some("README.markdown")
    );
    assert_eq!(entry(&["README.txt", "README"]).as_deref(), Some("README"));
    assert_eq!(
        entry(&["index.ts", "index.html", "INDEX.md", "index.css"]).as_deref(),
        Some("INDEX.md")
    );
    assert_eq!(
        entry(&["index.ts", "index.html", "index.css"]).as_deref(),
        Some("index.css")
    );
    // `index.` alone names nothing.
    assert_eq!(entry(&["index."]), None);
}

#[test]
fn only_a_regular_file_directly_in_the_directory_is_its_entry() {
    let dir = ScratchDir::new("path-links-entry-kinds");
    std::fs::create_dir(dir.join("README.md")).unwrap();
    std::fs::create_dir(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/README.md"), "x").unwrap();
    std::fs::write(dir.join("outside.md"), "x").unwrap();
    symlink(dir.join("outside.md"), dir.join("index.md")).unwrap();
    assert_eq!(entry_file(&dir), None);
    assert_eq!(entry_file(&dir.join("missing")), None);
}

#[test]
fn an_absolute_path_is_judged_by_where_it_resolves_not_by_how_it_is_written() {
    let project = ScratchDir::new("path-links-spelled");
    let elsewhere = ScratchDir::new("path-links-spelled-link");
    std::fs::write(project.join("a.ts"), "x").unwrap();
    symlink(&*project, elsewhere.join("alias")).unwrap();
    let roots = project_roots(&[&project]);
    // Written outside the project, through a link into it: the project's file.
    let written = canonical(&elsewhere).join("alias/a.ts");
    let resolved = reach(written.to_str().unwrap(), None, &no_temp(), &|| {
        roots.clone()
    })
    .unwrap();
    assert_eq!(
        (resolved.kind, resolved.path, resolved.project_root),
        (
            PathLinkKind::File,
            canonical(&project).join("a.ts"),
            Some(0)
        )
    );
    let direct = canonical(&project).join("a.ts");
    assert_eq!(
        kind(direct.to_str().unwrap(), None, &no_temp(), &roots),
        Some(PathLinkKind::File)
    );
}

#[test]
fn a_link_in_the_project_to_another_root_is_that_roots_file() {
    let first = ScratchDir::new("path-links-roots-first");
    let second = ScratchDir::new("path-links-roots-second");
    std::fs::write(second.join("b.ts"), "x").unwrap();
    symlink(second.join("b.ts"), first.join("to-second.ts")).unwrap();
    let roots = project_roots(&[&first, &second]);
    for raw in [
        "to-second.ts".to_string(),
        canonical(&first)
            .join("to-second.ts")
            .to_string_lossy()
            .into_owned(),
    ] {
        let resolved = reach(&raw, None, &no_temp(), &|| roots.clone()).unwrap();
        assert_eq!(resolved.project_root, Some(1), "{raw}");
    }
}

#[test]
fn a_path_spelled_in_a_temp_root_is_judged_where_a_link_in_it_leads() {
    let temp = ScratchDir::new("path-links-temp-link");
    let elsewhere = ScratchDir::new("path-links-temp-target");
    std::fs::write(elsewhere.join("a.txt"), "x").unwrap();
    std::fs::create_dir(elsewhere.join("folder")).unwrap();
    symlink(elsewhere.join("a.txt"), temp.join("a.txt")).unwrap();
    symlink(elsewhere.join("folder"), temp.join("folder")).unwrap();
    for (name, expected) in [
        ("a.txt", PathLinkKind::File),
        ("folder", PathLinkKind::Directory),
    ] {
        let path = canonical(&temp).join(name);
        assert_eq!(
            kind(
                path.to_str().unwrap(),
                None,
                &temp_of(&temp),
                &ProjectRoots::default()
            ),
            Some(expected),
            "{name}"
        );
    }
}

#[test]
fn a_link_from_a_temp_root_into_the_project_is_the_projects_file() {
    let temp = ScratchDir::new("path-links-temp-into");
    let project = ScratchDir::new("path-links-temp-into-project");
    std::fs::write(project.join("a.ts"), "x").unwrap();
    symlink(project.join("a.ts"), temp.join("to-project.ts")).unwrap();
    let roots = project_roots(&[&project]);
    let path = canonical(&temp).join("to-project.ts");
    let resolved = reach(path.to_str().unwrap(), None, &temp_of(&temp), &|| {
        roots.clone()
    })
    .unwrap();
    assert_eq!(resolved.project_root, Some(0));
}

#[cfg(unix)]
#[test]
fn a_path_outside_the_project_and_the_temp_roots_has_no_owner_check() {
    let dir = ScratchDir::new("path-links-no-owner-check");
    std::fs::create_dir(dir.join("open")).unwrap();
    std::fs::write(dir.join("open/a.txt"), "x").unwrap();
    std::fs::set_permissions(dir.join("open"), std::fs::Permissions::from_mode(0o777)).unwrap();
    std::fs::set_permissions(
        dir.join("open/a.txt"),
        std::fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    let path = canonical(&dir).join("open/a.txt");
    // As a temp root's file it is refused for others being able to change it; anywhere else it is
    // shown, as is any file on the host.
    assert_eq!(
        refusal(
            path.to_str().unwrap(),
            &temp_of(&dir),
            &ProjectRoots::default()
        ),
        Refusal::NotPrivate
    );
    assert_eq!(
        kind(
            path.to_str().unwrap(),
            None,
            &no_temp(),
            &ProjectRoots::default()
        ),
        Some(PathLinkKind::File)
    );
}

#[cfg(unix)]
#[test]
fn a_temp_path_must_be_the_daemons_users_and_closed_to_writes_by_others() {
    let temp = ScratchDir::new("path-links-owner");
    std::fs::create_dir_all(temp.join("open/inner")).unwrap();
    std::fs::create_dir_all(temp.join("closed")).unwrap();
    std::fs::write(temp.join("open/inner/a.txt"), "x").unwrap();
    std::fs::write(temp.join("closed/a.txt"), "x").unwrap();
    std::fs::write(temp.join("closed/writable.txt"), "x").unwrap();
    std::fs::set_permissions(temp.join("open"), std::fs::Permissions::from_mode(0o777)).unwrap();
    std::fs::set_permissions(
        temp.join("closed/writable.txt"),
        std::fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    let at = |name: &str| {
        let path = canonical(&temp).join(name);
        kind(
            path.to_str().unwrap(),
            None,
            &temp_of(&temp),
            &ProjectRoots::default(),
        )
    };
    assert_eq!(at("closed/a.txt"), Some(PathLinkKind::File));
    // A directory on the way, and the file itself, writable by others.
    assert_eq!(at("open/inner/a.txt"), None);
    assert_eq!(at("closed/writable.txt"), None);
}

#[test]
fn a_temp_root_that_is_the_filesystem_root_or_holds_home_is_not_used() {
    let pair = |path: &str| (PathBuf::from(path), PathBuf::from(path));
    let candidates = [
        pair("/"),
        pair("/Users"),
        pair("/Users/me/tmp"),
        pair("/private/tmp"),
    ];
    let roots = usable_temp_roots(&candidates, Some(Path::new("/Users/me")));
    // `/` and `/Users`, which holds home, are out; a directory inside home stays.
    assert_eq!(
        roots.canonical,
        [
            PathBuf::from("/Users/me/tmp"),
            PathBuf::from("/private/tmp")
        ]
    );
    assert_eq!(roots.lexical, roots.canonical);
    // `/` alone is refused with no home to hold it.
    let alone = usable_temp_roots(&[pair("/")], None);
    assert!(alone.canonical.is_empty() && alone.lexical.is_empty());
}

#[test]
fn a_path_with_dots_is_judged_by_where_it_leads_not_by_the_root_it_was_written_under() {
    let project = ScratchDir::new("path-links-dots");
    let elsewhere = ScratchDir::new("path-links-dots-target");
    std::fs::write(elsewhere.join("secret.txt"), "x").unwrap();
    let roots = project_roots(&[&project]);
    let name = elsewhere.file_name().unwrap().to_str().unwrap();
    let up = format!("../{name}/secret.txt");
    // A relative path may lead out of a root, and is then what it names: not the project's.
    let found = resolve(&up, None, &no_temp(), &|| roots.clone()).unwrap();
    assert_eq!(
        (found.path, found.project_root),
        (canonical(&elsewhere).join("secret.txt"), None)
    );
    // An absolute one is folded first and is then what it names: neither the project's nor a temp
    // root's, whichever root it was written under.
    let absolute = canonical(&project).join(&up);
    for temp in [no_temp(), temp_of(&project)] {
        let resolved = resolve(absolute.to_str().unwrap(), None, &temp, &|| roots.clone()).unwrap();
        assert_eq!(
            (resolved.path, resolved.project_root),
            (canonical(&elsewhere).join("secret.txt"), None)
        );
    }
}

#[test]
fn a_path_asks_for_the_project_roots_once_whether_or_not_it_is_under_a_temp_root() {
    let calls = std::cell::Cell::new(0);
    let temp = ScratchDir::new("path-links-lexical");
    let temp_roots = temp_of(&temp);
    let project = || {
        calls.set(calls.get() + 1);
        ProjectRoots::default()
    };
    let elsewhere = ScratchDir::new("path-links-lexical-elsewhere");
    std::fs::write(elsewhere.join("a.txt"), "x").unwrap();
    let outside = canonical(&elsewhere).join("a.txt");
    assert!(resolve(outside.to_str().unwrap(), None, &temp_roots, &project).is_some());
    assert_eq!(calls.get(), 1);
    // Told apart from a project kept under the temp root, which it may be.
    let inside = canonical(&temp).join("a.txt");
    std::fs::write(&inside, "x").unwrap();
    let resolved = resolve(inside.to_str().unwrap(), None, &temp_roots, &project).unwrap();
    assert_eq!(calls.get(), 2);
    assert_eq!(resolved.project_root, None);
}

#[test]
fn a_project_file_spelled_under_a_temp_root_is_still_the_projects() {
    let dir = ScratchDir::new("path-links-project-in-temp");
    std::fs::create_dir(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/a.txt"), "x").unwrap();
    std::fs::create_dir(dir.join("tree")).unwrap();
    std::fs::write(dir.join("tree/b.txt"), "x").unwrap();
    let mut roots = project_roots(&[&dir, &dir.join("tree")]);
    roots.worktrees[1] = Some("wt".into());
    // The project's directory is itself the temp root here, as a project kept in `/tmp` is.
    let temp = temp_of(&dir);
    for (name, index) in [("sub/a.txt", 0), ("tree/b.txt", 1)] {
        let path = canonical(&dir).join(name);
        let resolved = resolve(path.to_str().unwrap(), None, &temp, &|| roots.clone()).unwrap();
        assert_eq!(resolved.project_root, Some(index), "{name}");
        assert!(!resolved.viewer_only(), "{name}");
    }
    // A temp file outside every project root stays a temp file.
    let elsewhere = ScratchDir::new("path-links-project-in-temp-elsewhere");
    std::fs::write(elsewhere.join("c.txt"), "x").unwrap();
    let path = canonical(&elsewhere).join("c.txt");
    let resolved = resolve(path.to_str().unwrap(), None, &temp_of(&elsewhere), &|| {
        roots.clone()
    })
    .unwrap();
    assert_eq!(resolved.project_root, None);
    assert!(!resolved.viewer_only());
    // A folder of the project that others could have written into is not handed out, as in any
    // other place under a temp root.
    #[cfg(unix)]
    {
        std::fs::create_dir(dir.join("sub/open")).unwrap();
        std::fs::set_permissions(dir.join("sub/open"), std::fs::Permissions::from_mode(0o777))
            .unwrap();
        let path = canonical(&dir).join("sub/open");
        assert_eq!(
            refusal(path.to_str().unwrap(), &temp, &roots),
            Refusal::NotPrivate
        );
        // The same however the folder is spelled.
        assert_eq!(refusal("sub/open", &temp, &roots), Refusal::NotPrivate);
        // One closed to others is handed out, and is not left to the viewer.
        std::fs::create_dir(dir.join("sub/closed")).unwrap();
        std::fs::set_permissions(
            dir.join("sub/closed"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        for raw in [
            canonical(&dir).join("sub/closed").to_str().unwrap(),
            "sub/closed",
        ] {
            let resolved = resolve(raw, None, &temp, &|| roots.clone()).unwrap();
            assert_eq!(resolved.kind, PathLinkKind::Directory, "{raw}");
            assert!(!resolved.viewer_only(), "{raw}");
        }
    }
}

#[test]
fn dots_are_folded_lexically_before_the_file_system_sees_the_path() {
    let project = ScratchDir::new("path-links-fold");
    let elsewhere = ScratchDir::new("path-links-fold-target");
    std::fs::write(project.join("a.txt"), "x").unwrap();
    symlink(&*elsewhere, project.join("link")).unwrap();
    let roots = project_roots(&[&project]);
    // The file system would take `link/..` to the directory above `elsewhere`; written lexically it
    // is the project's own directory.
    let absolute = canonical(&project).join("link/../a.txt");
    assert_eq!(
        kind(absolute.to_str().unwrap(), None, &no_temp(), &roots),
        Some(PathLinkKind::File)
    );
    assert_eq!(
        kind("link/../a.txt", None, &no_temp(), &roots),
        Some(PathLinkKind::File)
    );
}

fn whereabouts(
    raw: &str,
    home: Option<&Path>,
    temp: &TempRoots,
    roots: &ProjectRoots,
) -> Whereabouts {
    match reach(raw, home, temp, &|| roots.clone()) {
        Err(Unreached::Gone { was }) => was,
        other => panic!("{raw}: expected a gone path, got {other:?}"),
    }
}

#[test]
fn a_gone_path_says_where_it_was_spelled() {
    let project = ScratchDir::new("path-links-gone-project");
    let temp = ScratchDir::new("path-links-gone-temp");
    let elsewhere = ScratchDir::new("path-links-gone-elsewhere");
    let roots = project_roots(&[&project]);
    let (in_project, in_temp, outside) =
        (canonical(&project), canonical(&temp), canonical(&elsewhere));
    let at =
        |path: &Path, temp: &TempRoots| whereabouts(path.to_str().unwrap(), None, temp, &roots);
    // The project's, spelled absolute or relative, even when the project itself is under a temp root.
    for temp in [no_temp(), temp_of(&project), temp_of(&temp)] {
        assert_eq!(at(&in_project.join("gone.ts"), &temp), Whereabouts::Project);
        assert_eq!(
            whereabouts("src/gone.ts", None, &temp, &roots),
            Whereabouts::Project
        );
    }
    // A link of the project whose target is gone is still spelled in the project.
    symlink(outside.join("gone.ts"), project.join("dangling")).unwrap();
    assert_eq!(
        at(&in_project.join("dangling"), &no_temp()),
        Whereabouts::Project
    );
    // In a temp root, outside the project: the temp root's.
    assert_eq!(
        at(&in_temp.join("gone.ts"), &temp_of(&temp)),
        Whereabouts::Temporary
    );
    // Anywhere else on the host, with or without temp roots, and for a `~/` path.
    assert_eq!(
        at(&outside.join("gone.ts"), &no_temp()),
        Whereabouts::Elsewhere
    );
    assert_eq!(
        at(&outside.join("gone.ts"), &temp_of(&temp)),
        Whereabouts::Elsewhere
    );
    assert_eq!(
        whereabouts("~/gone.ts", Some(&outside), &no_temp(), &roots),
        Whereabouts::Elsewhere
    );
}

/// What a reveal of `raw` selects, judged with no temp roots: the link it ends in by its own name,
/// else none.
fn own_entry_of(raw: &str, roots: &ProjectRoots) -> Option<PathBuf> {
    let resolved = reach(raw, None, &no_temp(), &|| roots.clone()).unwrap();
    own_entry(&resolved, &no_temp(), &|| roots.clone())
}

#[test]
fn a_link_anywhere_on_the_host_is_selected_by_its_own_name() {
    let dir = ScratchDir::new("path-links-own-entry");
    std::fs::create_dir(dir.join("sub")).unwrap();
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    symlink("notes.txt", dir.join("file-link")).unwrap();
    symlink("sub", dir.join("folder-link")).unwrap();
    symlink("../notes.txt", dir.join("sub/up")).unwrap();
    let base = canonical(&dir);
    let absolute = |name: &str| base.join(name).to_string_lossy().into_owned();
    let none = ProjectRoots::default();
    for (raw, expected) in [
        (absolute("file-link"), Some(base.join("file-link"))),
        (absolute("folder-link"), Some(base.join("folder-link"))),
        // A link below a linked folder is selected where it lies.
        (absolute("folder-link/up"), Some(base.join("sub/up"))),
        // Not a link, or a spelling that names the folder.
        (absolute("notes.txt"), None),
        (absolute("folder-link/"), None),
        (absolute("folder-link/."), None),
    ] {
        assert_eq!(own_entry_of(&raw, &none), expected, "{raw}");
    }
    // Where the link sits in a project it is the project's root that holds it, and a link of the
    // project that leads out of it is selected where it lies, though it resolves elsewhere.
    let elsewhere = ScratchDir::new("path-links-own-entry-elsewhere");
    std::fs::write(elsewhere.join("far.txt"), "x").unwrap();
    symlink(elsewhere.join("far.txt"), dir.join("out-link")).unwrap();
    let roots = project_roots(&[&dir]);
    for name in ["file-link", "out-link"] {
        assert_eq!(
            own_entry_of(&absolute(name), &roots),
            Some(base.join(name)),
            "{name}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_link_in_a_folder_others_can_write_is_selected_outside_the_temp_roots() {
    let dir = ScratchDir::new("path-links-own-entry-open");
    std::fs::create_dir(dir.join("open")).unwrap();
    std::fs::write(dir.join("open/a.txt"), "x").unwrap();
    symlink("a.txt", dir.join("open/link")).unwrap();
    std::fs::set_permissions(dir.join("open"), std::fs::Permissions::from_mode(0o777)).unwrap();
    let link = canonical(&dir).join("open/link");
    assert_eq!(
        own_entry_of(link.to_str().unwrap(), &ProjectRoots::default()),
        Some(link)
    );
}

/// A host with no `/net`, and one with an automount tree there, answer a look at it differently
/// (gone, or a connection out), so a name below it that is refused as `Other` and not as gone shows
/// the disk was not consulted.
#[cfg(unix)]
const NET_FILE: &str = "/net/octoboard-no-such-host/share/a.txt";

#[cfg(unix)]
#[test]
fn a_network_path_is_refused_before_the_disk_is_looked_at() {
    let project = ScratchDir::new("path-links-network");
    std::fs::write(project.join("a.txt"), "x").unwrap();
    let roots = project_roots(&[&project]);
    let home = Path::new("/net/octoboard-no-such-host/me");
    // Whatever the roots and temp roots, and whether or not anything is there.
    for (temp, roots) in [
        (no_temp(), ProjectRoots::default()),
        (no_temp(), roots.clone()),
        (temp_of(&project), roots.clone()),
    ] {
        for raw in [
            "/net",
            "/net/",
            "/net/octoboard-no-such-host",
            NET_FILE,
            "~/a.txt",
            "/net/./octoboard-no-such-host/a.txt",
            "/net/octoboard-no-such-host/../other/a.txt",
        ] {
            let result = reach(raw, Some(home), &temp, &|| roots.clone());
            assert_eq!(result, Err(Unreached::Refused(Refusal::Other)), "{raw}");
        }
    }
}

#[cfg(unix)]
#[test]
fn a_path_is_refused_as_network_by_where_its_dots_lead() {
    let project = ScratchDir::new("path-links-network-dots");
    let roots = project_roots(&[&project]);
    let ups = "../".repeat(canonical(&project).components().count());
    let from_project = canonical(&project).join(format!("{ups}net/octoboard-no-such-host/a.txt"));
    let refused = Err(Unreached::Refused(Refusal::Other));
    // Into the tree by dots, written absolute or relative to a root.
    assert_eq!(
        reach(from_project.to_str().unwrap(), None, &no_temp(), &|| roots
            .clone()),
        refused
    );
    assert_eq!(
        reach(
            "/tmp/../net/octoboard-no-such-host/a.txt",
            None,
            &no_temp(),
            &|| roots.clone()
        ),
        refused
    );
    assert_eq!(
        reach(
            &format!("{ups}net/octoboard-no-such-host/a.txt"),
            None,
            &no_temp(),
            &|| roots.clone()
        ),
        refused
    );
    // And out of it by dots: written under `/net`, it is judged by where it leads.
    let out = reach(
        "/net/../etc/octoboard-no-such-file",
        None,
        &no_temp(),
        &|| roots.clone(),
    );
    assert!(
        !matches!(out, Err(Unreached::Refused(Refusal::Other))),
        "{out:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_name_that_only_starts_like_net_is_looked_at_like_any_other_path() {
    let none = ProjectRoots::default();
    for raw in [
        "/network-octoboard-test/a.txt",
        "/netx-octoboard-test/a.txt",
        "/octoboard-test/net/a.txt",
    ] {
        assert_eq!(
            whereabouts(raw, None, &no_temp(), &none),
            Whereabouts::Elsewhere,
            "{raw}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_relative_path_is_refused_when_its_join_leaves_the_roots_into_the_network() {
    let project = ScratchDir::new("path-links-network-roots");
    std::fs::write(project.join("a.txt"), "x").unwrap();
    let roots = project_roots(&[&project]);
    let ups = "../".repeat(canonical(&project).components().count());
    // The join leads out of the roots into the automount tree, whether or not a file is there.
    for raw in [
        format!("{ups}net/octoboard-no-such-host/a.txt"),
        format!("{ups}net"),
    ] {
        assert_eq!(refusal(&raw, &no_temp(), &roots), Refusal::Other, "{raw}");
    }
}

/// The decision alone, on spellings that are never looked at: nothing here reads the disk.
#[cfg(unix)]
#[test]
fn a_spelling_on_a_network_share_is_refused_unless_it_lies_inside_a_root_of_the_project() {
    let share = PathBuf::from("/net/octoboard-no-such-host/project");
    let on_share = ProjectRoots {
        canonical: vec![PathBuf::from("/work/app"), share.clone()],
        lexical: vec![PathBuf::from("/work/app"), share.clone()],
        worktrees: vec![None, None],
    };
    let elsewhere = ProjectRoots {
        canonical: vec![PathBuf::from("/work/app")],
        lexical: vec![PathBuf::from("/work/app")],
        worktrees: vec![None],
    };
    let refused = |spelling: &str, roots: &ProjectRoots| {
        refused_as_network(Path::new(spelling), &|| roots.clone())
    };
    // Inside a root kept on the share, written absolute or as the join of a relative path.
    assert!(!refused("/net/octoboard-no-such-host/project", &on_share));
    assert!(!refused(
        "/net/octoboard-no-such-host/project/a.txt",
        &on_share
    ));
    let joined = lexically_normalise(&share.join("src/../a.txt"));
    assert!(!refused(joined.to_str().unwrap(), &on_share));
    // Beside it, or with no root on a share at all, it leaves the roots into one.
    assert!(refused(
        "/net/octoboard-no-such-host/other/a.txt",
        &on_share
    ));
    assert!(refused(
        "/net/octoboard-no-such-host/project/a.txt",
        &elsewhere
    ));
    assert!(refused("/net", &on_share));
    let leaving = lexically_normalise(&Path::new("/work/app").join("../../net/host/a.txt"));
    assert!(refused(leaving.to_str().unwrap(), &on_share));
    // What is not on a share is never refused, in a root or not.
    assert!(!refused("/work/app/a.txt", &elsewhere));
    assert!(!refused("/elsewhere/a.txt", &elsewhere));
}

#[cfg(windows)]
#[test]
fn a_drive_relative_path_is_refused_not_joined_to_a_root() {
    let project = ScratchDir::new("path-links-drive-relative");
    std::fs::write(project.join("a.txt"), "x").unwrap();
    let roots = project_roots(&[&project]);
    for raw in [r"C:a.txt", r"D:..\a.txt", "c:a.txt"] {
        assert_eq!(refusal(raw, &no_temp(), &roots), Refusal::Other, "{raw}");
    }
}

/// The grouping of what a probe may block on alike: the project root a spelling is in, else home
/// and one name, else its root and first two names.
#[cfg(unix)]
#[test]
fn a_path_lies_on_the_place_of_its_project_root_else_home_and_a_name_else_its_first_two_names() {
    let roots = ProjectRoots {
        canonical: vec![
            PathBuf::from("/Users/me/code/app"),
            PathBuf::from("/Users/me/code/app/.wt/x"),
        ],
        lexical: vec![
            PathBuf::from("/Users/me/code/app"),
            PathBuf::from("/Users/me/code/app/.wt/x"),
            PathBuf::from("/stored/app"),
        ],
        worktrees: vec![None, None],
    };
    let home = Path::new("/Users/me");
    let place = |raw: &str| place_of(raw, Some(home), &roots);
    for (raw, expected) in [
        // A project root is a place of its own, the innermost that holds the spelling, however
        // the root is written, and a relative path is the root it is joined to.
        ("/Users/me/code/app/src/a", "/Users/me/code/app"),
        ("/Users/me/code/app/.wt/x/a", "/Users/me/code/app/.wt/x"),
        ("/stored/app/a/b/c", "/stored/app"),
        ("~/code/app/a", "/Users/me/code/app"),
        ("src/a", "/Users/me/code/app"),
        ("src/../../app/a", "/Users/me/code/app"),
        // Leaving the roots, a spelling under home is home and one name.
        ("../other/a", "/Users/me/code"),
        ("~/nas/x/y", "/Users/me/nas"),
        ("/Users/me/nas", "/Users/me/nas"),
        ("/Users/me", "/Users/me"),
        ("/Users/me/../me/b/c", "/Users/me/b"),
        // Elsewhere, the root and the first two names, folded.
        ("/Volumes/X/a/b", "/Volumes/X"),
        ("/Volumes/X", "/Volumes/X"),
        ("/Volumes/X/../Y/a", "/Volumes/Y"),
        ("/Volumes/./X/a/./b", "/Volumes/X"),
        ("/Users/you/a/b", "/Users/you"),
        ("/a", "/a"),
        ("/", "/"),
        ("../../../../z/w/v", "/z/w"),
    ] {
        assert_eq!(place(raw), Path::new(expected), "{raw}");
    }
    assert_ne!(place("/Volumes/X/a"), place("/Volumes/Y/a"));
    assert_eq!(place("/Volumes/X/a"), place("/Volumes/X/b/c"));
    // Another project is another place, and a stall under home touches neither.
    let other = ProjectRoots {
        canonical: vec![PathBuf::from("/Users/me/code/site")],
        lexical: vec![PathBuf::from("/Users/me/code/site")],
        worktrees: vec![None],
    };
    assert_ne!(place("a"), place_of("a", Some(home), &other));
    assert_ne!(place("~/nas/a"), place("a"));
    // No root to try from, or no home: no place, which nothing is read for.
    assert_eq!(
        place_of("a/b", None, &ProjectRoots::default()),
        Path::new("")
    );
    assert_eq!(place_of("~/a", None, &roots), Path::new(""));
}

/// Windows spells these as it does; where the host is Unix they are names of a file in the folder.
#[cfg(windows)]
#[test]
fn a_unc_or_device_path_is_refused_and_the_verbatim_form_of_a_drive_is_not() {
    let dir = ScratchDir::new("path-links-unc");
    std::fs::write(dir.join("a.txt"), "x").unwrap();
    let roots = project_roots(&[&dir]);
    for raw in [
        r"\\server\share\a.txt",
        "//server/share/a.txt",
        r"\\.\pipe\x",
        r"\\.\C:\a.txt",
        r"\\?\UNC\server\share\a.txt",
        r"\\?\Volume{0b1c}\a.txt",
    ] {
        assert_eq!(
            reach(raw, None, &no_temp(), &|| roots.clone()),
            Err(Unreached::Refused(Refusal::Other)),
            "{raw}"
        );
    }
    let plain = canonical(&dir).join("a.txt");
    let verbatim = format!(r"\\?\{}", plain.display());
    let resolved = reach(&verbatim, None, &no_temp(), &|| roots.clone())
        .unwrap_or_else(|unreached| panic!("{verbatim}: {unreached:?}"));
    assert_eq!((resolved.kind, resolved.path), (PathLinkKind::File, plain));
}

/// A bundle among the folders above a path counts wherever the path was found outside the
/// project: from the top of the tree for one found elsewhere on the host, but only below the temp
/// root for one found in it.
#[test]
fn a_file_elsewhere_is_judged_launchable_by_every_folder_from_the_top_of_the_tree() {
    let dir = ScratchDir::new("path-links-top-of-tree");
    std::fs::create_dir_all(dir.join("Outer.app/Contents/Resources")).unwrap();
    std::fs::write(dir.join("Outer.app/Contents/Resources/notes.txt"), "x").unwrap();
    let base = canonical(&dir);
    let at = |temp: &TempRoots, name: &str| {
        reach(
            base.join(name).to_str().unwrap(),
            None,
            temp,
            &ProjectRoots::default,
        )
    };
    let inside = base.join("Outer.app/Contents");
    // Elsewhere on the host: a file in a bundle is for the viewer only, a folder of one is refused.
    let host = at(&no_temp(), "Outer.app/Contents/Resources/notes.txt").unwrap();
    assert_eq!(
        (host.kind, host.viewer_only(), host.project_root),
        (PathLinkKind::File, true, None)
    );
    for folder in [
        "Outer.app",
        "Outer.app/Contents",
        "Outer.app/Contents/Resources",
    ] {
        assert_eq!(
            at(&no_temp(), folder),
            Err(Unreached::Refused(Refusal::Launchable)),
            "{folder}"
        );
    }
    // A temp root above the bundle counts it; one inside it does not, as only what lies below the
    // root is the root's own business.
    assert!(at(&temp_of(&dir), "Outer.app/Contents/Resources/notes.txt")
        .unwrap()
        .viewer_only());
    let within = TempRoots {
        canonical: vec![inside.clone()],
        lexical: vec![inside],
        check_owner: false,
    };
    let in_temp = at(&within, "Outer.app/Contents/Resources/notes.txt").unwrap();
    assert_eq!(
        (in_temp.kind, in_temp.viewer_only()),
        (PathLinkKind::File, false)
    );
}

#[test]
fn a_relative_path_to_a_sibling_outside_every_root_is_reachable_and_not_the_projects() {
    let outer = ScratchDir::new("path-links-sibling");
    std::fs::create_dir_all(outer.join("proj/src")).unwrap();
    std::fs::create_dir_all(outer.join("sib/docs")).unwrap();
    std::fs::write(outer.join("sib/x.txt"), "x").unwrap();
    let roots = project_roots(&[&outer.join("proj")]);
    let base = canonical(&outer);
    for (raw, kind, target) in [
        ("../sib/x.txt", PathLinkKind::File, base.join("sib/x.txt")),
        (
            "../sib/docs",
            PathLinkKind::Directory,
            base.join("sib/docs"),
        ),
        // The way out is by any number of levels, and back in by another name.
        (
            "src/../../sib/x.txt",
            PathLinkKind::File,
            base.join("sib/x.txt"),
        ),
    ] {
        let resolved = reach(raw, None, &no_temp(), &|| roots.clone())
            .unwrap_or_else(|unreached| panic!("{raw}: {unreached:?}"));
        assert_eq!(
            (resolved.kind, resolved.path, resolved.project_root),
            (kind, target, None),
            "{raw}"
        );
    }
    // What is not there is gone, spelled as the project's, as the relative path is.
    assert_eq!(
        whereabouts("../sib/missing.txt", None, &no_temp(), &roots),
        Whereabouts::Project
    );
}

mod with_state {
    use super::*;
    use crate::browse::tests::Fixture;
    use crate::test_support::{git, repo_with};

    /// A scratch file and folder that are in no project and in no temp root of the test, wherever
    /// the checkout and the system's temp folder lie: the test's temp roots are set to `temp`
    /// (none, or the scratch folders it means as temp roots) for as long as this is held.
    struct Outside {
        dir: ScratchDir,
        _temp: TempRootsOverride,
    }

    impl Outside {
        fn new(temp: &[&Path]) -> Self {
            let dir = ScratchDir::new("path-links-outside");
            std::fs::write(dir.join("outside.txt"), "outside").unwrap();
            #[cfg(unix)]
            for (path, mode) in [("", 0o755), ("outside.txt", 0o644)] {
                std::fs::set_permissions(dir.join(path), std::fs::Permissions::from_mode(mode))
                    .unwrap();
            }
            Self {
                dir,
                _temp: with_temp_roots(temp),
            }
        }

        fn folder(&self) -> PathBuf {
            canonical(&self.dir)
        }

        fn file(&self) -> PathBuf {
            self.folder().join("outside.txt")
        }

        /// A path that is not there, below a folder that is not there either.
        fn gone(&self) -> PathBuf {
            self.folder().join("octoboardd-no-such-folder/gone.txt")
        }
    }

    #[test]
    fn probing_answers_in_order_and_caps_the_paths_asked() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-probe");
        std::fs::write(directory.join("a.ts"), b"x").unwrap();
        let project = fixture.project("links-project", &directory);
        let links = probe(
            &fixture.state,
            Some(&project),
            vec!["a.ts".into(), "missing.ts".into(), "/no/such/file".into()],
        )
        .unwrap();
        let kinds: Vec<_> = links
            .iter()
            .map(|link| serde_json::to_value(link.kind).unwrap())
            .collect();
        assert_eq!(kinds, ["file", "none", "none"]);
        assert_eq!(links[1].path, "missing.ts");

        let error = probe(
            &fixture.state,
            Some(&project),
            vec!["a.ts".into(); MAX_PROBED_PATHS + 1],
        )
        .unwrap_err();
        let coded = error.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::LIMIT_EXCEEDED);
        assert_eq!(
            coded.params.get("limit").map(String::as_str),
            Some("probed_paths")
        );
        assert_eq!(coded.params.get("max").map(String::as_str), Some("64"));
        assert_eq!(
            probe(
                &fixture.state,
                Some(&project),
                vec!["a.ts".into(); MAX_PROBED_PATHS]
            )
            .unwrap()
            .len(),
            MAX_PROBED_PATHS
        );

        let error = probe(&fixture.state, Some("unknown"), vec![]).unwrap_err();
        assert_eq!(
            error.downcast_ref::<CodedError>().unwrap().code,
            error_code::UNKNOWN_PROJECT
        );
    }

    #[test]
    fn a_project_whose_directory_is_gone_still_answers_for_the_temp_roots_and_the_host() {
        let fixture = Fixture::new();
        let missing = ScratchDir::new("path-links-gone").join("missing");
        let project = fixture.project("gone-project", &missing);
        let scratch = ScratchDir::new("path-links-gone-temp");
        // Owned and closed to others whatever the umask.
        #[cfg(unix)]
        std::fs::set_permissions(&*scratch, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(scratch.join("a.png"), "x").unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(
            scratch.join("a.png"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let in_temp = canonical(&scratch).join("a.png");
        let outside = Outside::new(&[&scratch]);
        let links = probe(
            &fixture.state,
            Some(&project),
            vec![
                "a.ts".into(),
                in_temp.to_string_lossy().into_owned(),
                outside.file().to_string_lossy().into_owned(),
            ],
        )
        .unwrap();
        assert_eq!(
            (links[0].kind, links[1].kind, links[2].kind),
            (PathLinkKind::None, PathLinkKind::File, PathLinkKind::File)
        );
    }

    #[test]
    fn opening_what_is_gone_is_refused_as_not_found_wherever_it_was_spelled() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-open");
        std::fs::create_dir(directory.join("src")).unwrap();
        std::fs::write(directory.join("src/file.ts"), "x").unwrap();
        let project = fixture.project("open-project", &directory);
        let gone_absolute = canonical(&directory).join("src/removed.ts");
        let gone_absolute = gone_absolute.to_string_lossy().into_owned();
        for (path, code) in [
            ("nothing.ts", error_code::FILE_NOT_FOUND),
            ("src/gone/a.ts", error_code::FILE_NOT_FOUND),
            (gone_absolute.as_str(), error_code::FILE_NOT_FOUND),
            ("../elsewhere.ts", error_code::FILE_NOT_FOUND),
            ("src/file.ts/below.ts", error_code::FILE_NOT_FOUND),
        ] {
            let error = open_with(
                &fixture.state,
                Some(&project),
                path,
                |_| false,
                |_| panic!("{path} must not be opened"),
            )
            .unwrap_err();
            let coded = error.downcast_ref::<CodedError>().unwrap();
            assert_eq!(coded.code, code, "{path}");
            if code == error_code::FILE_NOT_FOUND {
                // Spelled inside the project, absolute or not, even though the scratch project sits
                // under a temp root.
                assert_eq!(coded.params["root"], "project", "{path}");
            }
        }
        // A gone file in a temp root outside the project is the temp root's.
        let elsewhere = ScratchDir::new("path-links-open-temp");
        let outside = Outside::new(&[&elsewhere]);
        let gone_temp = canonical(&elsewhere).join("removed.png");
        let error = open_with(
            &fixture.state,
            Some(&project),
            gone_temp.to_str().unwrap(),
            |_| false,
            |_| panic!("must not be opened"),
        )
        .unwrap_err();
        let coded = error.downcast_ref::<CodedError>().unwrap();
        assert_eq!(
            (coded.code, coded.params["root"].as_str()),
            (error_code::FILE_NOT_FOUND, "temporary")
        );
        // Anywhere else on the host it is the host's.
        let error = open_with(
            &fixture.state,
            Some(&project),
            outside.gone().to_str().unwrap(),
            |_| false,
            |_| panic!("must not be opened"),
        )
        .unwrap_err();
        let coded = error.downcast_ref::<CodedError>().unwrap();
        assert_eq!(
            (coded.code, coded.params["root"].as_str()),
            (error_code::FILE_NOT_FOUND, "host")
        );
    }

    #[test]
    fn the_roots_of_a_project_include_its_place_in_a_linked_worktree() {
        let fixture = Fixture::new();
        let repository = repo_with("path-links-repo", &[(b"a.ts", b"x")]);
        let linked_parent = ScratchDir::new("path-links-linked");
        let linked = linked_parent.join("tree");
        git(
            &repository,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "other",
                linked.to_str().unwrap(),
            ],
        );
        let project = fixture.project("worktree-project", &repository);
        let roots = crate::browse::scope::project_live_roots(&fixture.state, &project, &|| {
            fixture.state.host_tools.isolated_git()
        })
        .unwrap();
        assert_eq!(roots.len(), 2, "{roots:?}");
        assert_eq!(roots[0].root, canonical(&repository));
        assert_eq!(roots[0].worktree, None);
        assert_eq!(roots[1].root, canonical(&linked));
        assert!(roots[1].worktree.is_some());
        // `a.ts` is in both, so it names no one file; a path only the worktree has does.
        std::fs::write(linked.join("only.ts"), "x").unwrap();
        let links = probe(
            &fixture.state,
            Some(&project),
            vec!["a.ts".into(), "only.ts".into()],
        )
        .unwrap();
        assert_eq!(
            (links[0].kind, links[1].kind),
            (PathLinkKind::None, PathLinkKind::File)
        );
        // The viewer reads it from the worktree, under its own name there.
        assert_eq!(links[0].view, None);
        assert_eq!(
            links[1].view,
            Some(PathLinkView {
                worktree: roots[1].worktree.clone(),
                path: "only.ts".into()
            })
        );
    }

    #[test]
    fn every_project_file_gets_a_view_and_nothing_else_does() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-view");
        std::fs::create_dir(directory.join("src")).unwrap();
        std::fs::write(directory.join("src/a%b.ts"), "text").unwrap();
        std::fs::write(directory.join("data.bin"), [0u8, 1, 2, 3]).unwrap();
        std::fs::write(directory.join("run.command"), "x").unwrap();
        let project = fixture.project("view-project", &directory);
        let temp = ScratchDir::new("path-links-view-temp");
        std::fs::write(temp.join("t.txt"), "x").unwrap();
        let in_temp = canonical(&temp)
            .join("t.txt")
            .to_string_lossy()
            .into_owned();
        let outside = Outside::new(&[&temp]);
        let outside = outside.file().to_string_lossy().into_owned();
        let paths = [
            "src/a%b.ts",
            "data.bin",
            "run.command",
            "src",
            &in_temp,
            &outside,
        ];
        let links = probe(
            &fixture.state,
            Some(&project),
            paths.iter().map(|path| path.to_string()).collect(),
        )
        .unwrap();
        let views: Vec<_> = links
            .iter()
            .map(|link| link.view.as_ref().map(|view| view.path.as_str()))
            .collect();
        assert_eq!(
            views,
            [
                Some("src/a%25b.ts"),
                Some("data.bin"),
                Some("run.command"),
                None,
                None,
                None
            ]
        );
        assert!(links.iter().all(|link| link
            .view
            .as_ref()
            .is_none_or(|view| view.worktree.is_none())));
        // Only what the system could launch is for the viewer alone; a folder, a plain file, a temp
        // file and one elsewhere on the host are not.
        let viewer_only: Vec<_> = links.iter().map(|link| link.viewer_only).collect();
        assert_eq!(viewer_only, [false, true, true, false, false, false]);
        assert_eq!(links[3].kind, PathLinkKind::Directory);
    }

    #[test]
    fn a_project_directory_is_viewed_through_its_entry_file() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-dir-view");
        std::fs::create_dir_all(directory.join("docs/guide")).unwrap();
        std::fs::write(directory.join("docs/readme.md"), "x").unwrap();
        std::fs::write(directory.join("docs/index.html"), "x").unwrap();
        std::fs::write(directory.join("docs/guide/index.html"), "x").unwrap();
        std::fs::create_dir(directory.join("empty")).unwrap();
        let project = fixture.project("dir-view-project", &directory);
        let links = probe(
            &fixture.state,
            Some(&project),
            ["docs", "docs/guide", "empty"].map(String::from).to_vec(),
        )
        .unwrap();
        let views: Vec<_> = links
            .iter()
            .map(|link| link.view.as_ref().map(|view| view.path.as_str()))
            .collect();
        assert_eq!(
            views,
            [Some("docs/readme.md"), Some("docs/guide/index.html"), None]
        );
        assert!(links
            .iter()
            .all(|link| link.kind == PathLinkKind::Directory && !link.viewer_only));
    }

    #[test]
    fn a_launchable_project_file_is_never_opened() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-open-launchable");
        std::fs::write(directory.join("run.command"), "x").unwrap();
        let project = fixture.project("launch-project", &directory);
        // Opens nothing, whatever the rule says: a regression must not launch the file.
        let opened = std::cell::Cell::new(false);
        let error = open_with(
            &fixture.state,
            Some(&project),
            "run.command",
            |_| false,
            |_| {
                opened.set(true);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(!opened.get());
        let coded = error.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::PATH_LINK_REFUSED);
        assert_eq!(coded.params["cause"], "launchable");
        assert_eq!(coded.params["path"], "run.command");
    }

    #[test]
    fn a_launchable_file_in_a_temp_root_is_never_opened() {
        let fixture = Fixture::new();
        let scratch = ScratchDir::new("path-links-open-temp-launchable");
        std::fs::write(scratch.join("run.sh"), "x").unwrap();
        let path = canonical(&scratch).join("run.sh");
        let opened = std::cell::Cell::new(false);
        let error = open_with(
            &fixture.state,
            None,
            path.to_str().unwrap(),
            |_| false,
            |_| {
                opened.set(true);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(!opened.get());
        let coded = error.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.params["cause"], "launchable");
    }

    #[cfg(unix)]
    #[test]
    fn a_project_file_others_could_change_is_shown_but_not_opened_under_a_temp_root() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-open-project-others");
        std::fs::create_dir(directory.join("open")).unwrap();
        std::fs::write(directory.join("open/notes.txt"), "x").unwrap();
        std::fs::set_permissions(
            directory.join("open"),
            std::fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        let project = fixture.project("open-others-project", &directory);
        let links = probe(
            &fixture.state,
            Some(&project),
            vec!["open/notes.txt".into()],
        )
        .unwrap();
        assert_eq!(links[0].kind, PathLinkKind::File);
        assert!(!links[0].viewer_only);
        let opened = std::cell::Cell::new(false);
        let error = open_with(
            &fixture.state,
            Some(&project),
            "open/notes.txt",
            |_| false,
            |_| {
                opened.set(true);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(!opened.get());
        let coded = error.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::PATH_LINK_REFUSED);
        assert_eq!(coded.params["cause"], "not_private");
    }

    pub(super) fn read_text(
        fixture: &Fixture,
        project: Option<&str>,
        path: &str,
        limit: usize,
    ) -> Result<(String, String)> {
        let file = read(
            &fixture.state,
            project,
            path,
            limit,
            &AtomicBool::new(false),
        )?;
        Ok((String::from_utf8(file.bytes).unwrap(), file.version))
    }

    #[test]
    fn reading_a_link_returns_a_file_of_the_project_a_temp_root_or_the_host_whatever_its_type() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-read");
        std::fs::write(directory.join("a.txt"), "project").unwrap();
        std::fs::write(directory.join("run.command"), "launchable").unwrap();
        let project = fixture.project("read-project", &directory);
        let scratch = ScratchDir::new("path-links-read-temp");
        std::fs::write(scratch.join("out.txt"), "scratch").unwrap();
        let in_temp = canonical(&scratch).join("out.txt");
        let outside = Outside::new(&[&scratch]);
        let manifest = std::fs::read_to_string(outside.file()).unwrap();
        let outside = outside.file();
        for (project, path, text) in [
            (Some(project.as_str()), "a.txt", "project"),
            (Some(project.as_str()), "run.command", "launchable"),
            (Some(project.as_str()), in_temp.to_str().unwrap(), "scratch"),
            (None, in_temp.to_str().unwrap(), "scratch"),
            (None, outside.to_str().unwrap(), manifest.as_str()),
            (
                Some(project.as_str()),
                outside.to_str().unwrap(),
                manifest.as_str(),
            ),
        ] {
            let (read, version) = read_text(&fixture, project, path, 1 << 20).unwrap();
            assert_eq!(read, text, "{path}");
            assert!(!version.is_empty());
        }
    }

    #[test]
    fn reading_a_link_refuses_what_opening_one_would_and_what_is_not_a_file() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-read-refused");
        std::fs::create_dir(directory.join("src")).unwrap();
        std::fs::write(directory.join("big.txt"), "0123456789").unwrap();
        let project = fixture.project("read-refused-project", &directory);
        let gone_dir = ScratchDir::new("path-links-read-refused-gone");
        let outside = Outside::new(&[&gone_dir]);
        let gone = canonical(&gone_dir).join("missing.txt");
        let read_code = |project: Option<&str>, path: &str, limit: usize| {
            let error = read_text(&fixture, project, path, limit).unwrap_err();
            let coded = error.downcast_ref::<CodedError>().unwrap();
            (coded.code, coded.params.get("root").cloned())
        };
        let project = Some(project.as_str());
        assert_eq!(
            read_code(project, "src", 1024).0,
            error_code::UNSUPPORTED_FILE_TYPE
        );
        assert_eq!(
            read_code(project, "missing.txt", 1024),
            (error_code::FILE_NOT_FOUND, Some("project".into()))
        );
        assert_eq!(
            read_code(None, gone.to_str().unwrap(), 1024),
            (error_code::FILE_NOT_FOUND, Some("temporary".into()))
        );
        assert_eq!(
            read_code(None, outside.gone().to_str().unwrap(), 1024),
            (error_code::FILE_NOT_FOUND, Some("host".into()))
        );
        // A relative path may lead out of the project; it is gone like any other.
        assert_eq!(
            read_code(project, "../elsewhere.txt", 1024).0,
            error_code::FILE_NOT_FOUND
        );
        // With no project there is no root to look from.
        assert_eq!(
            read_code(None, "big.txt", 1024).0,
            error_code::PATH_LINK_REFUSED
        );
        assert_eq!(
            read_code(project, "big.txt", 4).0,
            error_code::LIMIT_EXCEEDED
        );
        assert_eq!(
            read_code(Some("unknown"), "big.txt", 1024).0,
            error_code::UNKNOWN_PROJECT
        );
    }

    #[cfg(unix)]
    #[test]
    fn reading_a_link_refuses_a_temp_file_others_could_change() {
        let fixture = Fixture::new();
        let scratch = ScratchDir::new("path-links-read-others");
        std::fs::create_dir(scratch.join("open")).unwrap();
        std::fs::write(scratch.join("open/out.txt"), "x").unwrap();
        std::fs::set_permissions(scratch.join("open"), std::fs::Permissions::from_mode(0o777))
            .unwrap();
        let path = canonical(&scratch).join("open/out.txt");
        let error = read_text(&fixture, None, path.to_str().unwrap(), 1024).unwrap_err();
        let coded = error.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::PATH_LINK_REFUSED);
        assert_eq!(coded.params["cause"], "not_private");
    }

    #[test]
    fn a_plain_file_a_folder_and_a_temp_file_are_opened_but_never_a_launchable_one() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-open-project-file");
        std::fs::write(directory.join("notes.txt"), "x").unwrap();
        std::fs::create_dir(directory.join("src")).unwrap();
        let project = fixture.project("open-project", &directory);
        let attempt = |path: &str| {
            let opened = std::cell::RefCell::new(None);
            let result = open_with(
                &fixture.state,
                Some(&project),
                path,
                |_| false,
                |resolved| {
                    *opened.borrow_mut() = Some(resolved.to_path_buf());
                    Ok(())
                },
            );
            (result, opened.into_inner())
        };
        let (result, opened) = attempt("notes.txt");
        assert!(result.is_ok());
        assert_eq!(opened, Some(canonical(&directory.join("notes.txt"))));
        let (result, opened) = attempt("src");
        assert!(result.is_ok());
        assert_eq!(opened, Some(canonical(&directory.join("src"))));
        let scratch = ScratchDir::new("path-links-open-temp-file");
        std::fs::write(scratch.join("out.txt"), "x").unwrap();
        let in_temp = canonical(&scratch).join("out.txt");
        let (result, opened) = attempt(in_temp.to_str().unwrap());
        assert!(result.is_ok());
        assert_eq!(opened, Some(in_temp));
        // So are a file and a folder anywhere else on the host.
        let outside = Outside::new(&[&scratch]);
        for path in [outside.file(), outside.folder()] {
            let (result, opened) = attempt(path.to_str().unwrap());
            assert!(result.is_ok(), "{}", path.display());
            assert_eq!(opened, Some(path));
        }
    }

    #[test]
    fn a_folder_the_system_calls_a_package_is_never_opened_but_a_plain_one_is() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-open-package");
        std::fs::create_dir_all(directory.join("App.xcodeproj")).unwrap();
        std::fs::create_dir(directory.join("src")).unwrap();
        let project = fixture.project("package-project", &directory);
        let attempt = |path: &str| {
            let opened = std::cell::Cell::new(false);
            let result = open_with(
                &fixture.state,
                Some(&project),
                path,
                |resolved| resolved.extension().is_some_and(|ext| ext == "xcodeproj"),
                |_| {
                    opened.set(true);
                    Ok(())
                },
            );
            (result, opened.get())
        };
        let (result, opened) = attempt("App.xcodeproj");
        assert!(!opened);
        let error = result.unwrap_err();
        let coded = error.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::PATH_LINK_REFUSED);
        assert_eq!(coded.params["cause"], "launchable");
        let (result, opened) = attempt("src");
        assert!(result.is_ok() && opened);
    }

    /// What a reveal of `path` shows, or the error that refuses it.
    fn shown(fixture: &Fixture, project: Option<&str>, path: &str) -> Result<PathBuf> {
        let shown = std::cell::RefCell::new(None);
        reveal_with(&fixture.state, project, path, |shown_path| {
            *shown.borrow_mut() = Some(shown_path.to_path_buf());
            Ok(())
        })?;
        Ok(shown.into_inner().expect("shown on success"))
    }

    fn code_of(error: &anyhow::Error) -> crate::protocol::Code {
        error.downcast_ref::<CodedError>().unwrap().code
    }

    #[test]
    fn revealing_takes_a_file_a_folder_and_a_path_anywhere_on_the_host_and_refuses_what_is_gone() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-reveal");
        std::fs::write(directory.join("notes.txt"), "x").unwrap();
        std::fs::write(directory.join("run.command"), "x").unwrap();
        std::fs::create_dir(directory.join("src")).unwrap();
        let project = fixture.project("reveal-project", &directory);
        // Files that `open` refuses as viewer-only are shown, since nothing is launched.
        for (path, expected) in [
            ("notes.txt", directory.join("notes.txt")),
            ("run.command", directory.join("run.command")),
            ("src", directory.join("src")),
        ] {
            assert_eq!(
                shown(&fixture, Some(&project), path).unwrap(),
                canonical(&expected),
                "{path}"
            );
        }
        for (path, code) in [
            ("../elsewhere.txt", error_code::FILE_NOT_FOUND),
            ("missing.txt", error_code::FILE_NOT_FOUND),
        ] {
            let error = shown(&fixture, Some(&project), path).unwrap_err();
            assert_eq!(code_of(&error), code, "{path}");
        }
        // A file anywhere on the host is shown, as itself.
        let outside = Outside::new(&[]);
        assert_eq!(
            shown(&fixture, Some(&project), outside.file().to_str().unwrap()).unwrap(),
            outside.file()
        );
        let error =
            reveal_with(&fixture.state, Some("unknown"), "notes.txt", |_| Ok(())).unwrap_err();
        assert_eq!(code_of(&error), error_code::UNKNOWN_PROJECT);
    }

    #[test]
    fn revealing_selects_a_symbolic_link_below_the_root_by_its_own_name() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-reveal-link");
        let outside = ScratchDir::new("path-links-reveal-link-outside");
        let base = canonical(&directory);
        std::fs::write(directory.join("notes.txt"), "x").unwrap();
        std::fs::create_dir(directory.join("src")).unwrap();
        std::fs::write(directory.join("src/inner.txt"), "x").unwrap();
        crate::test_support::symlink("notes.txt", directory.join("file-link")).unwrap();
        crate::test_support::symlink("src", directory.join("folder-link")).unwrap();
        crate::test_support::symlink("inner.txt", directory.join("src/inner-link")).unwrap();
        crate::test_support::symlink(directory.join("src"), directory.join("via")).unwrap();
        // A folder linked out of the root, holding a link back into it.
        crate::test_support::symlink(&*outside, directory.join("out")).unwrap();
        crate::test_support::symlink(directory.join("notes.txt"), outside.join("back")).unwrap();
        let project = fixture.project("reveal-link-project", &directory);
        let absolute = |name: &str| base.join(name).to_string_lossy().into_owned();
        for (path, expected) in [
            // The link itself, written absolute or relative, to a file or to a folder.
            (absolute("file-link"), base.join("file-link")),
            ("file-link".to_string(), base.join("file-link")),
            (absolute("folder-link"), base.join("folder-link")),
            ("folder-link".to_string(), base.join("folder-link")),
            // A link below a linked folder that stays inside the root.
            ("via/inner-link".to_string(), base.join("src/inner-link")),
            // A `..` that is folded before the link at the end is looked at.
            ("src/../file-link".to_string(), base.join("file-link")),
            // Not a link: what it resolves to.
            ("src/inner.txt".to_string(), base.join("src/inner.txt")),
            ("via/inner.txt".to_string(), base.join("src/inner.txt")),
            // A last component of `.` or `..` names a folder as well.
            ("folder-link/.".to_string(), base.join("src")),
            ("folder-link/..".to_string(), base.clone()),
            // A trailing slash names the folder, not the link.
            ("folder-link/".to_string(), base.join("src")),
            (absolute("folder-link/"), base.join("src")),
            // The link's folder is outside the root: shown as what it resolves to.
            ("out/back".to_string(), base.join("notes.txt")),
        ] {
            assert_eq!(
                shown(&fixture, Some(&project), &path).unwrap(),
                expected,
                "{path}"
            );
        }
    }

    #[test]
    fn revealing_a_root_spelled_through_a_link_opens_the_root_folder() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-reveal-root");
        let aliases = ScratchDir::new("path-links-reveal-root-alias");
        let base = canonical(&directory);
        let alias = canonical(&aliases).join("alias");
        crate::test_support::symlink(&*directory, &alias).unwrap();
        std::fs::write(directory.join("notes.txt"), "x").unwrap();
        crate::test_support::symlink("notes.txt", directory.join("file-link")).unwrap();
        let project = fixture.project("reveal-root-project", &alias);
        for (path, expected) in [
            // The root itself, spelled as stored: not the alias in its parent.
            (alias.to_string_lossy().into_owned(), base.clone()),
            // A link in it, spelled through the alias.
            (
                alias.join("file-link").to_string_lossy().into_owned(),
                base.join("file-link"),
            ),
        ] {
            assert_eq!(
                shown(&fixture, Some(&project), &path).unwrap(),
                expected,
                "{path}"
            );
        }
    }

    #[test]
    fn revealing_selects_links_in_each_of_two_roots() {
        let fixture = Fixture::new();
        let repository = repo_with("path-links-reveal-two", &[(b"a.ts", b"x")]);
        let linked_parent = ScratchDir::new("path-links-reveal-two-linked");
        let linked = canonical(&linked_parent).join("tree");
        git(
            &repository,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "reveal-two",
                linked.to_str().unwrap(),
            ],
        );
        let main = canonical(&repository);
        crate::test_support::symlink("a.ts", main.join("main-link")).unwrap();
        crate::test_support::symlink("a.ts", linked.join("tree-link")).unwrap();
        let project = fixture.project("reveal-two-project", &repository);
        for (path, expected) in [
            ("main-link".to_string(), main.join("main-link")),
            ("tree-link".to_string(), linked.join("tree-link")),
            (
                linked.join("tree-link").to_string_lossy().into_owned(),
                linked.join("tree-link"),
            ),
        ] {
            assert_eq!(
                shown(&fixture, Some(&project), &path).unwrap(),
                expected,
                "{path}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn revealing_selects_a_link_in_a_temp_root_only_when_it_is_private() {
        let fixture = Fixture::new();
        let scratch = ScratchDir::new("path-links-reveal-temp");
        let base = canonical(&scratch);
        std::fs::create_dir(scratch.join("mine")).unwrap();
        std::fs::write(scratch.join("mine/a.txt"), "x").unwrap();
        crate::test_support::symlink("a.txt", scratch.join("mine/own")).unwrap();
        // A folder others can write to, as another user's would be, holding a link into the
        // private one.
        std::fs::create_dir(scratch.join("evil")).unwrap();
        std::fs::set_permissions(scratch.join("evil"), std::fs::Permissions::from_mode(0o777))
            .unwrap();
        crate::test_support::symlink("../mine/a.txt", scratch.join("evil/l")).unwrap();
        for (path, expected) in [
            ("mine/own", base.join("mine/own")),
            ("evil/l", base.join("mine/a.txt")),
        ] {
            let absolute = base.join(path);
            assert_eq!(
                shown(&fixture, None, absolute.to_str().unwrap()).unwrap(),
                expected,
                "{path}"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn revealing_selects_a_link_whose_mode_is_open_to_everyone() {
        let fixture = Fixture::new();
        let scratch = ScratchDir::new("path-links-reveal-mode");
        let base = canonical(&scratch);
        std::fs::create_dir(scratch.join("mine")).unwrap();
        std::fs::write(scratch.join("mine/a.txt"), "x").unwrap();
        let link = scratch.join("mine/own");
        std::os::unix::fs::symlink("a.txt", &link).unwrap();
        // A link's mode is 0777 on Linux; the same is set here, where `symlink` honours the umask.
        let path = std::ffi::CString::new(link.to_str().unwrap()).unwrap();
        // SAFETY: `path` is a valid NUL-terminated string that outlives the call.
        assert_eq!(unsafe { libc::lchmod(path.as_ptr(), 0o777) }, 0);
        let mode = std::fs::symlink_metadata(&link)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o777);
        assert_eq!(
            shown(&fixture, None, base.join("mine/own").to_str().unwrap()).unwrap(),
            base.join("mine/own")
        );
    }

    #[test]
    fn revealing_selects_a_link_in_the_root_that_holds_it_not_the_one_holding_its_target() {
        let fixture = Fixture::new();
        let repository = repo_with("path-links-reveal-nested", &[(b"a.ts", b"x")]);
        let main = canonical(&repository);
        std::fs::create_dir(main.join(".worktrees")).unwrap();
        let linked = main.join(".worktrees/w");
        git(
            &repository,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "reveal-nested",
                linked.to_str().unwrap(),
            ],
        );
        // A link in the project to a file of the worktree below it, and the reverse.
        crate::test_support::symlink(".worktrees/w/a.ts", main.join("down")).unwrap();
        crate::test_support::symlink("../../a.ts", linked.join("up")).unwrap();
        let project = fixture.project("reveal-nested-project", &repository);
        for (path, expected) in [
            ("down".to_string(), main.join("down")),
            ("up".to_string(), linked.join("up")),
            (
                linked.join("up").to_string_lossy().into_owned(),
                linked.join("up"),
            ),
        ] {
            assert_eq!(
                shown(&fixture, Some(&project), &path).unwrap(),
                expected,
                "{path}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn revealing_clears_a_folder_link_in_a_project_under_a_temp_root_when_its_folder_is_open() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-reveal-open-folder");
        let base = canonical(&directory);
        std::fs::create_dir(directory.join("src")).unwrap();
        std::fs::create_dir(directory.join("shared")).unwrap();
        std::fs::set_permissions(
            directory.join("shared"),
            std::fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        crate::test_support::symlink("../src", directory.join("shared/folder-link")).unwrap();
        crate::test_support::symlink("../src/a.txt", directory.join("shared/file-link")).unwrap();
        std::fs::write(directory.join("src/a.txt"), "x").unwrap();
        let project = fixture.project("reveal-open-folder-project", &directory);
        for (path, expected) in [
            // A folder link: judged like a folder of the project, so shown as what it leads to.
            ("shared/folder-link", base.join("src")),
            // A file link is not checked.
            ("shared/file-link", base.join("shared/file-link")),
        ] {
            assert_eq!(
                shown(&fixture, Some(&project), path).unwrap(),
                expected,
                "{path}"
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn revealing_a_path_spelled_under_tmp_shows_the_resolved_entry() {
        let fixture = Fixture::new();
        let name = format!("octoboardd-reveal-tmp-{}", std::process::id());
        let directory = PathBuf::from("/tmp").join(&name);
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(directory.join("a.txt"), "x").unwrap();
        std::os::unix::fs::symlink("a.txt", directory.join("l")).unwrap();
        let resolved = PathBuf::from("/private/tmp").join(&name);
        let results = [
            // The root, spelled through the system's link: the folder, not the link in `/`.
            ("/tmp", PathBuf::from("/private/tmp")),
            (&*format!("/tmp/{name}/l"), resolved.join("l")),
            (&*format!("/tmp/{name}/a.txt"), resolved.join("a.txt")),
        ]
        .map(|(path, expected)| (shown(&fixture, None, path), expected, path.to_string()));
        std::fs::remove_dir_all(&directory).unwrap();
        for (result, expected, path) in results {
            assert_eq!(result.unwrap(), expected, "{path}");
        }
    }

    #[test]
    fn opening_a_path_in_two_roots_is_refused_as_ambiguous() {
        let fixture = Fixture::new();
        let repository = repo_with("path-links-ambiguous", &[(b"a.ts", b"x")]);
        let linked_parent = ScratchDir::new("path-links-ambiguous-linked");
        let linked = linked_parent.join("tree");
        git(
            &repository,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "ambiguous",
                linked.to_str().unwrap(),
            ],
        );
        let project = fixture.project("ambiguous-project", &repository);
        let error = open_with(
            &fixture.state,
            Some(&project),
            "a.ts",
            |_| false,
            |_| panic!("must not be opened"),
        )
        .unwrap_err();
        let coded = error.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::PATH_LINK_REFUSED);
        assert_eq!(coded.params["cause"], "ambiguous");
    }

    #[cfg(unix)]
    #[test]
    fn a_network_path_is_answered_none_and_the_other_paths_of_the_request_are_answered() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-probe-network");
        std::fs::write(directory.join("a.ts"), b"x").unwrap();
        let project = fixture.project("network-project", &directory);
        let links = probe(
            &fixture.state,
            Some(&project),
            vec![NET_FILE.into(), "a.ts".into(), "/net".into()],
        )
        .unwrap();
        let kinds: Vec<_> = links.iter().map(|link| link.kind).collect();
        assert_eq!(
            kinds,
            [PathLinkKind::None, PathLinkKind::File, PathLinkKind::None]
        );
        assert_eq!(links[0].path, NET_FILE);
    }

    #[cfg(unix)]
    #[test]
    fn opening_showing_or_reading_a_network_path_is_refused_as_other_and_touches_nothing() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-open-network");
        let project = fixture.project("open-network-project", &directory);
        for project in [Some(project.as_str()), None] {
            let refused = [
                open_with(
                    &fixture.state,
                    project,
                    NET_FILE,
                    |_| false,
                    |_| panic!("must not be opened"),
                )
                .unwrap_err(),
                reveal_with(&fixture.state, project, NET_FILE, |_| {
                    panic!("must not be shown")
                })
                .unwrap_err(),
                read_text(&fixture, project, NET_FILE, 1 << 20).unwrap_err(),
            ];
            for error in refused {
                let coded = error.downcast_ref::<CodedError>().unwrap();
                assert_eq!(coded.code, error_code::PATH_LINK_REFUSED);
                assert_eq!(coded.params["cause"], "other");
                assert_eq!(coded.params["path"], NET_FILE);
            }
        }
    }

    /// What `paths` are answered, as the kinds of the links.
    fn kinds(fixture: &Fixture, project: &str, paths: &[&str]) -> Vec<PathLinkKind> {
        probe(
            &fixture.state,
            Some(project),
            paths.iter().map(|path| path.to_string()).collect(),
        )
        .unwrap()
        .into_iter()
        .map(|link| link.kind)
        .collect()
    }

    #[cfg(unix)]
    #[test]
    fn a_place_whose_earlier_look_is_still_stuck_is_answered_none_and_the_other_places_are_answered(
    ) {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-stuck");
        std::fs::write(directory.join("a.ts"), b"x").unwrap();
        std::fs::write(directory.join("b.ts"), b"x").unwrap();
        let project = fixture.project("stuck-project", &directory);
        let roots = crate::path_links::project_roots(&fixture.state, &project, false);
        // The top of the tree is a place of its own, whichever volume the scratch folder is on.
        let top = "/";
        let absolute = canonical(&directory).join("a.ts");
        let absolute = absolute.to_str().unwrap();
        let place = place_of("a.ts", None, &roots);
        assert_ne!(place, place_of(top, None, &roots));
        assert_eq!(place_of(absolute, None, &roots), place);
        let paths = ["a.ts", "b.ts", absolute, top];
        let (file, none, folder) = (
            PathLinkKind::File,
            PathLinkKind::None,
            PathLinkKind::Directory,
        );
        assert_eq!(
            kinds(&fixture, &project, &paths),
            [file, file, file, folder]
        );
        // The look at that place that outran its deadline is still running on its thread.
        let thread = std::sync::Arc::new(());
        fixture
            .state
            .path_probes
            .record(place.clone(), std::sync::Arc::downgrade(&thread));
        assert!(fixture.state.path_probes.is_stuck(&place));
        // Every path of the place is `none`, whichever way it is spelled, and no other is.
        assert_eq!(
            kinds(&fixture, &project, &paths),
            [none, none, none, folder],
            "every ask of the stuck place"
        );
        // Once its thread has ended it is looked at again.
        drop(thread);
        assert!(!fixture.state.path_probes.is_stuck(&place));
        assert_eq!(
            kinds(&fixture, &project, &paths),
            [file, file, file, folder]
        );
    }

    #[test]
    fn a_project_whose_roots_are_still_being_found_answers_every_path_none() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-roots-stuck");
        std::fs::write(directory.join("a.ts"), b"x").unwrap();
        let project = fixture.project("roots-stuck-project", &directory);
        let absolute = canonical(&directory).join("a.ts");
        let paths = ["a.ts", absolute.to_str().unwrap()];
        let thread = std::sync::Arc::new(());
        fixture
            .state
            .path_root_lookups
            .record(project.clone(), std::sync::Arc::downgrade(&thread));
        assert_eq!(kinds(&fixture, &project, &paths), [PathLinkKind::None; 2]);
        // Another project is not held back by it, nor is a request that names none.
        assert_eq!(
            kinds_without_project(&fixture, &paths[1..]),
            [PathLinkKind::File]
        );
        drop(thread);
        assert_eq!(kinds(&fixture, &project, &paths), [PathLinkKind::File; 2]);
    }

    fn kinds_without_project(fixture: &Fixture, paths: &[&str]) -> Vec<PathLinkKind> {
        probe(
            &fixture.state,
            None,
            paths.iter().map(|path| path.to_string()).collect(),
        )
        .unwrap()
        .into_iter()
        .map(|link| link.kind)
        .collect()
    }

    #[test]
    fn a_probe_answers_each_path_asked_in_order_repeats_included_and_stalls_no_place() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-not-stalled");
        std::fs::write(directory.join("a.ts"), b"x").unwrap();
        let project = fixture.project("not-stalled-project", &directory);
        let paths: Vec<String> = ["a.ts", "missing.ts", "a.ts", "/no/such/file"]
            .map(String::from)
            .to_vec();
        let links = probe(&fixture.state, Some(&project), paths.clone()).unwrap();
        // One answer for each path asked, repeated ones included, in the order asked.
        let answered: Vec<_> = links
            .iter()
            .map(|link| (link.path.as_str(), link.kind))
            .collect();
        assert_eq!(
            answered,
            [
                ("a.ts", PathLinkKind::File),
                ("missing.ts", PathLinkKind::None),
                ("a.ts", PathLinkKind::File),
                ("/no/such/file", PathLinkKind::None),
            ]
        );
        // Everything was found, so no look is left running: the places are not stuck, and those
        // that answered were answered by a look that has ended.
        let roots = crate::path_links::project_roots(&fixture.state, &project, false);
        for path in &paths {
            let place = place_of(path, None, &roots);
            assert!(!fixture.state.path_probes.is_stuck(&place), "{path}");
        }
        assert!(!fixture.state.path_root_lookups.is_stuck(project.as_str()));
    }

    #[cfg(unix)]
    #[test]
    fn what_could_be_launched_is_judged_from_the_top_of_the_tree_outside_every_root() {
        use std::cell::RefCell;
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-top-project");
        let project = fixture.project("top-project", &directory);
        let outside = Outside::new(&[]);
        let base = outside.folder();
        // A file with an executable bit, a file under a bundle folder, a bundle folder.
        std::fs::write(base.join("tool"), "x").unwrap();
        std::fs::set_permissions(base.join("tool"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        std::fs::create_dir_all(base.join("Thing.app/inner")).unwrap();
        std::fs::write(base.join("Thing.app/inner/a.txt"), "x").unwrap();
        let tool = base.join("tool").to_string_lossy().into_owned();
        let inside = base
            .join("Thing.app/inner/a.txt")
            .to_string_lossy()
            .into_owned();
        let bundle = base.join("Thing.app").to_string_lossy().into_owned();
        let plain = outside.file().to_string_lossy().into_owned();
        let links = probe(
            &fixture.state,
            Some(&project),
            [&plain, &tool, &inside, &bundle]
                .map(String::clone)
                .to_vec(),
        )
        .unwrap();
        let seen: Vec<_> = links
            .iter()
            .map(|link| (link.kind, link.viewer_only))
            .collect();
        assert_eq!(
            seen,
            [
                (PathLinkKind::File, false),
                (PathLinkKind::File, true),
                (PathLinkKind::File, true),
                (PathLinkKind::None, false),
            ]
        );
        // `open` refuses a launchable file and folder alike; `reveal` shows the file, not the folder.
        for path in [&tool, &inside, &bundle] {
            let error = open_with(
                &fixture.state,
                Some(&project),
                path,
                |_| false,
                |_| panic!("{path} must not be opened"),
            )
            .unwrap_err();
            let coded = error.downcast_ref::<CodedError>().unwrap();
            assert_eq!(coded.code, error_code::PATH_LINK_REFUSED, "{path}");
            assert_eq!(coded.params["cause"], "launchable", "{path}");
        }
        let shown = RefCell::new(Vec::new());
        for path in [&tool, &inside] {
            reveal_with(&fixture.state, Some(&project), path, |shown_path| {
                shown.borrow_mut().push(shown_path.to_path_buf());
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(
            shown.into_inner(),
            [base.join("tool"), base.join("Thing.app/inner/a.txt")]
        );
        let error = reveal_with(&fixture.state, Some(&project), &bundle, |_| Ok(())).unwrap_err();
        assert_eq!(code_of(&error), error_code::PATH_LINK_REFUSED);
        // Reading launches nothing.
        let (text, _) = read_text(&fixture, Some(&project), &tool, 1 << 20).unwrap();
        assert_eq!(text, "x");
    }

    #[test]
    fn a_sibling_of_the_project_is_reachable_with_no_view_and_can_be_read_and_opened() {
        let fixture = Fixture::new();
        let outer = ScratchDir::new("path-links-probe-sibling");
        // No temp root of the test holds it, so it is anywhere else on the host.
        let _temp = with_temp_roots(&[]);
        std::fs::create_dir_all(outer.join("proj")).unwrap();
        std::fs::create_dir_all(outer.join("sib")).unwrap();
        std::fs::write(outer.join("sib/x.txt"), "beside").unwrap();
        // Closed to others whatever the umask, as the temp root's rule asks of what it holds.
        #[cfg(unix)]
        for (path, mode) in [
            ("", 0o755),
            ("proj", 0o755),
            ("sib", 0o755),
            ("sib/x.txt", 0o644),
        ] {
            std::fs::set_permissions(outer.join(path), std::fs::Permissions::from_mode(mode))
                .unwrap();
        }
        let project = fixture.project("sibling-project", &outer.join("proj"));
        let links = probe(
            &fixture.state,
            Some(&project),
            vec![
                "../sib/x.txt".into(),
                "../sib".into(),
                "../sib/gone.txt".into(),
            ],
        )
        .unwrap();
        assert_eq!(
            (links[0].kind, links[1].kind, links[2].kind),
            (
                PathLinkKind::File,
                PathLinkKind::Directory,
                PathLinkKind::None
            )
        );
        assert!(links
            .iter()
            .all(|link| link.view.is_none() && !link.viewer_only));
        let (text, _) = read_text(&fixture, Some(&project), "../sib/x.txt", 1 << 20).unwrap();
        assert_eq!(text, "beside");
        let opened = std::cell::RefCell::new(None);
        open_with(
            &fixture.state,
            Some(&project),
            "../sib/x.txt",
            |_| false,
            |path| {
                *opened.borrow_mut() = Some(path.to_path_buf());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            opened.into_inner(),
            Some(canonical(&outer).join("sib/x.txt"))
        );
    }

    #[test]
    fn the_refusals_are_raised_under_their_codes() {
        let denied = Refusal::Denied("Permission denied (os error 13)".into()).into_error("a/b");
        let coded = denied.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::PERMISSION_DENIED);
        assert_eq!(coded.params["path"], "a/b");
        assert_eq!(coded.params["detail"], "Permission denied (os error 13)");
        for (refusal, code, cause) in [
            (
                Refusal::NotPrivate,
                error_code::PATH_LINK_REFUSED,
                Some("not_private"),
            ),
            (
                Refusal::NotAFileOrFolder,
                error_code::PATH_LINK_REFUSED,
                Some("not_a_file_or_folder"),
            ),
            (Refusal::Other, error_code::PATH_LINK_REFUSED, Some("other")),
            (
                Refusal::Unanswered,
                error_code::PATH_LINK_REFUSED,
                Some("unanswered"),
            ),
            (
                Refusal::Unreadable("Input/output error (os error 5)".into()),
                error_code::PATH_LINK_REFUSED,
                Some("unreadable"),
            ),
        ] {
            let detail = matches!(refusal, Refusal::Unreadable(_))
                .then_some("Input/output error (os error 5)");
            let error = refusal.into_error("a/b");
            let coded = error.downcast_ref::<CodedError>().unwrap();
            assert_eq!(coded.code, code);
            assert_eq!(coded.params.get("cause").map(String::as_str), cause);
            assert_eq!(coded.params.get("detail").map(String::as_str), detail);
        }
    }
}

#[test]
fn a_spelling_names_a_folder_by_a_trailing_separator_dot_or_dot_dot() {
    for raw in ["a/", "a/.", "a/..", ".", "..", "~/"] {
        assert!(names_directory(raw), "{raw}");
    }
    for raw in ["a", "a/b.txt", "a./b", "a/.hidden", "a/b..", "~"] {
        assert!(!names_directory(raw), "{raw}");
    }
    // A backslash separates names only where the host's paths take it.
    assert_eq!(names_directory("a\\"), cfg!(windows));
    assert_eq!(names_directory("a\\.."), cfg!(windows));
}

#[test]
fn a_tilde_and_a_separator_the_host_takes_start_a_path_below_home() {
    let home = project_roots(&[&ScratchDir::new("path-links-tilde-home")]).canonical[0].clone();
    let below = |raw: &str| absolute_spelling(raw, Some(&home));
    assert_eq!(below("~/a/b"), Ok(Some(home.join("a/b"))));
    // Leading separators do not make the rest absolute.
    assert_eq!(below("~//a"), Ok(Some(home.join("a"))));
    assert_eq!(below("~/"), Ok(Some(home.clone())));
    // Not a path below home: a name that starts with a tilde, the tilde alone.
    assert_eq!(below("~a/b"), Ok(None));
    assert_eq!(below("~"), Ok(None));
    assert_eq!(absolute_spelling("~/a", None), Err(()));
    // A backslash is a separator only where the host's paths take it.
    if cfg!(windows) {
        assert_eq!(below("~\\a\\b"), Ok(Some(home.join("a").join("b"))));
        assert_eq!(below("~\\\\a"), Ok(Some(home.join("a"))));
    } else {
        assert_eq!(below("~\\a"), Ok(None));
    }
}

/// A scratch folder is a "network" place for a walk that judges by `names`; a link whose target
/// spells a real one (`/net/...`) is refused by the spelling the walk reaches, not by a look.
#[cfg(unix)]
#[test]
fn a_link_into_a_network_place_is_refused_before_anything_is_looked_at() {
    let project = ScratchDir::new("path-links-link-network");
    let elsewhere = ScratchDir::new("path-links-link-network-elsewhere");
    let scratch = ScratchDir::new("path-links-link-network-temp");
    std::fs::write(project.join("a.txt"), "x").unwrap();
    let roots = project_roots(&[&project]);
    let temp = temp_of(&scratch);
    let share = "/net/octoboard-no-such-host/share";
    let base = |dir: &ScratchDir| canonical(dir);
    // Links in the project, in a temp root and elsewhere, straight to the share, through another
    // link, to the tree itself and relative with dots up to it.
    let ups = "../".repeat(canonical(&project).components().count());
    for dir in [&project, &scratch, &elsewhere] {
        symlink(share, dir.join("to-share")).unwrap();
        symlink("to-share", dir.join("chained")).unwrap();
        symlink("/net", dir.join("to-tree")).unwrap();
        symlink(
            format!("{ups}net/octoboard-no-such-host"),
            dir.join("dotted"),
        )
        .unwrap();
    }
    let linked: Vec<String> = [&project, &scratch, &elsewhere]
        .into_iter()
        .flat_map(|dir| {
            ["to-share", "chained", "to-tree", "dotted"]
                .map(|name| base(dir).join(name).to_string_lossy().into_owned())
        })
        .collect();
    for raw in &linked {
        for spelling in [raw.clone(), format!("{raw}/a.txt"), format!("{raw}/.")] {
            assert_eq!(
                reach(&spelling, None, &temp, &|| roots.clone()),
                Err(Unreached::Refused(Refusal::Other)),
                "{spelling}"
            );
        }
    }
    // Written relative to the project, the join leads to the link all the same.
    for raw in ["to-share", "chained/a.txt", "to-tree", "dotted/x"] {
        assert_eq!(refusal(raw, &temp, &roots), Refusal::Other, "{raw}");
    }
    // A link to a folder that is not a network place is followed as before.
    std::fs::create_dir(elsewhere.join("folder")).unwrap();
    symlink(elsewhere.join("folder"), project.join("fine")).unwrap();
    assert_eq!(
        kind("fine", None, &temp, &roots),
        Some(PathLinkKind::Directory)
    );
}

/// Looks that outrun a shortened deadline: `LOOKS_OVERRIDE` holds the looks of chosen paths until
/// the test releases them, and every deadline is as short as the test says.
mod stalled_looks {
    use std::time::Duration;

    use super::super::looks::{HeldLooks, LooksTrial, LOOKS_OVERRIDE};
    use super::with_state::read_text;
    use super::*;
    use crate::browse::tests::Fixture;
    use crate::session::wait::wait_for;
    use crate::test_support::PATIENCE;

    /// Long enough that a place's thread has begun its look by the time it ends on a loaded
    /// machine, short enough that a test waits it out cheaply.
    const SHORT_DEADLINE: Duration = Duration::from_millis(500);

    /// Holds the looks of `paths` and shortens the deadlines on the test's own thread until
    /// dropped, which also lets every held look go, so no thread outlives the test blocked.
    struct Held(std::sync::Arc<HeldLooks>);

    impl Held {
        fn new(paths: &[&str], projects: &[&str]) -> Self {
            let held = HeldLooks::new(paths, projects);
            LOOKS_OVERRIDE.set(Some(LooksTrial {
                deadline: SHORT_DEADLINE,
                held: held.clone(),
            }));
            Self(held)
        }

        /// How many looks of a held path began.
        fn began(&self) -> usize {
            self.0.began.load(std::sync::atomic::Ordering::SeqCst)
        }

        /// How many lookups of the roots of a held project began.
        fn roots_began(&self) -> usize {
            self.0.roots_began.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn release(&self) {
            self.0.release();
        }
    }

    impl Drop for Held {
        fn drop(&mut self) {
            LOOKS_OVERRIDE.set(None);
            self.0.release();
        }
    }

    fn kinds(fixture: &Fixture, project: &str, paths: &[&str]) -> Vec<PathLinkKind> {
        probe(
            &fixture.state,
            Some(project),
            paths.iter().map(|path| path.to_string()).collect(),
        )
        .unwrap()
        .into_iter()
        .map(|link| link.kind)
        .collect()
    }

    fn refused_as_unanswered(error: &anyhow::Error, path: &str) {
        let coded = error.downcast_ref::<CodedError>().unwrap();
        assert_eq!(coded.code, error_code::PATH_LINK_REFUSED);
        assert_eq!(coded.params["cause"], "unanswered");
        assert!(!coded.params.contains_key("detail"));
        assert_eq!(coded.params["path"], path);
    }

    #[cfg(unix)]
    #[test]
    fn a_stalled_look_is_answered_none_recorded_stuck_skipped_and_answered_again_once_released() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-held-probe");
        std::fs::write(directory.join("a.ts"), b"x").unwrap();
        let project = fixture.project("held-probe-project", &directory);
        let roots = crate::path_links::project_roots(&fixture.state, &project, false);
        let place = place_of("a.ts", None, &roots);
        // The top of the tree is a place of its own, whichever volume the scratch folder is on.
        let paths = ["a.ts", "/"];
        assert_ne!(place, place_of("/", None, &roots));
        let held = Held::new(&["a.ts"], &[]);
        let (file, none, folder) = (
            PathLinkKind::File,
            PathLinkKind::None,
            PathLinkKind::Directory,
        );
        // The stalled path is `none` and the other place is answered. A pass whose thread had not
        // begun the look by the deadline tested nothing, and is run again.
        assert!(
            wait_for(PATIENCE, || {
                let answered = kinds(&fixture, &project, &paths);
                held.began() == 1 && answered == [none, folder]
            }),
            "no look began, or the other place was not answered"
        );
        assert!(
            wait_for(PATIENCE, || fixture.state.path_probes.is_stuck(&place)),
            "the stalled place is recorded stuck"
        );
        // While it is stuck no new look is started for it, and the rest is answered all the same.
        assert_eq!(kinds(&fixture, &project, &paths), [none, folder]);
        assert_eq!(held.began(), 1);
        // Once the stalled look is released its thread ends, and the place is looked at again.
        held.release();
        assert!(
            wait_for(PATIENCE, || !fixture.state.path_probes.is_stuck(&place)),
            "the released look ends"
        );
        assert_eq!(kinds(&fixture, &project, &paths), [file, folder]);
        assert_eq!(held.began(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn opening_reading_and_showing_a_stalled_look_is_refused_in_time_and_works_once_released() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-held-click");
        std::fs::write(directory.join("a.ts"), b"x").unwrap();
        let project = fixture.project("held-click-project", &directory);
        let roots = crate::path_links::project_roots(&fixture.state, &project, false);
        let place = place_of("a.ts", None, &roots);
        // No temp root, so what `open` checks there does not apply to the scratch folder.
        let _no_temp = with_temp_roots(&[]);
        let held = Held::new(&["a.ts"], &[]);
        let project = Some(project.as_str());
        let read_it = || read_text(&fixture, project, "a.ts", 1 << 20);
        let open_it = || {
            open_with(
                &fixture.state,
                project,
                "a.ts",
                |_| false,
                |_| panic!("must not be opened"),
            )
        };
        let reveal_it = || {
            reveal_with(&fixture.state, project, "a.ts", |_| {
                panic!("must not be shown")
            })
        };
        // The first look outruns the deadline: refused, not hung, and the place is recorded stuck.
        // A look that had not begun by the deadline tested nothing, and is run again.
        let mut error = None;
        assert!(
            wait_for(PATIENCE, || {
                error = read_it().err();
                held.began() == 1
            }),
            "no look began"
        );
        refused_as_unanswered(&error.unwrap(), "a.ts");
        assert!(
            wait_for(PATIENCE, || fixture.state.path_probes.is_stuck(&place)),
            "the stalled place is recorded stuck"
        );
        // Every route is now refused at once, with no look of its own.
        refused_as_unanswered(&read_it().unwrap_err(), "a.ts");
        refused_as_unanswered(&open_it().unwrap_err(), "a.ts");
        refused_as_unanswered(&reveal_it().unwrap_err(), "a.ts");
        refused_as_unanswered(
            &locate(&fixture.state, project, "a.ts").unwrap_err(),
            "a.ts",
        );
        assert_eq!(held.began(), 1);
        // Released, the place is looked at again.
        held.release();
        assert!(wait_for(PATIENCE, || !fixture
            .state
            .path_probes
            .is_stuck(&place)));
        assert_eq!(read_it().unwrap().0, "x");
        assert_eq!(
            locate(&fixture.state, project, "a.ts").unwrap().kind,
            PathLinkKind::File
        );
        let shown = std::cell::Cell::new(false);
        reveal_with(&fixture.state, project, "a.ts", |_| {
            shown.set(true);
            Ok(())
        })
        .unwrap();
        assert!(shown.get());
        let opened = std::cell::Cell::new(false);
        let outcome = open_with(
            &fixture.state,
            project,
            "a.ts",
            |_| false,
            |_| {
                opened.set(true);
                Ok(())
            },
        );
        // The scratch folder is the daemon's own, so the open reaches the opener.
        assert!(outcome.is_ok() && opened.get(), "{outcome:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_stalled_roots_lookup_refuses_a_click_in_time_and_the_project_is_recorded_stuck() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-held-roots");
        std::fs::write(directory.join("a.ts"), b"x").unwrap();
        let project = fixture.project("held-roots-project", &directory);
        // No temp root, so what `open` checks there does not apply to the scratch folder.
        let _no_temp = with_temp_roots(&[]);
        let held = Held::new(&[], &[&project]);
        let id = Some(project.as_str());
        let read_it = || read_text(&fixture, id, "a.ts", 1 << 20);
        let open_it = || {
            open_with(
                &fixture.state,
                id,
                "a.ts",
                |_| false,
                |_| panic!("must not be opened"),
            )
        };
        let reveal_it = || reveal_with(&fixture.state, id, "a.ts", |_| panic!("must not be shown"));
        // The lookup of the roots outruns the deadline: refused, not hung. A lookup that had not
        // begun by the deadline tested nothing, and is run again.
        let mut error = None;
        assert!(
            wait_for(PATIENCE, || {
                error = read_it().err();
                held.roots_began() == 1
            }),
            "no lookup began"
        );
        refused_as_unanswered(&error.unwrap(), "a.ts");
        assert!(
            wait_for(PATIENCE, || fixture
                .state
                .path_root_lookups
                .is_stuck(project.as_str())),
            "the project is recorded stuck"
        );
        // Every route is now refused at once, with no lookup of its own.
        refused_as_unanswered(&read_it().unwrap_err(), "a.ts");
        refused_as_unanswered(&open_it().unwrap_err(), "a.ts");
        refused_as_unanswered(&reveal_it().unwrap_err(), "a.ts");
        assert_eq!(held.roots_began(), 1);
        // The path itself was never looked at, so its place is not stuck.
        let roots = crate::path_links::project_roots(&fixture.state, &project, false);
        let place = place_of("a.ts", None, &roots);
        assert!(!place.as_os_str().is_empty());
        assert!(!fixture.state.path_probes.is_stuck(&place));
        held.release();
        assert!(wait_for(PATIENCE, || !fixture
            .state
            .path_root_lookups
            .is_stuck(project.as_str())));
        assert_eq!(read_it().unwrap().0, "x");
    }
}

#[cfg(unix)]
#[test]
fn a_prefix_that_holds_a_root_is_allowed_in_passing_and_never_as_where_the_walk_ends() {
    use super::resolve::refused_prefix;
    let roots = vec![PathBuf::from("/net/h/proj")];
    let refused = |prefix: &str, more: bool| refused_prefix(Path::new(prefix), more, &roots);
    // Folders above the root are passed through, and refused where a link lands on them.
    for ancestor in ["/net", "/net/h"] {
        assert!(!refused(ancestor, true), "{ancestor} in passing");
        assert!(refused(ancestor, false), "{ancestor} as the end");
    }
    // The root and what is inside it are looked at as they are.
    for inside in ["/net/h/proj", "/net/h/proj/x"] {
        assert!(!refused(inside, true), "{inside}");
        assert!(!refused(inside, false), "{inside}");
    }
    // Beside the root, or merely starting alike, is a place of its own.
    for beside in ["/net/h/other", "/net/h2", "/net/h2/proj"] {
        assert!(refused(beside, true), "{beside}");
        assert!(refused(beside, false), "{beside}");
    }
    // No root on a share allows nothing there; a path on the local disk is never refused here.
    assert!(refused_prefix(Path::new("/net"), true, &[]));
    assert!(!refused_prefix(Path::new("/Users/me"), false, &roots));
}
