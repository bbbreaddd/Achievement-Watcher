param(
  [switch]$CheckOnly,
  [switch]$Release
)

$ErrorActionPreference = 'Stop'
$buildRoot = Join-Path $env:LOCALAPPDATA 'AchievementWatcherBuild'
$sourceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$assetSourceRoot = (Resolve-Path (Join-Path $sourceRoot '..\app\Source')).Path
$resourceSourceRoot = (Resolve-Path (Join-Path $sourceRoot '..\app\resources')).Path
$mediaSourceRoot = (Resolve-Path (Join-Path $sourceRoot '..\app\media')).Path
$localeSourceRoot = (Resolve-Path (Join-Path $sourceRoot '..\app\locale')).Path
$installerSourceRoot = (Resolve-Path (Join-Path $sourceRoot '..\app\build')).Path
$presetSourceRoot = (Resolve-Path (Join-Path $sourceRoot '..\app\presets')).Path
$stageRoot = Join-Path $buildRoot 'source'
$assetStageRoot = Join-Path $buildRoot 'app\Source'
$resourceStageRoot = Join-Path $buildRoot 'app\resources'
$mediaStageRoot = Join-Path $buildRoot 'app\media'
$localeStageRoot = Join-Path $buildRoot 'app\locale'
$installerStageRoot = Join-Path $buildRoot 'app\build'
$presetStageRoot = Join-Path $buildRoot 'app\presets'
$env:CARGO_TARGET_DIR = Join-Path $buildRoot 'target'
New-Item -ItemType Directory -Force -Path $env:CARGO_TARGET_DIR,$stageRoot,$assetStageRoot,$resourceStageRoot,$mediaStageRoot,$localeStageRoot,$installerStageRoot,$presetStageRoot | Out-Null

function Sync-Source {
  & robocopy $sourceRoot $stageRoot /MIR /XD node_modules target dist bin obj /XF '*.pdb' /NJH /NJS /NDL /NFL /NP | Out-Null
  if ($LASTEXITCODE -ge 8) { throw "Source staging failed with robocopy exit code $LASTEXITCODE" }
  & robocopy $assetSourceRoot $assetStageRoot *.svg /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
  if ($LASTEXITCODE -ge 8) { throw "Source icon staging failed with robocopy exit code $LASTEXITCODE" }
  & robocopy $resourceSourceRoot $resourceStageRoot /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
  if ($LASTEXITCODE -ge 8) { throw "Original UI resource staging failed with robocopy exit code $LASTEXITCODE" }
  & robocopy $mediaSourceRoot $mediaStageRoot *.wav /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
  if ($LASTEXITCODE -ge 8) { throw "Notification sound staging failed with robocopy exit code $LASTEXITCODE" }
  & robocopy $localeSourceRoot $localeStageRoot *.json /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
  if ($LASTEXITCODE -ge 8) { throw "Locale staging failed with robocopy exit code $LASTEXITCODE" }
  & robocopy $installerSourceRoot $installerStageRoot *.bmp /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
  if ($LASTEXITCODE -ge 8) { throw "Installer artwork staging failed with robocopy exit code $LASTEXITCODE" }
  & robocopy $presetSourceRoot $presetStageRoot /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
  if ($LASTEXITCODE -ge 8) { throw "Notification preset staging failed with robocopy exit code $LASTEXITCODE" }
}

Sync-Source

$sccache = Get-Command sccache -ErrorAction SilentlyContinue
if ($sccache) {
  $env:RUSTC_WRAPPER = $sccache.Source
}

Write-Host "Cargo artifacts: $env:CARGO_TARGET_DIR"
Write-Host "Development source: $stageRoot"
if ($sccache) { Write-Host 'Compiler cache: sccache' }

Push-Location $stageRoot
try {
  $lockHash = (Get-FileHash package-lock.json -Algorithm SHA256).Hash
  $hashFile = Join-Path $stageRoot 'node_modules\.achievement-watcher-lock-hash'
  $installedHash = if (Test-Path $hashFile) { Get-Content $hashFile -Raw } else { '' }
  if ($installedHash.Trim() -ne $lockHash) {
    npm ci
    New-Item -ItemType Directory -Force -Path (Split-Path $hashFile) | Out-Null
    Set-Content -LiteralPath $hashFile -Value $lockHash -NoNewline
  }

  if ($CheckOnly) {
    npm run check
    & cargo check -p achievement-watcher-preview
    exit $LASTEXITCODE
  }

  if ($Release) {
    npm run tauri build
    Write-Host "Installer output: $env:CARGO_TARGET_DIR\release\bundle\nsis"
    exit $LASTEXITCODE
  }

  $debugExecutable = Join-Path $env:CARGO_TARGET_DIR 'debug\achievement-watcher-preview.exe'
  $lockedProcesses = @(Get-CimInstance Win32_Process -Filter "Name = 'achievement-watcher-preview.exe'" |
    Where-Object { $_.ExecutablePath -eq $debugExecutable })
  if ($lockedProcesses.Count -gt 0) {
    $processIds = ($lockedProcesses.ProcessId -join ', ')
    throw "The development copy of Achievement Watcher is already running (PID: $processIds). Close it or run Stop-Process -Id $processIds, then start this script again."
  }

  $syncJob = Start-Job -ArgumentList $sourceRoot,$stageRoot,$assetSourceRoot,$assetStageRoot,$resourceSourceRoot,$resourceStageRoot,$mediaSourceRoot,$mediaStageRoot,$localeSourceRoot,$localeStageRoot,$installerSourceRoot,$installerStageRoot,$presetSourceRoot,$presetStageRoot -ScriptBlock {
    param($source, $stage, $assetSource, $assetStage, $resourceSource, $resourceStage, $mediaSource, $mediaStage, $localeSource, $localeStage, $installerSource, $installerStage, $presetSource, $presetStage)
    while ($true) {
      & robocopy $source $stage /MIR /XD node_modules target dist bin obj /XF '*.pdb' /NJH /NJS /NDL /NFL /NP | Out-Null
      & robocopy $assetSource $assetStage *.svg /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
      & robocopy $resourceSource $resourceStage /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
      & robocopy $mediaSource $mediaStage *.wav /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
      & robocopy $localeSource $localeStage *.json /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
      & robocopy $installerSource $installerStage *.bmp /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
      & robocopy $presetSource $presetStage /MIR /NJH /NJS /NDL /NFL /NP | Out-Null
      Start-Sleep -Milliseconds 750
    }
  }
  try {
    npm run tauri dev
  } finally {
    Stop-Job $syncJob -ErrorAction SilentlyContinue
    Remove-Job $syncJob -Force -ErrorAction SilentlyContinue
  }
} finally {
  Pop-Location
}
