; Hooks NSIS de l'installeur Solon (Tauri v2, bundle.windows.nsis.installerHooks).
; L'installeur est élevé (installMode perMachine) : c'est ici, et nulle part ailleurs, que Solon
; touche aux composants Windows et au Gestionnaire de services. L'application, elle, n'est jamais élevée.

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Configuration de Solon (composants Windows, service)…"
  ; setup.ps1 : active Hyper-V et la Plateforme de machine virtuelle si nécessaire, installe et
  ; démarre le service SolonService. Code de retour 3010 = redémarrage requis.
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\installer\setup.ps1" -InstallDir "$INSTDIR"'
  Pop $0
  ${If} $0 == 3010
    SetRebootFlag true
    DetailPrint "Un redémarrage de Windows est nécessaire pour terminer l'activation des composants."
  ${ElseIf} $0 != 0
    MessageBox MB_ICONEXCLAMATION|MB_OK "La configuration de Solon a rencontré une erreur (code $0).$\r$\nConsultez %ProgramData%\Solon\logs\setup.log puis relancez l'installation."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Arrêt et suppression du service Solon…"
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\installer\setup.ps1" -InstallDir "$INSTDIR" -Uninstall'
  Pop $0
  MessageBox MB_YESNO|MB_ICONQUESTION "Supprimer aussi les données des conteneurs (images, volumes) de Solon ?$\r$\nDossier : %ProgramData%\Solon" IDNO keep_data
    nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -Command "Remove-Item -Recurse -Force \"$$env:ProgramData\Solon\" -ErrorAction SilentlyContinue"'
  keep_data:
!macroend
