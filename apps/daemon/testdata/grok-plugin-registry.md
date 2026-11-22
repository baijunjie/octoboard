# Grok Build's plugin registry

`grok_installed_plugins_registry_1_0_92.json` is `installed-plugins/registry.json` as Grok Build 1.0.92 wrote it on
2026-11-22 in a scratch home, after `grok plugin install <git URL> --trust` and `grok plugin install <path> --trust` run
with `GROK_HOME` set to a directory whose `installed-plugins/` was a link to that scratch home's, as a per-session home's
is. Byte for byte, the file's own ending included (it has no final newline), but for the paths: the per-session
directory is written `{SESSION_HOME}` and the directory the two plugin sources were in `{SOURCES}`.
