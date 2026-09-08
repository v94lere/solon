# Télécharge le CLI Docker officiel (binaire statique Windows) et le plugin Compose, pour les livrer avec
# Monodon dans <install>\bin (Apache-2.0, redistribution autorisée). Sortie : installer\vendor\ (ignoré par git).
# Usage : .\installer\fetch-docker-cli.ps1 [-DockerVersion 29.7.2] [-ComposeVersion 5.1.4]
param(
    [string]$DockerVersion = "29.7.2",
    [string]$ComposeVersion = "5.1.4"
)
$ErrorActionPreference = "Stop"
$vendor = Join-Path $PSScriptRoot "vendor"
New-Item -ItemType Directory -Force (Join-Path $vendor "cli-plugins") | Out-Null
$stamp = Join-Path $vendor "VERSIONS.txt"
if ((Test-Path $stamp) -and ((Get-Content $stamp -Raw) -match "docker=$DockerVersion" -and (Get-Content $stamp -Raw) -match "compose=$ComposeVersion")) {
    "déjà présent : docker $DockerVersion, compose $ComposeVersion"; exit 0
}
# curl.exe (livré avec Windows) : Invoke-WebRequest échoue en TLS sur certains postes.
function Fetch($url, $out) { & curl.exe -fsSL --retry 3 -o $out $url; if ($LASTEXITCODE -ne 0) { throw "téléchargement échoué : $url" } }
$zip = Join-Path $env:TEMP "docker-$DockerVersion.zip"
"téléchargement du CLI Docker $DockerVersion…"
Fetch "https://download.docker.com/win/static/stable/x86_64/docker-$DockerVersion.zip" $zip
$tmp = Join-Path $env:TEMP "docker-cli-extract"
Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
Expand-Archive -Path $zip -DestinationPath $tmp
Copy-Item (Join-Path $tmp "docker\docker.exe") (Join-Path $vendor "docker-cli.exe") -Force
Remove-Item -Recurse -Force $tmp, $zip -ErrorAction SilentlyContinue
"téléchargement de Docker Compose $ComposeVersion…"
Fetch "https://github.com/docker/compose/releases/download/v$ComposeVersion/docker-compose-windows-x86_64.exe" (Join-Path $vendor "cli-plugins\docker-compose.exe")
@"
Composants tiers livrés avec Monodon (dossier bin) :
- docker-cli.exe : Docker CLI $DockerVersion (https://github.com/docker/cli), licence Apache-2.0
- cli-plugins\docker-compose.exe : Docker Compose $ComposeVersion (https://github.com/docker/compose), licence Apache-2.0
Monodon les lance tels quels ; docker.exe (Monodon) ne fait que les diriger vers le moteur Monodon.
"@ | Set-Content -Encoding UTF8 (Join-Path $vendor "NOTICE-third-party.txt")
"docker=$DockerVersion`ncompose=$ComposeVersion" | Set-Content -Encoding ascii $stamp
Get-ChildItem $vendor -Recurse -File | ForEach-Object { "{0,10:N0} Ko  {1}" -f ($_.Length/1KB), $_.FullName.Substring($vendor.Length + 1) }
