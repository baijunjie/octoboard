use super::*;
use crate::protocol::PathLinkView;
use crate::test_support::ScratchDir;

fn canonical(dir: &Path) -> PathBuf {
    std::fs::canonicalize(dir).unwrap()
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
fn a_path_outside_every_root_is_refused_even_through_a_link() {
    let project = ScratchDir::new("path-links-outside");
    let elsewhere = ScratchDir::new("path-links-elsewhere");
    std::fs::write(elsewhere.join("secret.txt"), "x").unwrap();
    std::os::unix::fs::symlink(elsewhere.join("secret.txt"), project.join("link.txt")).unwrap();
    let roots = project_roots(&[&project]);
    let at = |raw: &str| kind(raw, None, &no_temp(), &roots);
    assert_eq!(at(elsewhere.join("secret.txt").to_str().unwrap()), None);
    assert_eq!(at("link.txt"), None);
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
fn what_could_be_launched_is_never_opened_and_only_a_project_file_is_kept_to_be_viewed() {
    let dir = ScratchDir::new("path-links-launchable");
    for name in [
        "sheet.png",
        "run.command",
        "Loud.COMMAND",
        "tool",
        "x.fileloc",
    ] {
        std::fs::write(dir.join(name), "x").unwrap();
    }
    std::fs::set_permissions(dir.join("tool"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(dir.join("Thing.app/Contents")).unwrap();
    std::fs::write(dir.join("Thing.app/Contents/info.txt"), "x").unwrap();
    let roots = project_roots(&[&dir]);
    let launchable = [
        "run.command",
        "Loud.COMMAND",
        "tool",
        "x.fileloc",
        "Thing.app/Contents/info.txt",
    ];
    for (temp, scope) in [(no_temp(), "project"), (temp_of(&dir), "temp")] {
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
            let resolved = at(name);
            if scope == "project" {
                // A file is kept for the viewer, marked so that nothing opens it.
                assert_eq!(
                    resolved.map(|r| (r.kind, r.launchable)),
                    Some((PathLinkKind::File, true)),
                    "{name}"
                );
            } else {
                assert_eq!(resolved, None, "{scope}: {name}");
            }
        }
        // A bundle is a folder to open, so it is never reachable.
        assert_eq!(at("Thing.app").map(|r| r.kind), None, "{scope}");
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
    std::os::unix::fs::symlink(dir.join("outside.md"), dir.join("index.md")).unwrap();
    assert_eq!(entry_file(&dir), None);
    assert_eq!(entry_file(&dir.join("missing")), None);
}

#[test]
fn an_absolute_path_must_start_with_a_root_as_written_not_only_resolve_into_one() {
    let project = ScratchDir::new("path-links-spelled");
    let elsewhere = ScratchDir::new("path-links-spelled-link");
    std::fs::write(project.join("a.ts"), "x").unwrap();
    std::os::unix::fs::symlink(&*project, elsewhere.join("alias")).unwrap();
    let roots = project_roots(&[&project]);
    let written = canonical(&elsewhere).join("alias/a.ts");
    assert_eq!(
        kind(written.to_str().unwrap(), None, &no_temp(), &roots),
        None
    );
    let direct = canonical(&project).join("a.ts");
    assert_eq!(
        kind(direct.to_str().unwrap(), None, &no_temp(), &roots),
        Some(PathLinkKind::File)
    );
}

#[test]
fn a_link_from_a_temp_root_to_outside_it_is_refused() {
    let temp = ScratchDir::new("path-links-temp-link");
    let elsewhere = ScratchDir::new("path-links-temp-target");
    std::fs::write(elsewhere.join("a.txt"), "x").unwrap();
    std::os::unix::fs::symlink(elsewhere.join("a.txt"), temp.join("a.txt")).unwrap();
    let path = canonical(&temp).join("a.txt");
    assert_eq!(
        kind(
            path.to_str().unwrap(),
            None,
            &temp_of(&temp),
            &ProjectRoots::default()
        ),
        None
    );
}

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
fn dots_cannot_lead_out_of_the_roots_a_path_was_checked_against() {
    let project = ScratchDir::new("path-links-dots");
    let elsewhere = ScratchDir::new("path-links-dots-target");
    std::fs::write(elsewhere.join("secret.txt"), "x").unwrap();
    let roots = project_roots(&[&project]);
    let name = elsewhere.file_name().unwrap().to_str().unwrap();
    let up = format!("../{name}/secret.txt");
    let absolute = canonical(&project).join(&up);
    assert_eq!(kind(&up, None, &no_temp(), &roots), None);
    assert_eq!(
        kind(absolute.to_str().unwrap(), None, &no_temp(), &roots),
        None
    );
    assert_eq!(
        kind(
            absolute.to_str().unwrap(),
            None,
            &temp_of(&project),
            &ProjectRoots::default()
        ),
        None
    );
}

#[test]
fn only_a_path_outside_the_temp_roots_asks_for_the_project_roots() {
    let calls = std::cell::Cell::new(0);
    let temp = ScratchDir::new("path-links-lexical");
    let temp_roots = temp_of(&temp);
    let project = || {
        calls.set(calls.get() + 1);
        ProjectRoots::default()
    };
    assert!(resolve("/etc/hosts", None, &temp_roots, &project).is_none());
    assert_eq!(
        calls.get(),
        1,
        "an absolute path outside a temp root needs the project roots"
    );
    let inside = canonical(&temp).join("a.txt");
    std::fs::write(&inside, "x").unwrap();
    assert!(resolve(inside.to_str().unwrap(), None, &temp_roots, &project).is_some());
    assert_eq!(calls.get(), 1, "a temp path never asks for them");
}

#[test]
fn dots_are_folded_lexically_before_the_file_system_sees_the_path() {
    let project = ScratchDir::new("path-links-fold");
    let elsewhere = ScratchDir::new("path-links-fold-target");
    std::fs::write(project.join("a.txt"), "x").unwrap();
    std::os::unix::fs::symlink(&*elsewhere, project.join("link")).unwrap();
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

mod with_state {
    use super::*;
    use crate::browse::tests::Fixture;
    use crate::test_support::{git, repo_with};

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
    fn a_project_whose_directory_is_gone_still_answers_for_the_temp_roots() {
        let fixture = Fixture::new();
        let missing = ScratchDir::new("path-links-gone").join("missing");
        let project = fixture.project("gone-project", &missing);
        let scratch = ScratchDir::new("path-links-gone-temp");
        // Owned and closed to others whatever the umask.
        std::fs::set_permissions(&*scratch, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(scratch.join("a.png"), "x").unwrap();
        std::fs::set_permissions(
            scratch.join("a.png"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let in_temp = canonical(&scratch).join("a.png");
        let links = probe(
            &fixture.state,
            Some(&project),
            vec!["a.ts".into(), in_temp.to_string_lossy().into_owned()],
        )
        .unwrap();
        assert_eq!(
            (links[0].kind, links[1].kind),
            (PathLinkKind::None, PathLinkKind::File)
        );
    }

    #[test]
    fn opening_what_is_not_reachable_is_refused_as_outside_the_scope() {
        let fixture = Fixture::new();
        let directory = ScratchDir::new("path-links-open");
        let project = fixture.project("open-project", &directory);
        for path in ["/etc/hosts", "nothing.ts"] {
            let error = open_with(&fixture.state, Some(&project), path, |_| {
                panic!("{path} must not be opened")
            })
            .unwrap_err();
            assert_eq!(
                error.downcast_ref::<CodedError>().unwrap().code,
                error_code::OUTSIDE_SCOPE,
                "{path}"
            );
        }
    }

    #[test]
    fn the_roots_of_a_project_include_its_place_in_a_linked_worktree() {
        let fixture = Fixture::new();
        let repository = repo_with("path-links-repo", &[(b"a.ts", b"x")]);
        let linked = ScratchDir::new("path-links-linked").join("tree");
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
        let roots = crate::browse::project_live_roots(
            &fixture.state,
            &project,
            &crate::browse::git::GitEnv::from_shell,
        )
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
        let paths = ["src/a%b.ts", "data.bin", "run.command", "src", &in_temp];
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
                None
            ]
        );
        assert!(links.iter().all(|link| link
            .view
            .as_ref()
            .is_none_or(|view| view.worktree.is_none())));
        // Only what the system could launch is for the viewer alone.
        let viewer_only: Vec<_> = links.iter().map(|link| link.viewer_only).collect();
        assert_eq!(viewer_only, [false, false, true, false, false]);
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
        let error = open_with(&fixture.state, Some(&project), "run.command", |_| {
            opened.set(true);
            Ok(())
        })
        .unwrap_err();
        assert!(!opened.get());
        assert_eq!(
            error.downcast_ref::<CodedError>().unwrap().code,
            error_code::OUTSIDE_SCOPE
        );
    }
}
