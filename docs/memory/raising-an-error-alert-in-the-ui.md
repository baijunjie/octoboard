# Raising an error alert in the UI

## Reproduce a daemon failure with a real daemon on a throwaway `HOME` before keying a remedy or fixture to it

The routes, titles and remedies are in `docs/conventions/error-codes.md`, "Raising a failure in the UI"; the rule to
settle a daemon failure against a real daemon first is in `docs/conventions/tests.md`, "What a test asserts". How:

- Run `cargo run -p octoboardd` with `HOME` set to a throwaway directory: it keeps its data and its port file under
  `$HOME/.octoboard`. Make the data folder read-only for denied access, or place it on a small volume for a full disk
  (on macOS, a disk image made and mounted with `hdiutil`).
- Expect the daemon's own words, not the system's: only a failure built from the operating system's error carries
  `(os error N)`, while SQLite reports a denied access or a full disk as "attempt to write a readonly database" or
  "database or disk is full", with no signature to scan. A code's name does not say which route it comes on either
  (`store_io_failed` is only a `reason_code` nested in trust notices, never a request's error).
- Design the remedy from the code, params and `detail` it sends, and give the fixtures exactly those.
