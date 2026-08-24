$ErrorActionPreference = 'Stop'
$buildRoot = Join-Path $env:LOCALAPPDATA 'AchievementWatcherBuild'
if (Test-Path -LiteralPath $buildRoot) {
  Remove-Item -LiteralPath $buildRoot -Recurse -Force
  Write-Host "Removed $buildRoot"
} else {
  Write-Host 'The local Achievement Watcher build cache is already empty.'
}
