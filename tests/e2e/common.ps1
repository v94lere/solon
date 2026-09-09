# Variables communes aux scripts de bout en bout.
$Repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Build = Join-Path $Repo ".local\build"
New-Item -ItemType Directory -Force $Build | Out-Null
$Svc = if ($env:SOLON_SERVICE_EXE) { $env:SOLON_SERVICE_EXE } else { Join-Path $Repo "target\debug\solon-service.exe" }
$Root = if ($env:SOLON_ROOT) { $env:SOLON_ROOT } else { Join-Path $Build "solon-root" }
$ConsoleLog = Join-Path $Build "service-console.log"
$Image = "public.ecr.aws/docker/library/busybox:1.36"
$env:DOCKER_HOST = "npipe:////./pipe/solon"
# Configuration Docker CLI vide : évite le gestionnaire d'identifiants Windows de Docker Desktop.
$DockerConfig = Join-Path $Build "docker-config"
New-Item -ItemType Directory -Force $DockerConfig | Out-Null
$env:DOCKER_CONFIG = $DockerConfig

$script:sw = [System.Diagnostics.Stopwatch]::StartNew()
$script:fail = 0
function Log($m) { "[{0,6} ms] {1}" -f $script:sw.ElapsedMilliseconds, $m }
function Check($name, $ok, $detail) { if ($ok) { Log "OK    $name - $detail" } else { Log "ECHEC $name - $detail"; $script:fail++ } }
function Status { ((& $Svc status) -join "") | ConvertFrom-Json }
function WaitState([string[]]$states, [int]$seconds = 120) {
    $deadline = (Get-Date).AddSeconds($seconds); $s = $null
    do {
        Start-Sleep -Milliseconds 500
        $j = & $Svc status 2>$null
        if ($LASTEXITCODE -eq 0 -and $j) { $s = ($j -join "") | ConvertFrom-Json }
    } while ((Get-Date) -lt $deadline -and -not ($s -and ($states -contains $s.state)))
    return $s
}
function StartEngine { & $Svc start 2>&1 | Out-Null; return (WaitState @("ready", "failed")) }
function Finish { Log "RESULTAT : $(if ($script:fail -eq 0) { 'OK' } else { 'ECHEC' }) (échecs : $script:fail)"; exit $script:fail }
