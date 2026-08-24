param(
  [string]$ProcessName = "achievement-watcher-preview",
  [int]$Samples = 12,
  [int]$IntervalSeconds = 5,
  [int]$MemoryLimitMb = 100
)

$measurements = @()

function Get-AchievementWatcherProcessTree {
  $roots = @(Get-CimInstance Win32_Process -Filter "Name = '$ProcessName.exe'")
  if ($roots.Count -eq 0) { throw "Achievement Watcher process '$ProcessName' was not found" }
  $all = @(Get-CimInstance Win32_Process)
  $ids = [System.Collections.Generic.HashSet[uint32]]::new()
  foreach ($root in $roots) { [void]$ids.Add([uint32]$root.ProcessId) }
  do {
    $added = $false
    foreach ($process in $all) {
      if ($ids.Contains([uint32]$process.ParentProcessId) -and $ids.Add([uint32]$process.ProcessId)) { $added = $true }
    }
  } while ($added)
  return @(Get-Process -Id @($ids) -ErrorAction SilentlyContinue)
}

for ($sample = 0; $sample -lt $Samples; $sample++) {
  $processes = Get-AchievementWatcherProcessTree
  $memory = ($processes | Measure-Object WorkingSet64 -Sum).Sum / 1MB
  $cpuBefore = ($processes | Measure-Object CPU -Sum).Sum
  Start-Sleep -Seconds $IntervalSeconds
  $processesAfter = Get-AchievementWatcherProcessTree
  $cpuAfter = ($processesAfter | Measure-Object CPU -Sum).Sum
  $cpuPercent = (($cpuAfter - $cpuBefore) / $IntervalSeconds) * 100
  $measurements += [pscustomobject]@{ MemoryMb = $memory; CpuPercent = $cpuPercent }
}

$summary = $measurements | Measure-Object MemoryMb, CpuPercent -Average -Maximum
$measurements | Format-Table -AutoSize
$peakMemory = ($measurements | Measure-Object MemoryMb -Maximum).Maximum
$averageCpu = ($measurements | Measure-Object CpuPercent -Average).Average
Write-Host ("Peak working set: {0:N1} MB; average CPU: {1:N2}%" -f $peakMemory, $averageCpu)
if ($peakMemory -gt $MemoryLimitMb) {
  throw "Background memory budget exceeded: $peakMemory MB > $MemoryLimitMb MB"
}
