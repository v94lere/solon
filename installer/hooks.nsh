; Hooks NSIS de l'installeur Solon (Tauri v2, bundle.windows.nsis.installerHooks).
; L'installeur est élevé (installMode perMachine) : c'est ici, et nulle part ailleurs, que Solon
; touche aux composants Windows et au Gestionnaire de services. L'application, elle, n'est jamais élevée.

!macro NSIS_HOOK_PREINSTALL
  ; Mise à jour par-dessus une installation existante : le service tient solon-service.exe ouvert.
  ; L'arrêter arrête proprement le moteur (les données de %ProgramData%\Solon sont conservées).
  ; Puis attente (60 s au plus) que l'hyperviseur relâche les fichiers de l'image (vmlinuz, initrd.img,
  ; rootfs.vhd restent ouverts quelques secondes après l'arrêt de la machine) : sinon l'installeur
  ; silencieux remplace le manifeste mais pas l'image, et le moteur démarre en IMAGE_CORRUPTED.
  DetailPrint "Arrêt du service Solon s'il est présent…"
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -Command "Stop-Service -Name SolonService -Force -ErrorAction SilentlyContinue; Get-Process solon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue; $$d = \"$INSTDIR\image\"; for ($$i = 0; $$i -lt 60; $$i++) { $$busy = $$false; Get-ChildItem $$d -ErrorAction SilentlyContinue | ForEach-Object { try { $$h = [IO.File]::Open($$_.FullName, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None); $$h.Close() } catch { $$busy = $$true } }; if (-not $$busy) { break }; Start-Sleep 1 }; if ($$busy) { Write-Output \"image toujours verrouillée après 60 s\" } else { Write-Output \"image libre après $$i s\" }"'
  Pop $0
!macroend

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
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Supprimer aussi les données des conteneurs (images, volumes) de Solon ?$\r$\nDossier : %ProgramData%\Solon" /SD IDNO IDNO keep_data
    nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -Command "Remove-Item -Recurse -Force \"$$env:ProgramData\Solon\" -ErrorAction SilentlyContinue"'
  keep_data:
!macroend
