# La mémoire rendue à Windows, mesurée. C'est le reproche numéro un fait aux moteurs de conteneurs
# sous Windows : la machine grossit et ne rend rien. Ce banc alloue 800 Mo de vraie mémoire dans un
# conteneur, les libère, et regarde ce que le processus de la machine rend à Windows.
#
#   powershell -ExecutionPolicy Bypass -File bench\mem-reclaim.ps1
#
# Ne touche à rien d'autre : un conteneur nommé solon-mem-hog, supprimé à la fin.
param(
    [string]$Docker = "C:\Program Files\Solonin\docker.exe",
    [string]$Service = "C:\Program Files\Solon\solon-service.exe",
    [int]$AllocateMb = 800
)
$ErrorActionPreference = "Continue"

function Mem {
    $p = Get-Process vmmem, vmmemSolon -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $p) { return 0 }
    [math]::Round($p.WorkingSet64 / 1MB)
}
function Guest {
    $o = (& $Service exec "free -m | sed -n 2p" 2>$null | Select-String '"stdout"')
    if ($o -and "$o" -match 'Mem:\s+(\d+)\s+(\d+)\s+(\d+)') { return "invité : $($Matches[2]) Mo utilisés, $($Matches[3]) Mo libres" }
    ""
}
function Line($label) { "{0,-24} vmmem {1,5} Mo   {2}" -f $label, (Mem), (Guest) }

Line "repos"
& $Docker rm -f solon-mem-hog 2>$null | Out-Null
& $Docker run -d --name solon-mem-hog --shm-size="$($AllocateMb + 200)m" busybox `
    sh -c "dd if=/dev/zero of=/dev/shm/big bs=1M count=$AllocateMb 2>/dev/null; sleep 600" | Out-Null
Start-Sleep -Seconds 25
Line "$AllocateMb Mo alloués"
& $Docker rm -f solon-mem-hog | Out-Null
foreach ($i in 1..5) {
    Start-Sleep -Seconds 35
    Line "+$($i * 35) s après"
}
