# Arrête le moteur puis le service console (sans élévation).
. (Join-Path $PSScriptRoot "common.ps1")
& $Svc quit 2>&1 | Out-Null
Start-Sleep 3
if (Get-Process monodon-service -ErrorAction SilentlyContinue) { "le service tourne encore" ; exit 1 } else { "service arrêté" }
