# Test « machine vierge », phase 1 (ÉLEVÉ, une fenêtre UAC). Remet la machine dans l'état d'un PC sans
# Docker : désinstalle Docker Desktop, les distributions WSL et WSL, désinstalle Solon et ses données,
# désactive Hyper-V et la Plateforme de machine virtuelle. Un redémarrage est ensuite nécessaire.
# Journal : %ProgramData%\Solon-test\phase1.log. Ne touche pas au dépôt ni aux sauvegardes du Bureau.
param([switch]$KeepUbuntu)
$ErrorActionPreference = "Continue"
$logDir = Join-Path $env:ProgramData "Solon-test"; New-Item -ItemType Directory -Force $logDir | Out-Null
$log = Join-Path $logDir "phase1.log"
function Log($m) { $l = "{0:HH:mm:ss} {1}" -f (Get-Date), $m; Add-Content $log $l; Write-Host $l }
Log "=== phase 1 : retour à une machine sans Docker ==="

# 1. Docker Desktop
$dd = "C:\Program Files\Docker\Docker\Docker Desktop Installer.exe"
Get-Process "Docker Desktop","com.docker.backend","com.docker.build" -ErrorAction SilentlyContinue | Stop-Process -Force
if (Test-Path $dd) {
    Log "désinstallation de Docker Desktop…"
    $p = Start-Process -FilePath $dd -ArgumentList "uninstall","--quiet" -Wait -PassThru
    Log "Docker Desktop : code $($p.ExitCode)"
} else { Log "Docker Desktop : absent" }

# 2. Distributions WSL puis WSL
foreach ($d in @("docker-desktop","docker-desktop-data")) { wsl --unregister $d 2>$null | Out-Null; Log "distribution $d retirée (si présente)" }
if (-not $KeepUbuntu) { wsl --unregister Ubuntu 2>$null | Out-Null; Log "distribution Ubuntu retirée (sauvegarde : Bureau\ubuntu-wsl-sauvegarde.tar)" }
wsl --shutdown 2>$null
$wslMsi = Get-ItemProperty HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -eq "Windows Subsystem for Linux" }
if ($wslMsi) {
    $code = ($wslMsi.UninstallString -replace '.*(\{[0-9A-Fa-f-]+\}).*','$1')
    Log "désinstallation de WSL ($code)…"
    $p = Start-Process msiexec.exe -ArgumentList "/x",$code,"/qn","/norestart" -Wait -PassThru
    Log "WSL : code $($p.ExitCode)"
} else { Log "WSL (MSI) : absent" }

# 3. Solon (désinstalleur silencieux, puis données)
Get-Process solon -ErrorAction SilentlyContinue | Stop-Process -Force
$un = "C:\Program Files\Solon\uninstall.exe"
if (Test-Path $un) {
    Log "désinstallation de Solon…"
    $p = Start-Process -FilePath $un -ArgumentList "/S" -Wait -PassThru
    Log "Solon : code $($p.ExitCode)"
} else { Log "Solon : absent" }
Start-Sleep 3
if (Test-Path "$env:ProgramData\Solon") {
    Remove-Item -Recurse -Force "$env:ProgramData\Solon" -ErrorAction SilentlyContinue
    Log "données de Solon supprimées : $(-not (Test-Path "$env:ProgramData\Solon"))"
}
if (Test-Path "C:\Program Files\Solon") { Remove-Item -Recurse -Force "C:\Program Files\Solon" -ErrorAction SilentlyContinue }
$hosts = Get-Content "$env:SystemRoot\System32\drivers\etc\hosts" -ErrorAction SilentlyContinue
if ($hosts -match "solon-begin") { Log "AVERTISSEMENT : bloc Solon encore présent dans hosts" } else { Log "hosts : propre" }

# 4. Composants Windows
foreach ($f in @("Microsoft-Hyper-V-All","VirtualMachinePlatform","Microsoft-Windows-Subsystem-Linux")) {
    try {
        $st = (Get-WindowsOptionalFeature -Online -FeatureName $f -ErrorAction Stop).State
        if ($st -eq "Enabled") { $r = Disable-WindowsOptionalFeature -Online -FeatureName $f -NoRestart -ErrorAction Stop; Log "composant $f : désactivé (redémarrage requis : $($r.RestartNeeded))" }
        else { Log "composant $f : déjà $st" }
    } catch { Log "composant $f : $($_.Exception.Message)" }
}
Log "=== phase 1 terminée : REDÉMARRER WINDOWS, puis lancer l'installeur Solon depuis le Bureau ==="
Log "Après l'installation (elle demandera un redémarrage), redémarrer encore, puis exécuter tests\e2e\fresh-machine-phase2.ps1"
