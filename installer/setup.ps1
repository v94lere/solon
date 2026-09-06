# Configuration système de Solon, exécutée élevée par l'installeur NSIS (voir hooks.nsh).
# - active les composants Windows requis (Hyper-V, Plateforme de machine virtuelle) ;
# - installe (ou retire) le service Windows SolonService.
# Codes de retour : 0 OK, 3010 redémarrage requis, autre = erreur. Journal : %ProgramData%\Solon\logs\setup.log
param(
    [Parameter(Mandatory = $true)][string]$InstallDir,
    [switch]$Uninstall
)
$ErrorActionPreference = "Continue"
# Les binaires Rust écrivent en UTF-8 : lire leur sortie correctement dans le journal.
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$logDir = Join-Path $env:ProgramData "Solon\logs"
New-Item -ItemType Directory -Force $logDir | Out-Null
$log = Join-Path $logDir "setup.log"
function Log($m) { $line = "{0:yyyy-MM-dd HH:mm:ss} {1}" -f (Get-Date), $m; Add-Content -Path $log -Value $line; Write-Output $line }
$svc = Join-Path $InstallDir "solon-service.exe"

$binDir = Join-Path $InstallDir "bin"
function Set-MachinePath($present) {
    # Ajoute (ou retire) <install>in en tête du PATH machine : `docker` et `docker compose` de Solon.
    $key = "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment"
    $current = (Get-ItemProperty -Path $key -Name Path).Path
    $parts = @($current -split ";" | Where-Object { $_ -and ($_.TrimEnd("\") -ne $binDir.TrimEnd("\")) })
    if ($present) { $parts = @($binDir) + $parts }
    Set-ItemProperty -Path $key -Name Path -Value ($parts -join ";") -Type ExpandString
    # Prévenir les processus ouverts (Explorateur, nouveaux terminaux) du changement d'environnement.
    $sig = '[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)] public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);'
    $w = Add-Type -MemberDefinition $sig -Name "EnvBroadcast" -Namespace "Solon" -PassThru
    $r = [UIntPtr]::Zero
    $w::SendMessageTimeout([IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, "Environment", 2, 5000, [ref]$r) | Out-Null
}

if ($Uninstall) {
    Log "uninstall: stopping and removing the service"
    & $svc uninstall 2>&1 | ForEach-Object { Log $_ }
    try { Set-MachinePath $false; Log "PATH: $binDir removed" } catch { Log "PATH: $($_.Exception.Message)" }
    # Nettoyage des enregistrements HvSocket créés par une ancienne version de ce script.
    $base = "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Virtualization\GuestCommunicationServices"
    Get-ChildItem $base -ErrorAction SilentlyContinue | Where-Object { ($_ | Get-ItemProperty).ElementName -like "Solon vsock *" } | Remove-Item -Force
    exit 0
}

Log "installing from $InstallDir"
$needsReboot = $false

# 1. Composants Windows.
foreach ($feature in @("Microsoft-Hyper-V", "VirtualMachinePlatform")) {
    try {
        $info = Get-WindowsOptionalFeature -Online -FeatureName $feature -ErrorAction Stop
    } catch {
        Log "feature ${feature}: not available on this edition ($($_.Exception.Message))"
        # Sur Windows Famille, Microsoft-Hyper-V n'existe pas : l'application affichera UNSUPPORTED_WINDOWS_EDITION.
        continue
    }
    if ($info.State -eq "Enabled") { Log "feature ${feature}: already enabled"; continue }
    Log "feature ${feature}: enabling"
    try {
        $r = Enable-WindowsOptionalFeature -Online -FeatureName $feature -All -NoRestart -ErrorAction Stop
        if ($r.RestartNeeded) { $needsReboot = $true }
        Log "feature ${feature}: enabled (restart needed: $($r.RestartNeeded))"
    } catch {
        Log "feature ${feature}: FAILED $($_.Exception.Message) - the app will report the missing prerequisite"
    }
}

# 2. (Aucun enregistrement HvSocket n'est nécessaire : toutes les connexions sont ouvertes par l'hôte.)

# 2b. CLI docker / docker compose de Solon dans le PATH machine (nouveaux terminaux).
try { Set-MachinePath $true; Log "PATH: $binDir added (SOLON_BIN)" } catch { Log "PATH: FAILED $($_.Exception.Message)" }

# 3. Service Windows : (ré)installation puis démarrage.
& $svc uninstall 2>&1 | Out-Null
$out = & $svc install 2>&1
$out | ForEach-Object { Log $_ }
if ($LASTEXITCODE -ne 0) { Log "service install: exit code $LASTEXITCODE"; exit 1 }
if (-not $needsReboot) {
    Start-Service -Name SolonService -ErrorAction SilentlyContinue
    Log "service started: $((Get-Service SolonService).Status)"
}

if ($needsReboot) { Log "restart required"; exit 3010 }
exit 0
