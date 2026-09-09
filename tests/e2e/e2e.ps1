# Scénario de bout en bout, SANS élévation, contre un service console déjà lancé (start-console.ps1).
. (Join-Path $PSScriptRoot "common.ps1")

$s = WaitState @("ready", "failed")
Check "moteur prêt via le canal de contrôle" ($s.state -eq "ready") "state=$($s.state) boot=$($s.last_boot_ms) ms image=$($s.image_version) adresse=$($s.guest_address) crash=$($s.recovered_from_crash)"
if ($s.state -ne "ready") { Finish }

$pr = (& $Svc prereq) -join "" | ConvertFrom-Json
Check "rapport des prérequis" ($pr.ok -eq $true) (($pr.items | ForEach-Object { "$($_.id)=$($_.ok)" }) -join " ")

$v = docker version --format "{{.Server.Version}}" 2>&1
Check "docker version (utilisateur non élevé)" ($LASTEXITCODE -eq 0) "$v"

$t = [System.Diagnostics.Stopwatch]::StartNew()
$pull = docker pull $Image 2>&1 | Select-Object -Last 1
Check "docker pull $Image (réseau sortant + DNS)" ($LASTEXITCODE -eq 0) "$pull en $($t.ElapsedMilliseconds) ms"

$name = "solon-e2e-web"
docker stop $name 2>$null | Out-Null; docker container prune -f 2>$null | Out-Null
$id = docker run -d -p 8080:80 --name $name $Image sh -c "mkdir -p /srv/www && echo bonjour-solon > /srv/www/index.html && exec httpd -f -p 80 -h /srv/www" 2>&1
Check "docker run -d -p 8080:80" ($LASTEXITCODE -eq 0) "$("$id".Substring(0, [Math]::Min(12, "$id".Length)))"
$body = ""; $t = [System.Diagnostics.Stopwatch]::StartNew()
for ($i = 0; $i -lt 40 -and $body -notmatch "bonjour"; $i++) { Start-Sleep -Milliseconds 250; try { $body = (Invoke-WebRequest -Uri http://localhost:8080/ -UseBasicParsing -TimeoutSec 3).Content } catch { $body = "" } }
Check "http://localhost:8080 depuis Windows" ($body -match "bonjour-solon") "réponse='$($body.Trim())' après $($t.ElapsedMilliseconds) ms"
$st = Status
Check "port publié visible dans l'état du service" (@($st.published_ports | Where-Object { $_.host_port -eq 8080 }).Count -ge 1) (($st.published_ports | ForEach-Object { "$($_.host_ip):$($_.host_port)->$($_.container_ip):$($_.container_port)" }) -join " ")
docker stop $name 2>&1 | Out-Null; docker container prune -f 2>&1 | Out-Null
Start-Sleep -Milliseconds 800
$st = Status
Check "port retiré après suppression du conteneur" (@($st.published_ports | Where-Object { $_.host_port -eq 8080 }).Count -eq 0) "$(@($st.published_ports).Count) liaison(s)"

Log "mesure mémoire : attente 60 s d'inactivité"
Start-Sleep -Seconds 60
$vm = Get-Process | Where-Object { $_.ProcessName -like "vmmem*" -and $_.ProcessName -ne "vmmemWSL" }
$svcp = Get-Process solon-service -ErrorAction SilentlyContinue
$vmMB = [math]::Round(($vm | Measure-Object WorkingSet64 -Sum).Sum / 1MB)
$svcMB = [math]::Round(($svcp | Measure-Object WorkingSet64 -Sum).Sum / 1MB)
Log "RAM au repos : $($vm.ProcessName -join ',') working set = $vmMB Mo ; solon-service = $svcMB Mo ; total = $($vmMB + $svcMB) Mo"

$t = [System.Diagnostics.Stopwatch]::StartNew()
& $Svc stop | Out-Null
$st = Status
Check "arrêt propre" ($st.state -eq "stopped") "en $($t.ElapsedMilliseconds) ms"
Finish
