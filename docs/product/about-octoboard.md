# About Octoboard

Settings' **About** section (see "About" in `docs/product/settings.md`) shows the application itself, under its own
heading in the section: its icon, its name and, in the desktop application, the version running ("Version …"). Then,
one row each:

- **Updates** and **Update channel**, in the desktop application only: where the application's updates stand, with a
  **What is new** link to the release notes of the version it names and the button that moves the update on, and the
  choice between the Stable and Beta channels (see "The Updates and Update channel rows" in
  `docs/product/in-app-updates.md`).
- **Website**, **GitHub repository** and **Issue tracker**: external links, each shown as its address without the
  `https://` — `octoboard.dev`, `github.com/baijunjie/octoboard` and
  `github.com/baijunjie/octoboard/issues/new`.
- **License**: a line saying Octoboard is open-source software under the MIT license, with **View license**, a link to
  the license text in the repository.
- **Third-party licenses**, with **View licenses**, which opens the dialog described in "The third-party licenses
  dialog" below.
- **Copyright**: the project's shared copyright year and author name, with the name unchanged across UI languages.

In a browser the Updates and Update channel rows and the version are absent.

## The third-party licenses dialog

A dialog listing the open-source packages the application ships, under an introduction saying each is under the
license shown beside it. The list is generated when the application is built, so it is always the one of the build
running. It holds:

- the npm packages the window's code depends on at run time, and what each of those depends on in turn — not the
  packages used only to develop or test it, nor packages that hold only type declarations (`@types/*`);
- the Rust crates the desktop shell and the daemon are built from, for the two Mac architectures — not the crates that
  run only inside the compiler (procedural macros), nor what only those need.

Octoboard's own packages are not listed. Packages are in order of name; each row is a disclosure showing its name and
version, its license — the license expression the package declares, or "See license file" where it declares none — and
whether it is an npm package or a Rust crate. Opening a row shows its license text, or each of its texts one under the
other, keeping their line breaks; a package that ships no license text says so instead.

A search field above the list narrows it to the packages whose name or license contains what is typed, whatever its
case, and a count under the field says how many packages are listed. While the search holds something, the count is
announced as it changes, or that no package matches; with nothing matching, the list gives way to an empty state saying
no package matches.

The list is loaded when the dialog opens, which says it is loading meanwhile. Where it cannot be read, the dialog shows
an empty-state failure (see "Failures shown on the screen" in `docs/product/error-alerts.md`) saying the licenses could
not be loaded, with the loader's own text under it and **Try again**. A build made without the list shows,
with no search field and no count, an empty state saying that this build has no license list because it was made
without it.
