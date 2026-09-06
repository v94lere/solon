# Test « machine vierge », phase 2 (sans élévation) : après l'installation de Solon et le redémarrage,
# vérifie que tout marche sans Docker Desktop ni WSL. Écrit un rapport dans %ProgramData%\Solon-test\phase2.log.
$ErrorActionPreference = "Continue"
$logDir = Join-Path $env:ProgramData "Solon-test"; New-Item -ItemType Directory -Force $logDir | Out-Null
$log = Join-Path $logDir "phase2.log"
$fail = 0
function Log($m) { $l = "{0:HH:mm:ss} {1}" -f (Get-Date), $m; Add-Content $log $l; Write-Host $l }
function Check($name, $ok, $detail) { if ($ok) { Log "OK    $name - $detail" } else { Log "ECHEC $name - $detail"; $script:fail++ } }
Log "=== phase 2 : Solon seul sur la machine ==="

Check "Docker Desktop absent" (-not (Test-Path "C:\Program Files\Docker\Docker")) ""
# Windows garde un wsl.exe de façade même sans WSL : on vérifie l'absence du paquet MSI.
$wslMsi = Get-ItemProperty HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\* -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -eq "Windows Subsystem for Linux" }
Check "WSL absent (paquet désinstallé)" ($null -eq $wslMsi) "$(if ($wslMsi) { $wslMsi.DisplayVersion } else { 'aucun paquet WSL ; wsl.exe de Windows répond « non installé »' })"
$svcObj = Get-Service SolonService -ErrorAction SilentlyContinue
Check "service SolonService installé et démarré" ($svcObj -and $svcObj.Status -eq "Running") "$($svcObj.Status)"
$svc = "C:\Program Files\Solon\solon-service.exe"
Check "solon-service.exe présent" (Test-Path $svc) $svc
if (-not (Test-Path $svc)) { Log "RESULTAT : ECHEC ($fail)"; exit 1 }

$pr = (& $svc prereq) -join "" | ConvertFrom-Json
Check "prérequis tous OK (composants activés par l'installeur)" ($pr.ok -eq $true) (($pr.items | ForEach-Object { "$($_.id)=$($_.ok)" }) -join " ")
$t = [Diagnostics.Stopwatch]::StartNew()
& $svc start | Out-Null
do { Start-Sleep 1; $s = (& $svc status) -join "" | ConvertFrom-Json } while ($s.state -notin @("ready","failed") -and $t.ElapsedMilliseconds -lt 120000)
Check "moteur démarré (premier démarrage, disque créé)" ($s.state -eq "ready") "state=$($s.state) $($t.ElapsedMilliseconds) ms image=$($s.image_version) $($s.error.code) $($s.error.message)"
if ($s.state -ne "ready") { Log "RESULTAT : ECHEC ($fail)"; exit 1 }

$env:DOCKER_CONFIG = Join-Path $logDir "docker-config"; New-Item -ItemType Directory -Force $env:DOCKER_CONFIG | Out-Null
$d = "C:\Program Files\Solon\bin\docker.exe"
Check "docker.exe de Solon dans le PATH machine" (((Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment").Path) -like "*Solon\bin*") ""
$v = & $d version --format "{{.Client.Version}} / {{.Server.Version}}" 2>&1
Check "docker version" ($LASTEXITCODE -eq 0) "$v"
$t.Restart(); $out = & $d run --rm public.ecr.aws/docker/library/hello-world:latest 2>&1
Check "docker run hello-world (réseau sortant + moteur)" (($out -join " ") -match "Hello from Docker") "$($t.ElapsedMilliseconds) ms"
New-Item -ItemType Directory -Force "$env:TEMP\solon-fresh" | Out-Null; "bonjour" | Set-Content "$env:TEMP\solon-fresh\in.txt"
$out = & $d run --rm -v "${env:TEMP}\solon-fresh:/data" public.ecr.aws/docker/library/busybox:1.36 sh -c "cat /data/in.txt; echo retour > /data/out.txt" 2>&1
Check "montage d'un dossier Windows (solonfs) aller-retour" ((($out -join " ") -match "bonjour") -and (Test-Path "$env:TEMP\solon-fresh\out.txt")) "$out"
$name = "fresh-web"; & $d rm -f $name 2>&1 | Out-Null
& $d run -d --name $name -p 8085:80 public.ecr.aws/docker/library/busybox:1.36 sh -c "mkdir -p /srv && echo solon-ok > /srv/index.html && exec httpd -f -p 80 -h /srv" 2>&1 | Out-Null
$body = ""; for ($i = 0; $i -lt 40 -and $body -notmatch "solon-ok"; $i++) { Start-Sleep -Milliseconds 250; try { $body = (Invoke-WebRequest -Uri http://localhost:8085/ -UseBasicParsing -TimeoutSec 3).Content } catch { $body = "" } }
Check "port publié relayé sur localhost" ($body -match "solon-ok") "'$($body.Trim())'"
Start-Sleep 4
try { $dom = (Invoke-WebRequest -Uri "http://$name.solon.local/" -UseBasicParsing -TimeoutSec 5).Content } catch { $dom = "$($_.Exception.Message)" }
Check "domaine local $name.solon.local" ($dom -match "solon-ok") "'$($dom.Trim())'"
& $d rm -f $name 2>&1 | Out-Null
$vm = Get-Process | Where-Object { $_.ProcessName -like "vmmem*" }
Log "mémoire du moteur : $([math]::Round(($vm | Measure-Object WorkingSet64 -Sum).Sum/1MB)) Mo"
Log "RESULTAT : $(if ($fail -eq 0) { 'OK' } else { 'ECHEC' }) (échecs : $fail)"
exit $fail
