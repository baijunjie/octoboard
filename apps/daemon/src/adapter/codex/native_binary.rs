//! The native Codex program a package install ships, for a host whose launcher for it is a shim.
//!
//! The package `@openai/codex` is a Node launcher script, and the native program it starts lives
//! in a platform package: `vendor/<target>/bin/codex` (or `vendor/<target>/codex/codex`, in the
//! layout before it), under the package's own `node_modules/@openai/codex-<platform>` or beside
//! the package when the package manager hoisted it (pnpm links it beside the package's real
//! directory in its store). Which package managers' shims are recognised, and where each installs
//! the package, is the platform's (`platform::shell::shim_package`).

use std::env::consts::EXE_SUFFIX;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::platform::shell::{self, ResolvedBinary, Shim};
use crate::protocol::{Code, CodedError};
use crate::providers::codex;

const NPM_PACKAGE: &str = "@openai/codex";

/// The native program the shim `binary` launches, `None` for a binary that is no shim. A shim
/// whose install ships no native program for this machine, and a script shim whose package is in
/// no layout known, are a coded error: the launcher script is never run in the program's place.
pub(super) fn of(binary: &ResolvedBinary) -> Result<Option<ResolvedBinary>> {
    match shell::shim_package(binary, NPM_PACKAGE) {
        Shim::None => Ok(None),
        Shim::Package(package) => shipped_with(&package).map(Some),
        Shim::Unresolved => Err(CodedError::raised(
            Code::of_provider(codex::NATIVE_PROGRAM_MISSING),
            format!(
                "the Codex launcher at `{}` is a script whose package was not found",
                binary.program.display()
            ),
            &[],
        )),
    }
}

/// The native program of the package installed at `package`, or the coded error that it ships none.
fn shipped_with(package: &Path) -> Result<ResolvedBinary> {
    native_program(package)
        .map(ResolvedBinary::new)
        .ok_or_else(|| {
            CodedError::raised(
                Code::of_provider(codex::NATIVE_PROGRAM_MISSING),
                format!(
                    "the Codex install at `{}` ships no native program",
                    package.display()
                ),
                &[],
            )
        })
}

/// The native program of the package installed at `package` built for an architecture this
/// machine runs ([`shell::runnable_architectures`]), its native one first; one built for another is
/// never taken. The package is looked at where it was found and where it really is, since pnpm
/// installs the platform package beside the latter.
fn native_program(package: &Path) -> Option<PathBuf> {
    native_program_for(package, &shell::runnable_architectures())
}

/// [`native_program`] for a machine that runs programs of `architectures`, the first preferred.
fn native_program_for(package: &Path, architectures: &[&str]) -> Option<PathBuf> {
    let mut packages = vec![package.to_path_buf()];
    if let Ok(real) = crate::hostfs::canonical(package) {
        if real != package {
            packages.push(real);
        }
    }
    let mut roots = Vec::new();
    for package in &packages {
        roots.extend(package_roots(package)?);
    }
    let program = format!("codex{EXE_SUFFIX}");
    let mut found: Vec<(usize, PathBuf)> = Vec::new();
    for root in roots {
        let mut targets: Vec<PathBuf> = std::fs::read_dir(root.join("vendor"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        targets.sort();
        for target in targets {
            let Some(rank) = target
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| architectures.iter().position(|arch| name.starts_with(arch)))
            else {
                continue;
            };
            for folder in ["bin", "codex"] {
                let candidate = target.join(folder).join(&program);
                if candidate.is_file() {
                    found.push((rank, candidate));
                }
            }
        }
    }
    found
        .into_iter()
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, path)| path)
}

/// The package at `package` and the platform packages installed with it: nested in its own
/// `node_modules`, or beside it in its scope.
fn package_roots(package: &Path) -> Option<Vec<PathBuf>> {
    let scope = package.parent()?;
    let mut roots = vec![package.to_path_buf()];
    for directory in [
        package.join("node_modules").join(scope.file_name()?),
        scope.to_path_buf(),
    ] {
        let mut platform_packages: Vec<PathBuf> = std::fs::read_dir(directory)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("codex-"))
            })
            .collect();
        platform_packages.sort();
        roots.extend(platform_packages);
    }
    Some(roots)
}

#[cfg(test)]
mod tests {
    use std::env::consts::ARCH;

    use super::*;
    use crate::test_support::{code_of, ScratchDir};

    fn program_in(root: &Path, target: &str, folder: &str) -> PathBuf {
        let path = root
            .join("vendor")
            .join(target)
            .join(folder)
            .join(format!("codex{EXE_SUFFIX}"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "").unwrap();
        path
    }

    #[test]
    fn the_native_program_is_found_nested_or_hoisted_in_either_layout() {
        let dir = ScratchDir::new("codex-native-program");
        let nested = dir.join("nested/node_modules/@openai/codex");
        std::fs::create_dir_all(&nested).unwrap();
        let platform = nested.join("node_modules/@openai/codex-platform");
        let target = format!("{ARCH}-pc-windows-msvc");
        assert_eq!(native_program(&nested), None);
        let program = program_in(&platform, &target, "bin");
        assert_eq!(native_program(&nested), Some(program));

        let hoisted = dir.join("hoisted/node_modules/@openai/codex");
        std::fs::create_dir_all(&hoisted).unwrap();
        let program = program_in(
            &dir.join("hoisted/node_modules/@openai/codex-platform"),
            &target,
            "codex",
        );
        assert_eq!(native_program(&hoisted), Some(program));
    }

    /// A program the machine cannot run is never taken; of those it runs, its native
    /// architecture's is preferred over one it emulates.
    #[test]
    fn only_a_program_the_machine_runs_is_taken_its_native_one_first() {
        let dir = ScratchDir::new("codex-native-arch");
        let package = dir.join("node_modules/@openai/codex");
        std::fs::create_dir_all(&package).unwrap();
        let platform = package.join("node_modules/@openai/codex-platform");
        let arm = program_in(&platform, "aarch64-pc-windows-msvc", "bin");
        assert_eq!(native_program_for(&package, &["x86_64"]), None);
        assert_eq!(
            native_program_for(&package, &["aarch64", "x86_64"]),
            Some(arm.clone())
        );
        let x64 = program_in(&platform, "x86_64-pc-windows-msvc", "bin");
        assert_eq!(native_program_for(&package, &["x86_64"]), Some(x64.clone()));
        assert_eq!(
            native_program_for(&package, &["aarch64", "x86_64"]),
            Some(arm)
        );
        std::fs::remove_dir_all(platform.join("vendor/aarch64-pc-windows-msvc")).unwrap();
        assert_eq!(
            native_program_for(&package, &["aarch64", "x86_64"]),
            Some(x64)
        );
    }

    /// pnpm links the package into its global `node_modules` from its store, where the platform
    /// package lies beside the package's real directory.
    #[test]
    fn the_native_program_is_found_beside_a_linked_packages_real_directory() {
        let dir = ScratchDir::new("codex-native-pnpm");
        let store = dir.join("store/node_modules/@openai");
        std::fs::create_dir_all(store.join("codex")).unwrap();
        let program = program_in(
            &store.join("codex-platform"),
            &format!("{ARCH}-pc-windows-msvc"),
            "bin",
        );
        let linked = dir.join("global/5/node_modules/@openai");
        std::fs::create_dir_all(&linked).unwrap();
        crate::test_support::symlink(store.join("codex"), linked.join("codex")).unwrap();
        let found = native_program(&linked.join("codex")).expect("found through the link");
        assert_eq!(
            crate::hostfs::canonical(&found).unwrap(),
            crate::hostfs::canonical(&program).unwrap()
        );
    }

    #[test]
    fn a_package_that_ships_no_native_program_is_a_coded_error() {
        let dir = ScratchDir::new("codex-native-missing");
        let package = dir.join("node_modules/@openai/codex");
        std::fs::create_dir_all(&package).unwrap();
        let err = shipped_with(&package).expect_err("nothing is installed beside the launcher");
        assert_eq!(
            code_of(&err),
            Code::of_provider(codex::NATIVE_PROGRAM_MISSING)
        );

        let program = program_in(
            &package.join("node_modules/@openai/codex-platform"),
            &format!("{ARCH}-pc-windows-msvc"),
            "bin",
        );
        assert_eq!(shipped_with(&package).unwrap().program, program);
    }

    #[test]
    fn a_binary_that_is_no_shim_has_no_native_program_to_start_in_its_place() {
        let dir = ScratchDir::new("codex-native-no-shim");
        let program = dir.join(format!("codex{EXE_SUFFIX}"));
        std::fs::write(&program, "").unwrap();
        assert_eq!(of(&ResolvedBinary::new(program)).unwrap(), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_launch_through_a_shim_without_a_native_program_is_refused() {
        let dir = ScratchDir::new("codex-native-shim-refused");
        let env = crate::test_support::codex_shim_without_native_program(&dir);
        let err = crate::adapter::resolve_binary(crate::protocol::AgentId::CODEX, &env)
            .expect_err("the shim's package ships no native program");
        assert_eq!(
            code_of(&err),
            Code::of_provider(codex::NATIVE_PROGRAM_MISSING)
        );
    }
}
