; Hooks NSIS de l'installeur Solon (Tauri v2, bundle.windows.nsis.installerHooks).
; L'installeur est élevé (installMode perMachine) : c'est ici, et nulle part ailleurs, que Solon
; touche aux composants Windows et au Gestionnaire de services. L'application, elle, n'est jamais élevée.

!macro NSIS_HOOK_PREINSTALL
  ; Mise à jour par-dessus une installation existante : le service tient solon-service.exe ouvert.
  ; L'arrêter arrête proprement le moteur (les données de %ProgramData%\Solon sont conservées).
  ; Puis attente (60 s au plus) que l'hyperviseur relâche les fichiers de l'image (vmlinuz, initrd.img,
  ; rootfs.vhd restent ouverts quelques secondes après l'arrêt de la machine) : sinon l'installeur
  ; silencieux remplace le manifeste mais pas l'image, et le moteur démarre en IMAGE_CORRUPTED.
  ; Installation précédente sous l'ancien nom du projet (Monodon). D'abord mettre les données à l'abri :
  ; %ProgramData%\Monodon est déplacé vers %ProgramData%\Solon (fusion, robocopy /MOVE) AVANT de lancer
  ; l'ancien désinstalleur, dont la question « supprimer les données ? » ne peut plus rien détruire.
  ; Le service Monodon est arrêté au préalable pour libérer le disque de données.
  IfFileExists "$PROGRAMFILES64\Monodon\uninstall.exe" 0 no_legacy
    DetailPrint "Moving Monodon data to Solon…"
    nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -Command "Stop-Service -Name MonodonService -Force -ErrorAction SilentlyContinue; Get-Process monodon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue; $$src = Join-Path $$env:ProgramData Monodon; $$dst = Join-Path $$env:ProgramData Solon; if (Test-Path $$src) { for ($$i = 0; $$i -lt 60; $$i++) { try { $$h = [IO.File]::Open((Join-Path $$src data.vhdx), [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None); $$h.Close(); break } catch { Start-Sleep 1 } }; robocopy $$src $$dst /E /MOVE /NFL /NDL /NJH /NJS /R:3 /W:2 | Out-Null; Write-Output \"data moved, robocopy code $$LASTEXITCODE\" }"'
    Pop $0
    DetailPrint "Removing the previous Monodon installation…"
    ExecWait '"$PROGRAMFILES64\Monodon\uninstall.exe" /S _?=$PROGRAMFILES64\Monodon'
    Delete "$PROGRAMFILES64\Monodon\uninstall.exe"
    RMDir /r "$PROGRAMFILES64\Monodon"
  no_legacy:
  DetailPrint "Stopping the Solon service if present…"
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -Command "Stop-Service -Name SolonService -Force -ErrorAction SilentlyContinue; Get-Process solon -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue; $$d = \"$INSTDIR\image\"; for ($$i = 0; $$i -lt 60; $$i++) { $$busy = $$false; Get-ChildItem $$d -ErrorAction SilentlyContinue | ForEach-Object { try { $$h = [IO.File]::Open($$_.FullName, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None); $$h.Close() } catch { $$busy = $$true } }; if (-not $$busy) { break }; Start-Sleep 1 }; if ($$busy) { Write-Output \"image still locked after 60 s\" } else { Write-Output \"image released after $$i s\" }"'
  Pop $0
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Configuring Solon (Windows features, service)…"
  ; setup.ps1 : active Hyper-V et la Plateforme de machine virtuelle si nécessaire, installe et
  ; démarre le service SolonService. Code de retour 3010 = redémarrage requis.
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\installer\setup.ps1" -InstallDir "$INSTDIR"'
  Pop $0
  ${If} $0 == 3010
    SetRebootFlag true
    DetailPrint "Windows must be restarted to finish enabling the required features."
  ${ElseIf} $0 != 0
    MessageBox MB_ICONEXCLAMATION|MB_OK "Solon setup failed (code $0).$\r$\nSee %ProgramData%\Solon\logs\setup.log, then run the installer again."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DetailPrint "Stopping and removing the Solon service…"
  nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\installer\setup.ps1" -InstallDir "$INSTDIR" -Uninstall'
  Pop $0
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON2 "Also delete Solon's container data (images, volumes)?$\r$\nFolder: %ProgramData%\Solon" /SD IDNO IDNO keep_data
    nsExec::ExecToLog 'powershell -NoProfile -ExecutionPolicy Bypass -Command "Remove-Item -Recurse -Force \"$$env:ProgramData\Solon\" -ErrorAction SilentlyContinue"'
  keep_data:
!macroend
