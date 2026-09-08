# Signature Authenticode d'un binaire Monodon, appelée par le bundler Tauri (bundle.windows.signCommand,
# voir tauri.signed.conf.json ; construire avec `npm run tauri build -- --config src-tauri/tauri.signed.conf.json`).
# Deux modes, choisis par variables d'environnement :
#   MONODON_SIGN_THUMBPRINT   empreinte SHA-1 d'un certificat OV/EV du magasin personnel (HSM/carte accepté)
#   MONODON_SIGN_TRUSTED_*    Azure Trusted Signing : ENDPOINT, ACCOUNT, PROFILE, et MONODON_SIGN_DLIB
#                           (Azure.CodeSigning.Dlib.dll du paquet Microsoft.Trusted.Signing.Client)
# Horodatage RFC 3161 : MONODON_SIGN_TIMESTAMP (défaut http://timestamp.digicert.com).
param([Parameter(Mandatory = $true)][string]$Path)
$ErrorActionPreference = "Stop"
$ts = if ($env:MONODON_SIGN_TIMESTAMP) { $env:MONODON_SIGN_TIMESTAMP } else { "http://timestamp.digicert.com" }
$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $signtool) { throw "signtool.exe introuvable : installer le SDK Windows 10/11" }

if ($env:MONODON_SIGN_THUMBPRINT) {
    & $signtool.FullName sign /fd SHA256 /td SHA256 /tr $ts /sha1 $env:MONODON_SIGN_THUMBPRINT /d "Monodon" $Path
} elseif ($env:MONODON_SIGN_TRUSTED_ENDPOINT) {
    $meta = Join-Path $env:TEMP "monodon-sign-metadata.json"
    @{ Endpoint = $env:MONODON_SIGN_TRUSTED_ENDPOINT; CodeSigningAccountName = $env:MONODON_SIGN_TRUSTED_ACCOUNT; CertificateProfileName = $env:MONODON_SIGN_TRUSTED_PROFILE } | ConvertTo-Json | Set-Content -Encoding ascii $meta
    & $signtool.FullName sign /fd SHA256 /td SHA256 /tr $ts /dlib $env:MONODON_SIGN_DLIB /dmdf $meta /d "Monodon" $Path
} else {
    throw "Aucun certificat configuré (MONODON_SIGN_THUMBPRINT ou MONODON_SIGN_TRUSTED_*)"
}
if ($LASTEXITCODE -ne 0) { throw "signtool a échoué ($LASTEXITCODE) sur $Path" }
