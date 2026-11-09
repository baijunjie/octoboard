> Severity: Major

# Windows update install stalls when a file of the installation is still in use

## Symptom

On Windows, the update installer started on the way out of Octoboard stops on an Abort/Retry/Ignore box for every
installation file a running process still holds open, and waits there instead of finishing.

## Reproduction steps

1. Install Octoboard on Windows from its NSIS installer.
2. From the installation folder, start processes that keep its files open: `octoboardd.exe mcp …` (the MCP bridge
   form of the daemon binary), and a program that creates a pseudoconsole through the bundled `conpty.dll`, which
   starts the bundled `arm64\OpenConsole.exe` (or the folder of the machine's own architecture) as its host.
3. Run the same version's installer over the installation in the mode the updater uses: `<installer>.exe /P /UPDATE`.
4. Watch the installer.

## Expected vs. actual

- Expected: the installer replaces every file it ships and finishes on its own, with no prompt.
  `docs/product/in-app-updates.md` ("Downloading and installing") states that a quit stops the daemon, then runs the
  installer in its passive mode, which shows its progress, and the update is installed on the way out.
- Actual: for each file in use (`octoboardd.exe`, `conpty.dll`, `arm64\OpenConsole.exe`) the installer shows an
  Abort/Retry/Ignore box and waits. It ends no process. Choosing Ignore leaves the installation with a mix of old and
  new files, and the installer still exits with status 0.

## Environment

- Octoboard 0.1.0-beta.3, Windows installer.
- Windows 11 ARM64 virtual machine running the x64 build under emulation, over SSH, with no desktop logon.
- The "update" was the same version installed over itself, since only that version has a Windows installer. Files
  were told apart by marker bytes appended to the installed files.
- The processes holding the files were started by hand, not by Octoboard's daemon.

## Scope of impact

- Users on Windows whose quit installs an update while any process still runs from the installation folder.
- Workaround: end those processes before quitting, or retry once they have ended.
- Whether such a process can be left running in normal use is unknown (see Leads).

## Leads

- Verified: the installer neither ends processes holding its files nor skips them; it prompts per file.
- Verified: when the same processes run from a copy of the files outside the installation folder, the installer
  shows no box, replaces every file and exits 0, and the processes keep working.
- Inferred, not investigated: an MCP bridge is started from the daemon binary's own path in the installation folder,
  so a bridge whose agent outlives the daemon's stop would keep `octoboardd.exe` open while the installer runs.
  Whether any process outlives the daemon's stop before the installer starts today is unknown.

## Acceptance criteria

- [ ] A quit that installs a downloaded update finishes the install with no prompt, even when a process started from
      the installation folder is still running at that moment.
- [ ] After that install, every file the installer ships is the new version's.
- [ ] No installation is left with a mix of old and new files.
