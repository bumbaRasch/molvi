; molvi NSIS uninstall hook.
; Doc-confirmed: https://v2.tauri.app/reference/config (NSIS_HOOK_PREUNINSTALL)
; + https://v2.tauri.app/distribute/windows-installer (installerHooks).
;
; WHY: Tauri's NSIS uninstaller removes only $INSTDIR (the exe files), NOT the
; appdata dir. So %APPDATA%\com.molvi.app\settings.json — which carries
; `onboarded: true` — survived uninstall, and a reinstall skipped onboarding.
; The whole point of uninstalling for the user is a clean slate; this prompts
; before wiping appdata (settings, dictionary, snippets, history, models).
;
; `/SD IDNO` = silent default No: the Tauri UPDATER applies updates by running
; the new setup.exe (installMode "passive"), which re-runs the uninstall step.
; A silent/passive run auto-answers No → an in-place update NEVER wipes user
; data. Only an interactive uninstall the user chose to run can answer Yes.

!macro NSIS_HOOK_PREUNINSTALL
  MessageBox MB_YESNO|MB_ICONQUESTION "Remove all molvi data (settings, dictionary, snippets, history, models)?" /SD IDNO IDNO skip_molvi_data
    RMDir /r "$APPDATA\com.molvi.app"
  skip_molvi_data:
!macroend
