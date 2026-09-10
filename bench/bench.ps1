# Mesures comparables Docker Desktop / Solon, sur le moteur que le CLI `docker` utilise actuellement.
# Usage : powershell -ExecutionPolicy Bypass -File bench\bench.ps1 [-Docker "C:\Program Files\Solon\bin\docker.exe"]
#         [-SkipDb] (saute la création de base Odoo, ~15 s) [-IdleSeconds 120]
# Le script ne modifie que le projet Compose `solon-bench` (port 18069) et ses deux volumes, supprimés à la fin.
param(
    [string]$Docker = "docker",
    [switch]$SkipDb,
    [int]$IdleSeconds = 120
)
$ErrorActionPreference = "Continue"   # les messages de progression de docker arrivent sur stderr : ne pas les traiter en erreurs
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$compose = Join-Path $here "compose.yaml"
$project = "solon-bench"
$url = "http://localhost:18069/web/login"

function Sec($sw) { "{0:N1} s" -f $sw.Elapsed.TotalSeconds }
function Mb($bytes) { "{0:N0} Mo" -f ($bytes / 1MB) }

# Quel moteur ? D'après le nom du serveur et le tuyau utilisés par le CLI.
$info = & $Docker info --format '{{.Name}} | {{.ServerVersion}} | {{.OperatingSystem}}' 2>$null
if (-not $info) { throw "docker info ne répond pas : le moteur est-il démarré ?" }
$isSolon = ($info -match 'solon') -or ((& $Docker context inspect --format '{{.Endpoints.docker.Host}}' 2>$null) -match 'solon')
$engine = if ($isSolon) { "Solon" } else { "Docker Desktop" }
"Moteur : $engine ($info)"

# Mémoire côté Windows : processus du moteur et de son interface.
function EngineMemory {
    $names = if ($isSolon) { '^(solon|solon-service|vmmem|vmwp)$' } else { '^(vmmemWSL|vmmem|Docker Desktop.*|com\.docker.*|docker.*)$' }
    $ps = Get-Process | Where-Object { $_.Name -match $names }
    $sum = ($ps | Measure-Object WorkingSet64 -Sum).Sum
    $detail = ($ps | Group-Object Name | ForEach-Object { "{0} {1}" -f $_.Name, (Mb (($_.Group | Measure-Object WorkingSet64 -Sum).Sum)) }) -join ", "
    return @{ total = $sum; detail = $detail }
}

$results = [ordered]@{}

# 1. Démarrage d'un conteneur (à chaud : image présente).
& $Docker pull -q public.ecr.aws/docker/library/busybox:1.36 | Out-Null
$times = 1..3 | ForEach-Object { $sw = [Diagnostics.Stopwatch]::StartNew(); & $Docker run --rm public.ecr.aws/docker/library/busybox:1.36 true | Out-Null; $sw.Elapsed.TotalSeconds }
$results["docker run --rm busybox true (3 essais)"] = ($times | ForEach-Object { "{0:N2} s" -f $_ }) -join " / "

# 2. Pile Odoo + PostgreSQL : down puis up, temps jusqu'à la première réponse HTTP 200.
& $Docker compose -p $project -f $compose pull -q 2>$null | Out-Null
& $Docker compose -p $project -f $compose up -d 2>$null | Out-Null   # premier passage : création des volumes
Start-Sleep 5
$sw = [Diagnostics.Stopwatch]::StartNew(); & $Docker compose -p $project -f $compose down 2>$null | Out-Null; $results["compose down"] = Sec $sw
$sw = [Diagnostics.Stopwatch]::StartNew(); & $Docker compose -p $project -f $compose up -d 2>$null | Out-Null; $results["compose up -d (images présentes)"] = Sec $sw
$sw = [Diagnostics.Stopwatch]::StartNew(); $ok = $false
while (-not $ok -and $sw.Elapsed.TotalSeconds -lt 90) { try { $ok = ((Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 5).StatusCode -eq 200) } catch { Start-Sleep -Milliseconds 200 } }
$results["Odoo répond (HTTP 200) après up"] = if ($ok) { Sec $sw } else { "pas de réponse en 90 s" }

# 3. Tâche CPU : création d'une base Odoo avec données de démonstration (surtout mono-thread).
if (-not $SkipDb) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    & $Docker compose -p $project -f $compose exec -T odoo odoo -d benchdb -i base --stop-after-init --db_host=db --db_user=odoo --db_password=odoo 2>$null | Out-Null
    $results["création d'une base Odoo avec démo"] = Sec $sw
}

# 4. Latence d'une page, moyenne de 5 chargements (sur la base créée ; sinon le sélecteur de bases).
$lat = 1..5 | ForEach-Object { $sw = [Diagnostics.Stopwatch]::StartNew(); Invoke-WebRequest -Uri ($url + $(if ($SkipDb) { "" } else { "?db=benchdb" })) -UseBasicParsing -TimeoutSec 10 | Out-Null; $sw.Elapsed.TotalMilliseconds }
$results["page de connexion Odoo (moyenne de 5)"] = "{0:N0} ms" -f ($lat | Measure-Object -Average).Average

# 5. Écritures synchrones sur un volume Docker (ce que fait une base de données).
$fs = & $Docker compose -p $project -f $compose exec -T db sh -c "pg_test_fsync -s 3 -f /var/lib/postgresql/data/fsync.tmp 2>/dev/null | grep -E '^\s+(fdatasync|fsync)\s' | head -2; rm -f /var/lib/postgresql/data/fsync.tmp"
$results["pg_test_fsync (fdatasync / fsync, 8 ko)"] = (($fs | ForEach-Object { ($_ -replace '\s+', ' ').Trim() }) -join " ; ")

# 6. Mémoire côté Windows avec la pile qui tourne, après repos.
"Repos de $IdleSeconds s pour la mesure de mémoire…"
Start-Sleep $IdleSeconds
$m = EngineMemory
$others = (& $Docker ps -q | Measure-Object).Count - 2
$results["mémoire Windows du moteur, pile au repos $IdleSeconds s"] = "{0} ({1}) ; autres conteneurs en cours : {2}" -f (Mb $m.total), $m.detail, $others

# Nettoyage.
& $Docker compose -p $project -f $compose down -v 2>$null | Out-Null

""
"| Mesure | $engine |"
"|---|---|"
foreach ($k in $results.Keys) { "| $k | $($results[$k]) |" }
