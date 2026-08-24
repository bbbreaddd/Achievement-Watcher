param([string]$CertificatePath)

$package = Get-AppxPackage -Name "AchievementWatcher.GameBarCompanion"
if ($package) { Remove-AppxPackage -Package $package.PackageFullName }
if ($CertificatePath) {
  $certificate = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($CertificatePath)
  $store = New-Object System.Security.Cryptography.X509Certificates.X509Store("TrustedPeople", "CurrentUser")
  $store.Open("ReadWrite")
  try {
    $matches = $store.Certificates.Find("FindByThumbprint", $certificate.Thumbprint, $false)
    foreach ($match in $matches) { $store.Remove($match) }
  } finally { $store.Close() }
}
Write-Host "Removed Achievement Watcher Game Bar Companion. The desktop application was not changed."
