# Windows Terminal licence and notices

`LICENSE` and `NOTICE.md` are the unchanged texts of the microsoft/terminal repository at tag `v1.25.2733.0`. The
Windows installer ships `conpty.dll` and `OpenConsole.exe` from the `Microsoft.Windows.Console.ConPTY` NuGet package
(see `scripts/fetch-conpty.mjs`), which carries no licence file of its own; they are built from that repository, and
`bundle.resources` in `tauri.windows.conf.json` installs these two files beside them.
