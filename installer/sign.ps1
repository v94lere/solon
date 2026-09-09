# Signature Authenticode d'un binaire Solon, appelée par le bundler Tauri (bundle.windows.signCommand,
# voir tauri.signed.conf.json ; construire avec `npm run tauri build -- --config src-tauri/tauri.signed.conf.json`).
# Deux modes, choisis par variables d'environnement :
#   SOLON_SIGN_THUMBPRINT   empreinte SHA-1 d'un certificat OV/EV du magasin personnel (HSM/carte accepté)
#   SOLON_SIGN_TRUSTED_*    Azure Trusted Signing : ENDPOINT, ACCOUNT, PROFILE, et SOLON_SIGN_DLIB
#                           (Azure.CodeSigning.Dlib.dll du paquet Microsoft.Trusted.Signing.Client)
# Horodatage RFC 3161 : SOLON_SIGN_TIMESTAMP (défaut http://timestamp.digicert.com).
param([Parameter(Mandatory = $true)][string]$Path)
$ErrorActionPreference = "Stop"
$ts = if ($env:SOLON_SIGN_TIMESTAMP) { $env:SOLON_SIGN_TIMESTAMP } else { "http://timestamp.digicert.com" }
$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $signtool) { throw "signtool.exe introuvable : installer le SDK Windows 10/11" }

if ($env:SOLON_SIGN_THUMBPRINT) {
    & $signtool.FullName sign /fd SHA256 /td SHA256 /tr $ts /sha1 $env:SOLON_SIGN_THUMBPRINT /d "Solon" $Path
} elseif ($env:SOLON_SIGN_TRUSTED_ENDPOINT) {
    $meta = Join-Path $env:TEMP "solon-sign-metadata.json"
    @{ Endpoint = $env:SOLON_SIGN_TRUSTED_ENDPOINT; CodeSigningAccountName = $env:SOLON_SIGN_TRUSTED_ACCOUNT; CertificateProfileName = $env:SOLON_SIGN_TRUSTED_PROFILE } | ConvertTo-Json | Set-Content -Encoding ascii $meta
    & $signtool.FullName sign /fd SHA256 /td SHA256 /tr $ts /dlib $env:SOLON_SIGN_DLIB /dmdf $meta /d "Solon" $Path
} else {
    throw "Aucun certificat configuré (SOLON_SIGN_THUMBPRINT ou SOLON_SIGN_TRUSTED_*)"
}
if ($LASTEXITCODE -ne 0) { throw "signtool a échoué ($LASTEXITCODE) sur $Path" }
