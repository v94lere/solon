# Coupure brutale : terminaison de la machine (sans arrêt invité) pendant des écritures continues dans
# un volume, redémarrage, vérification du disque (fsck) et des données. Sans élévation.
. (Join-Path $PSScriptRoot "common.ps1")

$s = StartEngine
if ($s.state -ne "ready") { Check "moteur prêt" $false "$($s.state)"; Finish }
docker volume rm -f monodon-crashvol 2>$null | Out-Null
docker volume create monodon-crashvol 2>&1 | Out-Null
# Le script shell est passé tel quel (guillemets simples PowerShell) : compteur incrémenté en boucle.
# `counter` est écrit avec fsync (sync) comme le ferait une base de données ; `log` sans (page cache).
$writer = 'i=0; while true; do i=$((i+1)); echo $i > /data/counter; sync; echo $i >> /data/log; done'
$run = docker run -d --name monodon-writer -v monodon-crashvol:/data $Image sh -c $writer 2>&1
Start-Sleep 3
$state = docker inspect monodon-writer --format "{{.State.Status}} {{.State.Error}}" 2>&1
Check "écrivain en marche" ("$state" -match "^running") "run='$run' état='$state'"
$before = (& $Svc exec "cat /var/lib/docker/volumes/monodon-crashvol/_data/counter") -join "" | ConvertFrom-Json
Log "compteur avant coupure : $($before.stdout.Trim())"
& $Svc stop --force | Out-Null
Log "machine terminée brutalement ; redémarrage"
$s = StartEngine
Check "moteur prêt après coupure" ($s.state -eq "ready") "boot=$($s.last_boot_ms) ms"
$fsck = Select-String -Path $ConsoleLog -Pattern "fsck code" | Select-Object -Last 1
$fsckLine = if ($fsck) { $fsck.Line.Substring($fsck.Line.IndexOf('disque de données')) } else { "(absent)" }
Check "disque de données vérifié (fsck code 0 ou 1)" ($fsckLine -match "fsck code [01] ") $fsckLine
$after = (& $Svc exec "cat /var/lib/docker/volumes/monodon-crashvol/_data/counter; echo; wc -l < /var/lib/docker/volumes/monodon-crashvol/_data/log") -join "" | ConvertFrom-Json
$lines = @($after.stdout -split "`n" | Where-Object { $_.Trim() -ne "" })
$counter = if ($lines.Count -ge 1) { [int]$lines[0] } else { 0 }
$logLines = if ($lines.Count -ge 2) { [int]$lines[1] } else { 0 }
Check "données synchronisées (fsync) conservées" ($counter -ge 1) "compteur avant=$($before.stdout.Trim()) après=$counter"
Log "données non synchronisées : $logLines ligne(s) de journal conservées (perte attendue : fenêtre de sync ~2 s)"
$ps = docker ps -a --format "{{.Names}} {{.Status}}" 2>&1
Log "conteneurs après redémarrage : $ps"
docker container prune -f 2>&1 | Out-Null
docker volume rm -f monodon-crashvol 2>&1 | Out-Null
Finish
