; Keep the CLI beside parley.exe; Tauri bundles both Cargo binary targets.
; Use explicit shortcuts rather than changing the user's PATH.
!macro NSIS_HOOK_POSTINSTALL
  ${If} $NoShortcutMode <> 1
  ${AndIf} $UpdateMode <> 1
    !insertmacro MUI_STARTMENU_WRITE_BEGIN Application
      CreateDirectory "$SMPROGRAMS\$AppStartMenuFolder"
      CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\Parley Terminal.lnk" "$SYSDIR\cmd.exe" '/d /k ""$INSTDIR\parley-cli.exe""' "$INSTDIR\parley.exe" 0
      CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\Parley Tutor.lnk" "$INSTDIR\parley.exe" "--tutor-only"
    !insertmacro MUI_STARTMENU_WRITE_END
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro MUI_STARTMENU_GETFOLDER Application $AppStartMenuFolder
    Delete "$SMPROGRAMS\$AppStartMenuFolder\Parley Terminal.lnk"
    Delete "$SMPROGRAMS\$AppStartMenuFolder\Parley Tutor.lnk"
  ${EndIf}
!macroend
