# Test de l'installeur NSIS : installation silencieuse (UNE fenêtre UAC), vérification du service Windows
# réel (LocalSystem, session 0), démarrage du moteur, puis scénario de bout en bout (e2e.ps1) contre ce
# service. Ne désinstalle pas : Solon reste installé comme chez un utilisateur.
# Usage : .\tests\e2e\install-test.ps1 [-Setup <chemin de Solon_x.y.z_x64-setup.exe>]
param(
    [string]$Setup = (Join-Path $PSScriptRoot "..\..\target\release\bundle\nsis\Solon_0.1.0_x64-setup.exe")
)
$inst = Join-Path ${env:ProgramFiles} "Solon"
$env:SOLON_SERVICE_EXE = Join-Path $inst "solon-service.exe"
. (Join-Path $PSScriptRoot "common.ps1")
$Setup = (Resolve-Path $Setup).Path

# Le service console de développement ne doit pas tenir les pipes.
$dev = Join-Path $Repo "target\release\solon-service.exe"
if (Test-Path $dev) { & $dev quit 2>$null | Out-Null; Start-Sleep 3 }

Log "installation silencieuse : $Setup"
$p = Start-Process -FilePath $Setup -ArgumentList "/S" -Verb RunAs -PassThru -Wait
Check "installeur terminé" ($p.ExitCode -eq 0) "code $($p.ExitCode)"

$svc = Get-Service -Name SolonService -ErrorAction SilentlyContinue
Check "service SolonService présent" ($null -ne $svc) "$($svc.StartType)"
if ($svc) {
    for ($i = 0; $i -lt 20 -and $svc.Status -ne "Running"; $i++) { Start-Sleep 1; $svc.Refresh() }
    Check "service en cours d'exécution" ($svc.Status -eq "Running") "$($svc.Status)"
}
Check "image installée" (Test-Path (Join-Path $inst "image\manifest.json")) "$inst\image"
Check "solon-service.exe installé" (Test-Path $Svc) $Svc
$setupLog = Join-Path $env:ProgramData "Solon\logs\setup.log"
if (Test-Path $setupLog) { Get-Content $setupLog -Tail 12 | ForEach-Object { Log "  setup.log | $_" } }

$t = [System.Diagnostics.Stopwatch]::StartNew()
$s = StartEngine
Check "moteur démarré par le vrai service Windows" ($s.state -eq "ready") "state=$($s.state) en $($t.ElapsedMilliseconds) ms image=$($s.image_version)"
if ($s.state -ne "ready") { Get-Content (Join-Path $env:ProgramData "Solon\logs\solon-service.log*") -Tail 30 | ForEach-Object { Log "  log | $_" }; Finish }

Log "scénario e2e contre le service installé"
& (Join-Path $PSScriptRoot "e2e.ps1")
if ($LASTEXITCODE -ne 0) { $script:fail += $LASTEXITCODE }
Finish
