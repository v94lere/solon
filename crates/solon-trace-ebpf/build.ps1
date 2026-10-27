# Fabrique l'objet BPF de la radiographie et le dépose dans crates/solon-agent/src/bpf/.
#
# L'objet est versionné dans le dépôt : la compilation croisée de l'agent depuis Windows l'inclut
# par `include_bytes!`, et bpf-linker ne tourne pas sous Windows (voir Dockerfile). Relancer ce
# script après toute modification de crates/solon-trace-ebpf ou de solon-trace-common, puis
# commiter l'objet avec les sources.
#
# Le moteur utilisé est celui de Solon lui-même : aucun autre atelier Linux n'est nécessaire.
[CmdletBinding()]
param(
    # Reconstruit l'image de l'atelier même si elle existe déjà.
    [switch]$Rebuild
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$repo = Resolve-Path (Join-Path $here "..\..")
$image = "solon-bpf-builder"

$exists = docker image inspect $image 2>$null
if ($Rebuild -or -not $exists) {
    Write-Host "==> Atelier de compilation ($image)"
    docker build -t $image $here
    if ($LASTEXITCODE -ne 0) { throw "construction de l'image $image" }
}

Write-Host "==> Programmes eBPF"
docker run --rm -v "${repo}:/src" $image
if ($LASTEXITCODE -ne 0) { throw "compilation des programmes eBPF" }
