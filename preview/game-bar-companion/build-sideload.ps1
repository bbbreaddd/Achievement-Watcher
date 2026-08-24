param(
  [string]$Configuration = "Release",
  [string]$CertificatePassword = $env:AW_GAMEBAR_CERT_PASSWORD
)

$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$certificatePath = Join-Path $projectRoot "AchievementWatcher.GameBar_TemporaryKey.pfx"
$certificatePublicPath = Join-Path $projectRoot "AchievementWatcher.GameBar.cer"

if (-not $CertificatePassword) {
  $secureInput = Read-Host "Password for the local signing key" -AsSecureString
  $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secureInput)
  try { $CertificatePassword = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer) }
  finally { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer) }
}
$password = ConvertTo-SecureString -String $CertificatePassword -Force -AsPlainText

if (-not (Test-Path $certificatePath)) {
  $certificate = New-SelfSignedCertificate -Type Custom -Subject "CN=Achievement Watcher" -KeyUsage DigitalSignature -FriendlyName "Achievement Watcher Game Bar Companion" -CertStoreLocation "Cert:\CurrentUser\My" -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
  Export-PfxCertificate -Cert $certificate -FilePath $certificatePath -Password $password | Out-Null
  Export-Certificate -Cert $certificate -FilePath $certificatePublicPath | Out-Null
}

msbuild (Join-Path $projectRoot "AchievementWatcher.GameBar.csproj") /restore /p:Configuration=$Configuration /p:Platform=x64 /p:AppxPackageSigningEnabled=true "/p:PackageCertificatePassword=$CertificatePassword"
if ($LASTEXITCODE -ne 0) { throw "Game Bar companion build failed" }
