; Hooks of the Windows NSIS installer (`bundle.windows.nsis.installerHooks` in `tauri.windows.conf.json`).
;
; A file of the installation that a running process holds open cannot be written, and NSIS's `File` then stops
; on an Abort/Retry/Ignore box for each one; Ignore leaves the installation a mix of old and new files. Windows
; does let such a file be renamed, and the process goes on running from it under its new name. So before any
; file is written, each shipped file a process holds is moved aside to `<file>.old-<n>`, and the new file takes
; its place. What was moved aside is deleted once nothing holds it: right after the files are written, at the
; next install and at the uninstall. The daemon, when it next starts, also deletes what was moved aside of itself,
; `conpty.dll` and the `OpenConsole.exe` files, but not of the license files. An install that fails, or one told
; to Ignore a file it could not write, puts back what it moved aside where no new file took its place, so it
; leaves no shipped file missing.
;
; The application must not run on past its update, so it is ended before anything is moved aside, by the same
; check Tauri runs right after this hook; its main binary is never moved aside.

!include LogicLib.nsh

; What a file moved aside is named after: `<file>.old-<n>`, `n` the first number not taken.
!define OCTOBOARD_SET_ASIDE ".old-"

; Where this run moved each shipped file aside, by the file's number in OCTOBOARD_FOR_EACH_SHIPPED_FILE; empty
; for one it left in place.
Var OctoboardSetAside0
Var OctoboardSetAside1
Var OctoboardSetAside2
Var OctoboardSetAside3
Var OctoboardSetAside4
Var OctoboardSetAside5

; Calls the macro `action` with the path and the number of each file the installer ships but the main binary
; and the uninstaller: the daemon (`externalBin`) and the resources of `tauri.windows.conf.json`.
!macro OCTOBOARD_FOR_EACH_SHIPPED_FILE action
  !insertmacro ${action} "$INSTDIR\octoboardd.exe" 0
  !insertmacro ${action} "$INSTDIR\conpty.dll" 1
  !insertmacro ${action} "$INSTDIR\x64\OpenConsole.exe" 2
  !insertmacro ${action} "$INSTDIR\arm64\OpenConsole.exe" 3
  !insertmacro ${action} "$INSTDIR\licenses\windows-terminal\LICENSE" 4
  !insertmacro ${action} "$INSTDIR\licenses\windows-terminal\NOTICE.md" 5
!macroend

; Replaces the path on the stack with the name it moved the file aside to, or with an empty string when it left
; the file in place. A file that cannot be opened for writing is how one a process holds shows (a running
; executable, a loaded DLL); one that cannot be renamed either is left for `File` to report as before.
Function OctoboardSetAsideIfInUse
  Exch $0
  Push $1
  Push $2
  Push $3
  StrCpy $3 ""
  ${If} ${FileExists} "$0"
    ClearErrors
    FileOpen $1 "$0" a
    ${If} ${Errors}
      StrCpy $2 1
      ${DoWhile} ${FileExists} "$0${OCTOBOARD_SET_ASIDE}$2"
        IntOp $2 $2 + 1
      ${Loop}
      ClearErrors
      Rename "$0" "$0${OCTOBOARD_SET_ASIDE}$2"
      ${IfNot} ${Errors}
        StrCpy $3 "$0${OCTOBOARD_SET_ASIDE}$2"
      ${EndIf}
    ${Else}
      FileClose $1
    ${EndIf}
  ${EndIf}
  ClearErrors
  StrCpy $0 $3
  Pop $3
  Pop $2
  Pop $1
  Exch $0
FunctionEnd

!macro OCTOBOARD_SET_ASIDE_IF_IN_USE path index
  Push "${path}"
  Call OctoboardSetAsideIfInUse
  Pop $OctoboardSetAside${index}
!macroend

; Puts back what this run moved aside from `path`, unless a new file is already there.
!macro OCTOBOARD_PUT_BACK path index
  ${If} $OctoboardSetAside${index} != ""
  ${AndIfNot} ${FileExists} "${path}"
    Rename "$OctoboardSetAside${index}" "${path}"
  ${EndIf}
!macroend

; Deletes what was moved aside from `path` and is no longer held.
!macro OCTOBOARD_DELETE_SET_ASIDE path index
  Delete "${path}${OCTOBOARD_SET_ASIDE}*"
!macroend

; The same, and what is still held at the next reboot where Windows lets the uninstaller arrange that, since the
; folder cannot be removed while a file is left in it.
!macro OCTOBOARD_DELETE_SET_ASIDE_AT_REBOOT path index
  Delete /REBOOTOK "${path}${OCTOBOARD_SET_ASIDE}*"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro CheckIfAppIsRunning "$INSTDIR\${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
  !insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_SET_ASIDE_IF_IN_USE
!macroend

; First puts back a file moved aside whose new one was not written (Ignore chosen on its box), so that it is not
; missing. A reboot is never asked for: what is still held is left for the daemon's next start (the executables
; and `conpty.dll`) or for the next install or the uninstall (the license files).
!macro NSIS_HOOK_POSTINSTALL
  !insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_PUT_BACK
  !insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_DELETE_SET_ASIDE
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_DELETE_SET_ASIDE_AT_REBOOT
!macroend

; Called when the install fails: an `Abort` (Tauri's check of the main binary when the application cannot be
; ended, or Abort chosen on a file that cannot be written) or a file that cannot be extracted.
Function .onInstFailed
  !insertmacro OCTOBOARD_FOR_EACH_SHIPPED_FILE OCTOBOARD_PUT_BACK
FunctionEnd
