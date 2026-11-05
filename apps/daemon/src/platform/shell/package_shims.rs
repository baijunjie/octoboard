//! Where Node package managers install a global package beside the shims they write for it on
//! Windows, as plain functions over paths: the part of `shim_package` that needs no call into the
//! OS, kept apart so that its tests run on every host.
//!
//! A shim starts the package's launcher script through Node (or Bun), and a batch-file shim takes
//! arguments through `cmd`'s quoting, so a caller that knows the package ships a program of its
//! own starts that program in the shim's place, found from the package's directory.

use std::path::{Path, PathBuf};

/// What a resolved binary is to the package it may launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shim {
    /// No shim: the program itself.
    None,
    /// A shim of the package installed at this directory.
    Package(PathBuf),
    /// A script shim (a `.cmd`, `.bat` or `.ps1`) whose package is in none of the layouts known
    /// here: never run in place of the program the package ships.
    Unresolved,
}

/// What `program` is to the package `package` (`@scope/name`): a script shim (`.cmd`, `.bat`,
/// `.ps1`) or a program shim (an `.exe` that Bun or Volta writes) with the package found in its
/// manager's layout, a script shim with the package found nowhere, or no shim. A package counts as
/// installed where its `package.json` is.
pub fn shim_of(program: &Path, package: &str) -> Shim {
    let Some(extension) = program
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
    else {
        return Shim::None;
    };
    let script = matches!(extension.as_str(), "cmd" | "bat" | "ps1");
    if !script && extension != "exe" {
        return Shim::None;
    }
    let Some(directory) = program.parent() else {
        return Shim::None;
    };
    let relative: PathBuf = package.split('/').collect();
    let mut candidates = home_layouts(directory, &relative);
    if script {
        candidates.extend(script_layouts(directory, &relative));
    }
    match candidates
        .into_iter()
        .find(|candidate| candidate.join("package.json").is_file())
    {
        Some(found) => Shim::Package(found),
        None if script => Shim::Unresolved,
        None => Shim::None,
    }
}

/// Where a package is installed beside the script shims for it in `directory`: npm's global
/// prefix, which holds both the shims and `node_modules`; a project's `node_modules\.bin`; and
/// pnpm's global directory, whose shims sit above `global\<layout version>\node_modules`.
fn script_layouts(directory: &Path, relative: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![directory.join("node_modules").join(relative)];
    if directory
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(".bin"))
    {
        if let Some(modules) = directory.parent() {
            candidates.push(modules.join(relative));
        }
    }
    let mut layouts: Vec<PathBuf> = std::fs::read_dir(directory.join("global"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    // The newest layout first, where pnpm left an older one behind.
    newest_first(&mut layouts);
    candidates.extend(
        layouts
            .into_iter()
            .map(|layout| layout.join("node_modules").join(relative)),
    );
    candidates
}

/// Orders pnpm's global layouts newest first: one named by a number by that number, so `10` is
/// newer than `6`, ahead of one named otherwise, which goes by its name.
fn newest_first(layouts: &mut [PathBuf]) {
    layouts.sort_by_cached_key(|layout| {
        let name = layout.file_name().unwrap_or_default();
        std::cmp::Reverse((
            name.to_str().and_then(|name| name.parse::<u64>().ok()),
            name.to_owned(),
        ))
    });
}

/// Where a package is installed by a manager that keeps its shims in a `bin` directory of its own
/// home: Yarn's (`Data\global\node_modules`), Bun's (`install\global\node_modules`) and Volta's,
/// which installs each package as an npm prefix of its own under `tools\image\packages`.
fn home_layouts(directory: &Path, relative: &Path) -> Vec<PathBuf> {
    let in_bin = directory
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("bin"));
    let Some(home) = directory.parent().filter(|_| in_bin) else {
        return Vec::new();
    };
    let image = home
        .join("tools")
        .join("image")
        .join("packages")
        .join(relative);
    vec![
        home.join("Data")
            .join("global")
            .join("node_modules")
            .join(relative),
        home.join("install")
            .join("global")
            .join("node_modules")
            .join(relative),
        image.join("node_modules").join(relative),
        image.join("lib").join("node_modules").join(relative),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    /// Installs `@scope/tool` at `package` (relative to `dir`) and returns its directory.
    fn installed(dir: &Path, package: &str) -> PathBuf {
        let path = dir.join(package).join("@scope").join("tool");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("package.json"), "{}").unwrap();
        path
    }

    fn of(program: PathBuf) -> Shim {
        shim_of(&program, "@scope/tool")
    }

    #[test]
    fn an_npm_shim_names_the_package_installed_beside_it() {
        let dir = ScratchDir::new("shims-npm");
        let global = dir.join("prefix");
        let package = installed(&global, "node_modules");
        let local = dir.join("project").join("node_modules");
        std::fs::create_dir_all(local.join(".bin")).unwrap();
        let local_package = installed(&dir.join("project"), "node_modules");
        assert_eq!(of(global.join("tool.cmd")), Shim::Package(package.clone()));
        assert_eq!(of(global.join("TOOL.PS1")), Shim::Package(package));
        assert_eq!(
            of(local.join(".bin").join("tool.cmd")),
            Shim::Package(local_package)
        );
        assert_eq!(of(global.join("tool.exe")), Shim::None);
        assert_eq!(of(global.join("tool")), Shim::None);
    }

    #[test]
    fn a_pnpm_shim_names_the_package_in_the_newest_global_layout() {
        let dir = ScratchDir::new("shims-pnpm");
        let pnpm = dir.join("pnpm");
        installed(&pnpm.join("global").join("5"), "node_modules");
        let newest = installed(&pnpm.join("global").join("6"), "node_modules");
        assert_eq!(of(pnpm.join("tool.cmd")), Shim::Package(newest));
        // By number, not by text, where `10` sorts before `6`.
        let newer = installed(&pnpm.join("global").join("10"), "node_modules");
        assert_eq!(of(pnpm.join("tool.cmd")), Shim::Package(newer));
    }

    #[test]
    fn pnpm_layouts_named_by_numbers_go_by_number_ahead_of_others() {
        let mut layouts: Vec<PathBuf> = ["6", "beta", "10", "alpha", "5"]
            .map(|name| Path::new("global").join(name))
            .into();
        newest_first(&mut layouts);
        let names: Vec<_> = layouts
            .iter()
            .map(|layout| layout.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, ["10", "6", "5", "beta", "alpha"]);
    }

    #[test]
    fn yarn_bun_and_volta_shims_name_the_package_in_their_homes() {
        let dir = ScratchDir::new("shims-homes");
        let yarn = dir.join("Yarn");
        let package = installed(&yarn.join("Data").join("global"), "node_modules");
        assert_eq!(
            of(yarn.join("bin").join("tool.cmd")),
            Shim::Package(package)
        );

        let bun = dir.join(".bun");
        let package = installed(&bun.join("install").join("global"), "node_modules");
        assert_eq!(of(bun.join("bin").join("tool.exe")), Shim::Package(package));

        for (volta, prefix) in [("Volta", "node_modules"), ("volta-lib", "lib/node_modules")] {
            let volta = dir.join(volta);
            let image = volta
                .join("tools")
                .join("image")
                .join("packages")
                .join("@scope")
                .join("tool");
            let package = installed(&image, prefix);
            assert_eq!(
                of(volta.join("bin").join("tool.exe")),
                Shim::Package(package.clone())
            );
            assert_eq!(
                of(volta.join("bin").join("tool.cmd")),
                Shim::Package(package)
            );
        }
    }

    #[test]
    fn a_script_shim_whose_package_is_nowhere_is_unresolved_and_a_program_no_shim() {
        let dir = ScratchDir::new("shims-unresolved");
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        assert_eq!(of(bin.join("tool.cmd")), Shim::Unresolved);
        assert_eq!(of(bin.join("tool.bat")), Shim::Unresolved);
        assert_eq!(of(bin.join("tool.exe")), Shim::None);
    }
}
