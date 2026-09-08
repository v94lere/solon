# Lance le service en mode console, élevé (UAC), avec un dossier racine de test.
# Usage : .\start-console.ps1 -ImageDir C:\...\image\out\0.1.0-dev.2 [-Release]
param(
    [Parameter(Mandatory = $true)][string]$ImageDir,
    [switch]$Release
)
. (Join-Path $PSScriptRoot "common.ps1")
if ($Release) { $Svc = Join-Path $Repo "target\release\monodon-service.exe" }
$cmd = Join-Path $Build "service-console.cmd"
@"
@echo off
set MONODON_ROOT=$Root
set MONODON_IMAGE_DIR=$ImageDir
set RUST_LOG=info,guest=info,monodon_service=debug
"$Svc" console --start > "$ConsoleLog" 2>&1
"@ | Set-Content -Path $cmd -Encoding ascii
Remove-Item $ConsoleLog -ErrorAction SilentlyContinue
Start-Process cmd.exe -ArgumentList "/c", "`"$cmd`"" -Verb RunAs
$s = WaitState @("ready", "failed") 180
Log "state=$($s.state) boot=$($s.last_boot_ms) ms image=$($s.image_version) adresse=$($s.guest_address) crash=$($s.recovered_from_crash)"
if ($s.state -ne "ready") { $s.error | ConvertTo-Json -Compress; exit 1 }
