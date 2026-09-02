# Configuration système de Solon, exécutée élevée par l'installeur NSIS (voir hooks.nsh).
# - active les composants Windows requis (Hyper-V, Plateforme de machine virtuelle) ;
# - enregistre les identifiants de services HvSocket de Solon (réservé aux connexions invité → hôte) ;
# - installe (ou retire) le service Windows SolonService.
# Codes de retour : 0 OK, 3010 redémarrage requis, autre = erreur. Journal : %ProgramData%\Solon\logs\setup.log
param(
    [Parameter(Mandatory = $true)][string]$InstallDir,
    [switch]$Uninstall
)
$ErrorActionPreference = "Continue"
$logDir = Join-Path $env:ProgramData "Solon\logs"
New-Item -ItemType Directory -Force $logDir | Out-Null
$log = Join-Path $logDir "setup.log"
function Log($m) { $line = "{0:yyyy-MM-dd HH:mm:ss} {1}" -f (Get-Date), $m; Add-Content -Path $log -Value $line; Write-Output $line }
$svc = Join-Path $InstallDir "solon-service.exe"

if ($Uninstall) {
    Log "désinstallation : arrêt et suppression du service"
    & $svc uninstall 2>&1 | ForEach-Object { Log $_ }
    exit 0
}

Log "installation depuis $InstallDir"
$needsReboot = $false

# 1. Composants Windows.
foreach ($feature in @("Microsoft-Hyper-V", "VirtualMachinePlatform")) {
    try {
        $info = Get-WindowsOptionalFeature -Online -FeatureName $feature -ErrorAction Stop
    } catch {
        Log "composant $feature : introuvable sur cette édition ($($_.Exception.Message))"
        # Sur Windows Famille, Microsoft-Hyper-V n'existe pas : l'application affichera UNSUPPORTED_WINDOWS_EDITION.
        continue
    }
    if ($info.State -eq "Enabled") { Log "composant $feature : déjà activé"; continue }
    Log "composant $feature : activation"
    try {
        $r = Enable-WindowsOptionalFeature -Online -FeatureName $feature -All -NoRestart -ErrorAction Stop
        if ($r.RestartNeeded) { $needsReboot = $true }
        Log "composant $feature : activé (redémarrage requis : $($r.RestartNeeded))"
    } catch {
        Log "composant $feature : ÉCHEC $($_.Exception.Message) — l'application détectera le prérequis manquant"
    }
}

# 2. Services HvSocket (identifiants réservés à Solon, ports vsock 5000-5003 et 9000-9199).
$base = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Virtualization\GuestCommunicationServices"
foreach ($port in (5000..5003) + (9000..9199)) {
    $guid = ("{0:x8}-facb-11e6-bd58-64006a7986d3" -f $port)
    $key = Join-Path $base $guid
    if (-not (Test-Path $key)) {
        New-Item -Path $key -Force | Out-Null
        Set-ItemProperty -Path $key -Name "ElementName" -Value "Solon vsock $port"
    }
}
Log "services HvSocket enregistrés"

# 3. Service Windows : (ré)installation puis démarrage.
& $svc uninstall 2>&1 | Out-Null
$out = & $svc install 2>&1
$out | ForEach-Object { Log $_ }
if ($LASTEXITCODE -ne 0) { Log "installation du service : code $LASTEXITCODE"; exit 1 }
if (-not $needsReboot) {
    Start-Service -Name SolonService -ErrorAction SilentlyContinue
    Log "service démarré : $((Get-Service SolonService).Status)"
}

if ($needsReboot) { Log "redémarrage requis"; exit 3010 }
exit 0
