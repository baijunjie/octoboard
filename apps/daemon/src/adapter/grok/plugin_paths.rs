//! Keeping the user's plugin registry free of paths through a session's own `GROK_HOME`.
//!
//! Grok records a plugin it installs under `$GROK_HOME/installed-plugins/<name>` in
//! `installed-plugins/registry.json` (measured on 1.0.92). A session's `GROK_HOME` is its
//! per-session home, whose `installed-plugins` is a link to the source home's: the plugin's files
//! and the registry land in the user's own directory, but the registry names the plugin through
//! the per-session home, which is removed when the session ends. A plugin from a git source is
//! then no longer loaded by the user's own Grok and its update fails. So the entries a session
//! recorded that way are pointed at the path Grok would have recorded run against the source home
//! itself: after the session's process exits, and before each launch for what an earlier process
//! left that nothing saw exit.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::home::FARM_DIR;
use crate::adapter::files::LARGE_FILE_LIMIT;
use crate::hostfs::json_paths::{below, moved_below, rewrite_paths, Select};

/// The entry of a Grok home that holds installed plugins and their registry.
pub(super) const PLUGINS_DIR: &str = "installed-plugins";
/// The registry inside [`PLUGINS_DIR`].
const REGISTRY_FILE: &str = "registry.json";

/// What a session leaves to do with the user's plugin registry once its process has exited.
///
/// Kept in the terminal's holder meta for adoption (`super::adoption`), so it crosses builds: a
/// field added later needs `#[serde(default)]`.
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginPaths {
    /// The per-session home's plugin directory, by the spelling Grok recorded paths under.
    session_plugins: PathBuf,
    /// The source home's plugin directory, by the spelling Grok records when run against it.
    user_plugins: PathBuf,
}

/// `Some` when the per-session home `grok_home` reaches the source home's plugin directory
/// through a link, so what a session installs is the user's and its registry is the user's own.
/// A per-session home with a plugin directory of its own (the source home has none) keeps its
/// installs and its registry to itself, and both go with it.
pub(super) fn plugin_paths(source_home: &Path, grok_home: &Path) -> Option<PluginPaths> {
    let session_plugins = grok_home.join(PLUGINS_DIR);
    let user_plugins = source_home.join(PLUGINS_DIR);
    let is_link = crate::platform::fs::read_alias(&session_plugins)
        .ok()
        .flatten()
        .is_some();
    let same = crate::hostfs::canonical(&session_plugins).ok()?
        == crate::hostfs::canonical(&user_plugins).ok()?;
    (is_link && same).then_some(PluginPaths {
        session_plugins,
        user_plugins,
    })
}

/// Where the registry keeps the path of each installed plugin: `repos.<name>.path` (1.0.92).
const RECORDED_PATHS: Select = Select::At(&["repos", "*", "path"]);

impl PluginPaths {
    /// After the session's process has exited: the entries it recorded under its own home are
    /// pointed at the source home's, by the spelling Grok records when run against it. A failure
    /// is logged and leaves the registry as it was.
    pub fn publish(&self) {
        let own = |path: &Path| {
            if !path.starts_with(&self.session_plugins) {
                return None;
            }
            // The home is what differs between the two: replacing it keeps what Grok wrote after
            // it, its separators included, as Grok writes it whichever home it runs against.
            moved_below(
                path,
                self.session_plugins.parent()?,
                self.user_plugins.parent()?,
            )
        };
        self.rewrite(&own, "after the session");
    }

    /// Before the session's Grok starts: what an earlier process recorded under this same
    /// per-session home, or under the home of another session that is gone or now leads to
    /// another plugin directory, is pointed at the directory the plugins are in. An entry gets
    /// into this registry only through a home whose link led here, so that is where its files
    /// are, whichever account that session ran as: the directory is named by its resolved path,
    /// not by this session's spelling of it. A per-session home that still leads here is a
    /// running session's, whose own exit takes care of it.
    pub(super) fn settle_earlier(&self) {
        let Ok(plugins) = crate::hostfs::canonical(&self.user_plugins) else {
            return;
        };
        let scratch_root = self
            .session_plugins
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        let earlier = |path: &Path| {
            if let Ok(rest) = path.strip_prefix(&self.session_plugins) {
                return Some(below(&plugins, rest));
            }
            let (farm_plugins, rest) = split_at_farm(path, scratch_root.as_deref()?)?;
            match crate::hostfs::canonical(&farm_plugins) {
                Ok(leads) if leads == plugins => None,
                Ok(_) => Some(below(&plugins, rest)),
                Err(err) if err.kind() == io::ErrorKind::NotFound => Some(below(&plugins, rest)),
                // It cannot be looked up, so it may be a running session's.
                Err(_) => None,
            }
        };
        self.rewrite(&earlier, "before the launch");
    }

    /// Rewrites the `path` of every entry of the user's registry that `moved` gives another path
    /// for, and nothing else of it; a registry with no such entry is not written, and one that
    /// does not read as JSON is Grok's to mend and is left alone.
    fn rewrite(&self, moved: &dyn Fn(&Path) -> Option<PathBuf>, when: &str) {
        let registry = self.user_plugins.join(REGISTRY_FILE);
        let unread = |_: &str| false;
        if let Err(err) = rewrite_paths(&registry, LARGE_FILE_LIMIT, RECORDED_PATHS, moved, &unread)
        {
            tracing::warn!(
                path = %registry.display(),
                %err,
                when,
                "pointing Grok's plugin registry at the user's own plugin directory failed"
            );
        }
    }
}

impl crate::session::term::AfterExit for PluginPaths {
    fn run(&self) {
        self.publish();
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// `path` split into the plugin directory of a per-session home directly under a session's
/// scratch directory in `scratch_root`, and the rest below it; `None` for any other path.
fn split_at_farm<'a>(path: &'a Path, scratch_root: &Path) -> Option<(PathBuf, &'a Path)> {
    let below = path.strip_prefix(scratch_root).ok()?;
    let mut parts = below.components();
    let session = parts.next()?;
    if parts.next()?.as_os_str() != FARM_DIR || parts.next()?.as_os_str() != PLUGINS_DIR {
        return None;
    }
    let farm_plugins = scratch_root.join(session).join(FARM_DIR).join(PLUGINS_DIR);
    Some((farm_plugins, parts.as_path()))
}

/// For a test outside this provider: the work a session whose per-session home is `grok_home`
/// leaves for after its exit over the plugin registry of `source_home`.
#[cfg(test)]
pub(crate) fn after_exit_for_test(
    source_home: &Path,
    grok_home: &Path,
) -> Option<Box<dyn crate::session::term::AfterExit>> {
    plugin_paths(source_home, grok_home)
        .map(|paths| Box::new(paths) as Box<dyn crate::session::term::AfterExit>)
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::test_support::ScratchDir;

    /// A registry Grok Build 1.0.92 wrote after a git install and a local one under a per-session
    /// home (`testdata/grok-plugin-registry.md`), its paths as placeholders.
    const CAPTURED: &str =
        include_str!("../../../testdata/grok_installed_plugins_registry_1_0_92.json");

    /// `path` as it stands inside a JSON string.
    fn in_json(path: &Path) -> String {
        let quoted = serde_json::to_string(&path.to_string_lossy()).unwrap();
        quoted[1..quoted.len() - 1].to_string()
    }

    /// The captured registry with its two plugins recorded under `home`'s plugin directory.
    fn captured(home: &Path) -> String {
        CAPTURED
            .replace("{SESSION_HOME}", &in_json(home))
            .replace("{SOURCES}", "/sources")
    }

    /// A registry in the captured one's shape with one plugin recorded under each of `paths`.
    fn registry(paths: &[&Path]) -> String {
        let captured: Value = serde_json::from_str(&captured(Path::new("/unused"))).unwrap();
        let entry = captured["repos"]["plugin-git-b5401dd4"].clone();
        let mut repos = serde_json::Map::new();
        for (index, path) in paths.iter().enumerate() {
            let mut entry = entry.clone();
            entry["path"] = Value::String(path.to_string_lossy().into_owned());
            repos.insert(format!("repo-{index}"), entry);
        }
        serde_json::to_string_pretty(&serde_json::json!({"version": 1, "repos": repos})).unwrap()
    }

    /// A source home with a plugin directory, and the per-session home of session `id` under
    /// the scratch root `run`, its plugin directory a link to the source's.
    fn homes(dir: &ScratchDir, id: &str) -> (PathBuf, PathBuf) {
        let source = dir.join("user-home");
        std::fs::create_dir_all(source.join(PLUGINS_DIR)).unwrap();
        let farm = farm_leading_to(dir, id, &source.join(PLUGINS_DIR));
        (source, farm)
    }

    /// The per-session home of session `id`, its plugin directory a link to `plugins`.
    fn farm_leading_to(dir: &ScratchDir, id: &str, plugins: &Path) -> PathBuf {
        let farm = dir.join("run").join(id).join(FARM_DIR);
        std::fs::create_dir_all(&farm).unwrap();
        let target = crate::hostfs::canonical(plugins).unwrap();
        crate::hostfs::alias(&target, &farm.join(PLUGINS_DIR), true).unwrap();
        farm
    }

    fn recorded(source: &Path) -> Vec<PathBuf> {
        let registry: Value = serde_json::from_str(
            &std::fs::read_to_string(source.join(PLUGINS_DIR).join(REGISTRY_FILE)).unwrap(),
        )
        .unwrap();
        registry["repos"]
            .as_object()
            .unwrap()
            .values()
            .map(|repo| PathBuf::from(repo["path"].as_str().unwrap()))
            .collect()
    }

    /// After the exit, the registry Grok wrote under the per-session home is, byte for byte,
    /// the one it would have written under the source home.
    #[test]
    fn the_sessions_own_entries_are_pointed_at_the_source_home_after_its_exit() {
        let dir = ScratchDir::new("grok-plugin-paths-exit");
        let (source, farm) = homes(&dir, "s");
        let file = source.join(PLUGINS_DIR).join(REGISTRY_FILE);
        std::fs::write(&file, captured(&farm)).unwrap();

        let paths = plugin_paths(&source, &farm).expect("the farm links the plugin directory");
        paths.publish();

        assert_eq!(std::fs::read_to_string(&file).unwrap(), captured(&source));
    }

    /// An entry that names the per-session home's plugin directory itself is rewritten to the
    /// source home's as it is spelled, with no separator at its end.
    #[test]
    fn an_entry_naming_the_plugin_directory_itself_gets_no_separator_at_its_end() {
        let dir = ScratchDir::new("grok-plugin-paths-directory-itself");
        let (source, farm) = homes(&dir, "s");
        let file = source.join(PLUGINS_DIR).join(REGISTRY_FILE);
        std::fs::write(&file, registry(&[&farm.join(PLUGINS_DIR)])).unwrap();

        plugin_paths(&source, &farm).unwrap().publish();

        let registry: Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(
            registry["repos"]["repo-0"]["path"].as_str().unwrap(),
            source.join(PLUGINS_DIR).to_string_lossy()
        );
    }

    /// A registry with nothing of the session's in it is not written: it keeps its bytes and its
    /// identity, a registry that is not JSON included, and a missing one is not created.
    #[test]
    fn a_registry_with_nothing_to_rewrite_is_left_byte_for_byte() {
        let dir = ScratchDir::new("grok-plugin-paths-untouched");
        let (source, farm) = homes(&dir, "s");
        let paths = plugin_paths(&source, &farm).unwrap();
        let file = source.join(PLUGINS_DIR).join(REGISTRY_FILE);
        paths.publish();
        paths.settle_earlier();
        assert!(!file.exists());

        let users_own = crate::hostfs::canonical(source.join(PLUGINS_DIR)).unwrap();
        let unreadable = format!("not json {}", in_json(&farm.join(PLUGINS_DIR)));
        for content in [
            captured(users_own.parent().unwrap()),
            captured(&source),
            unreadable,
            String::new(),
        ] {
            std::fs::write(&file, &content).unwrap();
            let identity = |path: &Path| {
                crate::platform::fs::identity(path, crate::platform::fs::Links::Keep)
                    .expect("identity")
            };
            let before = identity(&file);
            paths.publish();
            paths.settle_earlier();
            assert_eq!(std::fs::read_to_string(&file).unwrap(), content);
            assert!(identity(&file) == before, "the registry was replaced");
        }
    }

    /// Before a launch, what an earlier process of this session left, what a session whose home
    /// is gone left, and what a session whose home now leads to another directory left, are
    /// rewritten; a session whose home still leads here keeps its entries, and so does a path
    /// that only looks like a per-session home somewhere else.
    #[test]
    fn entries_of_homes_that_are_gone_are_settled_before_a_launch() {
        let dir = ScratchDir::new("grok-plugin-paths-earlier");
        let (source, farm) = homes(&dir, "s");
        let (_, running) = homes(&dir, "running");
        let elsewhere = dir.join("another-home").join(PLUGINS_DIR);
        std::fs::create_dir_all(&elsewhere).unwrap();
        let moved_on = farm_leading_to(&dir, "moved-on", &elsewhere);
        let run = dir.join("run");
        let own = farm.join(PLUGINS_DIR).join("own-1");
        let gone = run
            .join("gone")
            .join(FARM_DIR)
            .join(PLUGINS_DIR)
            .join("gone-2");
        let live = running.join(PLUGINS_DIR).join("live-3");
        let foreign = dir
            .join("other-run")
            .join("x")
            .join(FARM_DIR)
            .join(PLUGINS_DIR)
            .join("foreign-4");
        let not_plugins = run.join("gone").join(FARM_DIR).join("skills").join("s-5");
        let recreated = moved_on.join(PLUGINS_DIR).join("recreated-6");
        std::fs::write(
            source.join(PLUGINS_DIR).join(REGISTRY_FILE),
            registry(&[&own, &gone, &live, &foreign, &not_plugins, &recreated]),
        )
        .unwrap();

        plugin_paths(&source, &farm).unwrap().settle_earlier();

        let plugins = crate::hostfs::canonical(source.join(PLUGINS_DIR)).unwrap();
        assert_eq!(
            recorded(&source),
            [
                plugins.join("own-1"),
                plugins.join("gone-2"),
                live,
                foreign,
                not_plugins,
                plugins.join("recreated-6"),
            ]
        );
    }

    /// A per-session home with a plugin directory of its own, which the source home lacks, has
    /// nothing of the user's to rewrite; neither has one whose link leads elsewhere.
    #[test]
    fn a_home_with_a_plugin_directory_of_its_own_leaves_nothing_to_do() {
        let dir = ScratchDir::new("grok-plugin-paths-own-dir");
        let source = dir.join("user-home");
        std::fs::create_dir_all(&source).unwrap();
        let farm = dir.join("run").join("s").join(FARM_DIR);
        std::fs::create_dir_all(farm.join(PLUGINS_DIR)).unwrap();
        assert!(plugin_paths(&source, &farm).is_none());

        std::fs::create_dir_all(source.join(PLUGINS_DIR)).unwrap();
        assert!(plugin_paths(&source, &farm).is_none());

        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::remove_dir(farm.join(PLUGINS_DIR)).unwrap();
        crate::hostfs::alias(
            &crate::hostfs::canonical(&elsewhere).unwrap(),
            &farm.join(PLUGINS_DIR),
            true,
        )
        .unwrap();
        assert!(plugin_paths(&source, &farm).is_none());
    }

    /// For an account that reaches the default account's plugin directory through a link: its
    /// own session's entries take the account's spelling at exit, as Grok run against the
    /// account's home records them, the registry written where it leads; a dead session's
    /// entries, settled before a launch, take the path of the directory the plugins are in,
    /// whichever account that session ran as.
    #[test]
    fn a_registry_reached_through_a_link_is_written_where_it_leads() {
        let dir = ScratchDir::new("grok-plugin-paths-linked");
        let default = dir.join("default-home");
        std::fs::create_dir_all(default.join(PLUGINS_DIR)).unwrap();
        let source = dir.join("account-home");
        std::fs::create_dir_all(&source).unwrap();
        let shared = crate::hostfs::canonical(default.join(PLUGINS_DIR)).unwrap();
        crate::hostfs::alias(&shared, &source.join(PLUGINS_DIR), true).unwrap();
        let farm = farm_leading_to(&dir, "s", &shared);
        let installed = farm.join(PLUGINS_DIR).join("tool");
        let dead = dir
            .join("run")
            .join("dead")
            .join(FARM_DIR)
            .join(PLUGINS_DIR)
            .join("left");
        std::fs::write(shared.join(REGISTRY_FILE), registry(&[&dead, &installed])).unwrap();

        let paths = plugin_paths(&source, &farm).unwrap();
        paths.publish();
        assert_eq!(
            recorded(&default),
            [dead, source.join(PLUGINS_DIR).join("tool")]
        );
        paths.settle_earlier();
        assert_eq!(
            recorded(&default),
            [shared.join("left"), source.join(PLUGINS_DIR).join("tool")]
        );
        assert!(crate::platform::fs::read_alias(&source.join(PLUGINS_DIR))
            .unwrap()
            .is_some());
    }

    /// A launch from a source home with a plugin directory settles what an earlier process left
    /// under this session's home before Grok starts, and leaves the correction of its own
    /// installs for after the exit; a source home without the directory leaves nothing.
    #[test]
    fn a_launch_settles_earlier_entries_and_leaves_its_own_for_after_the_exit() {
        use crate::adapter::grok::tests::{plan_isolated, with_default_home};
        use crate::adapter::tests::spec_fixture;

        let mut fixture = spec_fixture();
        let home = with_default_home(&mut fixture);
        std::fs::create_dir_all(home.join(PLUGINS_DIR)).unwrap();
        let farm_plugins = fixture.scratch.join(FARM_DIR).join(PLUGINS_DIR);
        let file = home.join(PLUGINS_DIR).join(REGISTRY_FILE);
        std::fs::write(&file, registry(&[&farm_plugins.join("earlier")])).unwrap();

        let plan = plan_isolated(&fixture.spec()).expect("plan");
        let settled = crate::hostfs::canonical(home.join(PLUGINS_DIR))
            .unwrap()
            .join("earlier");
        assert_eq!(recorded(&home), std::slice::from_ref(&settled));

        // Installed by the session, through its home's link.
        std::fs::write(&file, registry(&[&settled, &farm_plugins.join("new")])).unwrap();
        plan.after_exit.as_ref().expect("work after the exit").run();
        assert_eq!(
            recorded(&home),
            [settled, home.join(PLUGINS_DIR).join("new")]
        );

        let mut bare = spec_fixture();
        with_default_home(&mut bare);
        assert!(plan_isolated(&bare.spec())
            .expect("plan")
            .after_exit
            .is_none());
    }

    /// The correction is rebuilt from the launch's adoption record, beside the write-back, and a
    /// record of a build that kept none still reads.
    #[test]
    fn the_correction_is_rebuilt_from_the_adoption_record() {
        use crate::adapter::grok::adoption;
        use crate::adapter::grok::trust::CarriedTrust;

        let dir = ScratchDir::new("grok-plugin-paths-adopted");
        let (source, farm) = homes(&dir, "s");
        let installed = farm.join(PLUGINS_DIR).join("tool");
        std::fs::write(
            source.join(PLUGINS_DIR).join(REGISTRY_FILE),
            registry(&[&installed]),
        )
        .unwrap();
        let trust = CarriedTrust {
            session_store: "/session/trusted_folders.toml".into(),
            user_store: "/user/trusted_folders.toml".into(),
            folder: "/work".to_string(),
            record_wait: std::time::Duration::from_secs(5),
        };
        let paths = plugin_paths(&source, &farm).unwrap();
        let mut record = adoption::record(&trust, None, Some(&paths)).expect("a record");

        let rebuilt = adoption::rebuild(&record, Path::new("/octoboard/consoles"));
        rebuilt.after_exit.expect("work after the exit").run();
        assert_eq!(recorded(&source), [source.join(PLUGINS_DIR).join("tool")]);

        record.as_object_mut().unwrap().remove("plugin_paths");
        let older = adoption::rebuild(&record, Path::new("/octoboard/consoles"));
        assert!(older.carried_trust.is_some() && older.after_exit.is_none());
    }
}
