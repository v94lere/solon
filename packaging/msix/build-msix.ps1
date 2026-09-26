# Construit le paquet MSIX de Solon à partir des binaires déjà compilés.
#
#   .\packaging\msix\build-msix.ps1                 # paquet de test, signé avec un certificat local
#   .\packaging\msix\build-msix.ps1 -Store          # paquet à téléverser au Partner Center (non signé)
#
# Prérequis : `cargo build --release -p solon-service -p solon-docker-shim` puis, dans apps\desktop,
# `npm run tauri build` (ou au moins `cargo build --release` pour solon.exe). Le SDK Windows fournit
# makeappx, makepri et signtool.
#
# Pour le Store, l'identité et l'éditeur viennent du Partner Center (Identité du produit) :
#   -IdentityName 12345Editeur.Solon -Publisher "CN=ABCD1234-..." -PublisherDisplayName "Valère Neveux"
[CmdletBinding()]
param(
    [string]$Version,
    [string]$IdentityName = "ValereNeveux.Solon",
    [string]$Publisher = "CN=Valere Neveux",
    [string]$PublisherDisplayName = "Valère Neveux",
    # Nom réservé au Partner Center. La certification exige qu'il corresponde ; le nom affiché dans
    # Windows reste « Solon », il vient du manifeste et ne change pas.
    [string]$DisplayName = "Solon",
    [switch]$Store,
    [string]$Out
)

$ErrorActionPreference = "Continue"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$msixDir = $PSScriptRoot

function Fail($m) { Write-Host "ERREUR : $m" -ForegroundColor Red; exit 1 }
function Step($m) { Write-Host "==> $m" -ForegroundColor Cyan }

# --- Version : celle de tauri.conf.json, complétée en quatre nombres (MSIX l'exige).
if (-not $Version) {
    $conf = Get-Content (Join-Path $repo "apps\desktop\src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json
    $Version = $conf.version
}
if ($Version -notmatch '^\d+\.\d+\.\d+(\.\d+)?$') { Fail "version inattendue : $Version" }
if (($Version -split '\.').Count -eq 3) { $Version = "$Version.0" }
# Le Store refuse une révision non nulle : elle lui est réservée.
if (-not $Version.EndsWith(".0")) { Fail "la dernière partie de la version doit être 0 (réservée au Store) : $Version" }

# --- Outils du SDK Windows : on prend la version la plus récente installée.
$sdkRoot = "C:\Program Files (x86)\Windows Kits\10\bin"
if (-not (Test-Path $sdkRoot)) { Fail "SDK Windows introuvable ($sdkRoot). Installez « Windows SDK » depuis Visual Studio Installer." }
$sdk = Get-ChildItem $sdkRoot -Directory |
    Where-Object { $_.Name -match '^10\.' -and (Test-Path (Join-Path $_.FullName "x64\makeappx.exe")) } |
    Sort-Object { [version]$_.Name } | Select-Object -Last 1
if (-not $sdk) { Fail "makeappx.exe introuvable sous $sdkRoot" }
$makeappx = Join-Path $sdk.FullName "x64\makeappx.exe"
$makepri = Join-Path $sdk.FullName "x64\makepri.exe"
$signtool = Join-Path $sdk.FullName "x64\signtool.exe"
Step "SDK Windows $($sdk.Name)"

# --- Fichiers à empaqueter : les mêmes que ceux de l'installeur NSIS, moins ce qui n'a pas de sens
#     dans un MSIX (le désinstalleur, et setup.ps1 qui installe le service et modifie le PATH).
$imageVer = (Get-Content (Join-Path $repo "apps\desktop\src-tauri\tauri.conf.json") -Raw | ConvertFrom-Json).bundle.resources.PSObject.Properties |
    Where-Object { $_.Value -eq "image/manifest.json" } | ForEach-Object { $_.Name }
if (-not $imageVer) { Fail "impossible de déduire la version de l'image depuis tauri.conf.json" }
$imageDir = Split-Path (Join-Path $repo ($imageVer -replace '^\.\./\.\./\.\./', '')) -Parent

$files = @(
    @{ From = "target\release\solon.exe";                                  To = "solon.exe" }
    @{ From = "target\release\solon-service.exe";                          To = "solon-service.exe" }
    @{ From = "target\release\docker.exe";                                 To = "bin\docker.exe" }
    @{ From = "installer\vendor\docker-cli.exe";                           To = "bin\docker-cli.exe" }
    @{ From = "installer\vendor\cli-plugins\docker-compose.exe";           To = "bin\cli-plugins\docker-compose.exe" }
    @{ From = "installer\vendor\NOTICE-third-party.txt";                   To = "bin\NOTICE-third-party.txt" }
    @{ From = "installer\setup-features.ps1";                              To = "installer\setup-features.ps1" }
    @{ From = "LICENSE";                                                   To = "LICENSE.txt" }
)
foreach ($n in @("vmlinuz", "initrd.img", "rootfs.vhd", "manifest.json")) {
    $files += @{ From = (Join-Path $imageDir $n | Resolve-Path -Relative -ErrorAction SilentlyContinue); To = "image\$n"; Abs = (Join-Path $imageDir $n) }
}

# --- Mise en place du dossier à empaqueter.
$layout = Join-Path $env:TEMP "solon-msix-layout"
if (Test-Path $layout) { Remove-Item -Recurse -Force $layout }
New-Item -ItemType Directory -Force $layout | Out-Null
Step "copie des fichiers"
foreach ($f in $files) {
    $src = if ($f.Abs) { $f.Abs } else { Join-Path $repo $f.From }
    if (-not (Test-Path $src)) { Fail "fichier manquant : $src`n  Compilez d'abord : cargo build --release -p solon-service -p solon-docker-shim, puis npm run tauri build dans apps\desktop." }
    $dst = Join-Path $layout $f.To
    New-Item -ItemType Directory -Force (Split-Path $dst) | Out-Null
    Copy-Item $src $dst -Force
}

# --- Visuels.
$assets = Join-Path $layout "Assets"
New-Item -ItemType Directory -Force $assets | Out-Null
Copy-Item (Join-Path $msixDir "Assets\*.png") $assets -Force
foreach ($n in @("Square44x44Logo.png", "Square71x71Logo.png", "Square150x150Logo.png", "Square310x310Logo.png", "StoreLogo.png")) {
    $src = Join-Path $repo "apps\desktop\src-tauri\icons\$n"
    if (Test-Path $src) { Copy-Item $src (Join-Path $assets $n) -Force }
}
foreach ($n in @("Square44x44Logo.png", "Square150x150Logo.png", "Square310x310Logo.png", "Square71x71Logo.png", "StoreLogo.png", "Wide310x150Logo.png")) {
    if (-not (Test-Path (Join-Path $assets $n))) { Fail "visuel manquant : Assets\$n" }
}

# --- Manifeste.
Step "manifeste : $IdentityName $Version, éditeur $Publisher, fiche « $DisplayName »"
$manifest = Get-Content (Join-Path $msixDir "AppxManifest.xml") -Raw -Encoding UTF8
$manifest = $manifest.Replace("@IDENTITY_NAME@", $IdentityName).Replace("@PUBLISHER@", $Publisher).Replace("@VERSION@", $Version).Replace("@PUBLISHER_DISPLAY@", $PublisherDisplayName).Replace("@DISPLAY_NAME@", $DisplayName)
[IO.File]::WriteAllText((Join-Path $layout "AppxManifest.xml"), $manifest, (New-Object Text.UTF8Encoding($false)))

# --- Index des ressources (resources.pri), exigé par la certification du Store.
Step "index des ressources"
$priconfig = Join-Path $env:TEMP "solon-priconfig.xml"
& $makepri createconfig /cf $priconfig /dq "en-US_fr-FR" /o | Out-Null
if ($LASTEXITCODE -ne 0) { Fail "makepri createconfig a échoué" }
& $makepri new /pr $layout /cf $priconfig /of (Join-Path $layout "resources.pri") /o | Out-Null
if ($LASTEXITCODE -ne 0) { Fail "makepri new a échoué" }

# --- Empaquetage.
if (-not $Out) {
    $outDir = Join-Path $repo "target\release\bundle\msix"
    New-Item -ItemType Directory -Force $outDir | Out-Null
    $Out = Join-Path $outDir "Solon_${Version}_x64.msix"
}
if (Test-Path $Out) { Remove-Item -Force $Out }
Step "empaquetage"
& $makeappx pack /d $layout /p $Out /o | Out-Null
if ($LASTEXITCODE -ne 0) { Fail "makeappx pack a échoué" }

$sizeMb = [math]::Round((Get-Item $Out).Length / 1MB, 1)
Write-Host "paquet : $Out ($sizeMb Mo)" -ForegroundColor Green

# --- Signature.
if ($Store) {
    Write-Host ""
    Write-Host "Paquet non signé, c'est voulu : le Store le signe lui-même avec le certificat de Microsoft." -ForegroundColor Yellow
    Write-Host "Téléversez-le dans Partner Center (Soumissions -> Paquets)." -ForegroundColor Yellow
    exit 0
}

# Certificat local, pour essayer le paquet sur cette machine. Il ne vaut que pour ce PC.
Step "signature avec un certificat de développement"
$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $Publisher -and $_.NotAfter -gt (Get-Date) } | Select-Object -First 1
if (-not $cert) {
    Write-Host "création d'un certificat auto-signé $Publisher (valable 3 ans)"
    $cert = New-SelfSignedCertificate -Type Custom -Subject $Publisher -KeyUsage DigitalSignature `
        -FriendlyName "Solon MSIX (développement)" -CertStoreLocation "Cert:\CurrentUser\My" `
        -NotAfter (Get-Date).AddYears(3) `
        -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
}
& $signtool sign /fd SHA256 /a /sha1 $cert.Thumbprint $Out
if ($LASTEXITCODE -ne 0) { Fail "signtool a échoué" }

$cerPath = [IO.Path]::ChangeExtension($Out, ".cer")
Export-Certificate -Cert $cert -FilePath $cerPath | Out-Null
Write-Host ""
Write-Host "Signé. Pour l'installer sur ce PC :" -ForegroundColor Green
Write-Host "  1. dans un PowerShell administrateur, faire confiance au certificat de test :"
Write-Host "     Import-Certificate -FilePath `"$cerPath`" -CertStoreLocation Cert:\LocalMachine\Root"
Write-Host "  2. installer le paquet :"
Write-Host "     Add-AppxPackage `"$Out`""
Write-Host "  Désinstaller : Get-AppxPackage *$($IdentityName.Split('.')[-1])* | Remove-AppxPackage"
