# Le service est tué pendant que la machine tourne ; à sa relance il doit se rattacher à la machine
# (même identifiant, conteneurs intacts) au lieu de la recréer. UAC ×2 (taskkill élevé, relance).
param([Parameter(Mandatory = $true)][string]$ImageDir, [switch]$Release)
. (Join-Path $PSScriptRoot "common.ps1")

$s = StartEngine
if ($s.state -ne "ready") { Check "moteur prêt" $false "$($s.state)"; Finish }
Log "vm_id avant : $($s.vm_id)"
docker run -d --name monodon-survivor $Image sleep 600 2>&1 | Out-Null

$kill = Join-Path $Build "kill-svc.cmd"
Set-Content -Path $kill -Value "@echo off`r`ntaskkill /F /IM monodon-service.exe" -Encoding ascii
Start-Process cmd.exe -ArgumentList "/c", "`"$kill`"" -Verb RunAs -Wait
Start-Sleep 2
$alive = @(Get-Process | Where-Object { $_.ProcessName -like "vmmem*" -and $_.ProcessName -ne "vmmemWSL" })
Check "machine toujours en vie après la mort du service" ($alive.Count -ge 1) ($alive.ProcessName -join ",")

$startArgs = @{ ImageDir = $ImageDir }; if ($Release) { $startArgs.Release = $true }
& (Join-Path $PSScriptRoot "start-console.ps1") @startArgs | Out-Null
$after = WaitState @("ready", "failed") 120
Check "service relancé et moteur prêt" ($after.state -eq "ready") "boot=$($after.last_boot_ms) ms crash=$($after.recovered_from_crash)"
Check "même machine (rattachement)" ($after.vm_id -eq $s.vm_id) "avant=$($s.vm_id) après=$($after.vm_id)"
$ps = docker ps --format "{{.Names}} {{.Status}}" 2>&1
Check "conteneur toujours en marche" ("$ps" -match "monodon-survivor") "$ps"
Check "journal : rattachement" ((Select-String -Path $ConsoleLog -Pattern "rattachement" | Measure-Object).Count -ge 1) ""
docker stop monodon-survivor 2>&1 | Out-Null; docker container prune -f 2>&1 | Out-Null
Finish
