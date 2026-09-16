; Keep the CLI beside parley.exe; Tauri bundles both Cargo binary targets.
; Use explicit shortcuts rather than changing the user's PATH.
!macro NSIS_HOOK_POSTINSTALL
  CreateDirectory "$SMPROGRAMS\Parley"
  CreateShortcut "$SMPROGRAMS\Parley\Parley Terminal.lnk" "$SYSDIR\cmd.exe" '/d /k ""$INSTDIR\parley-cli.exe""' "$INSTDIR\parley.exe" 0
  CreateShortcut "$SMPROGRAMS\Parley\Parley Tutor.lnk" "$INSTDIR\parley.exe" "--tutor-only"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  Delete "$SMPROGRAMS\Parley\Parley Terminal.lnk"
  Delete "$SMPROGRAMS\Parley\Parley Tutor.lnk"
!macroend
