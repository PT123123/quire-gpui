param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Database,
    [string]$Label = "run",
    [int]$IdleSeconds = 8,
    [int]$WarmupSeconds = 2,
    [string[]]$ExtraArgs = @(),
    [int]$WindowWidth = 1600,
    [int]$WindowHeight = 900
)

$ErrorActionPreference = "Stop"
$exePath = (Resolve-Path -LiteralPath $Exe).Path
$dbPath = [IO.Path]::GetFullPath($Database)
$arguments = @("--db", $dbPath) + $ExtraArgs
$nativeWindow = @"
using System;
using System.Runtime.InteropServices;
public static class QuirePerfWindow {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int command);
    [DllImport("user32.dll", SetLastError=true)] public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int width, int height, bool repaint);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out Rect rect);
}
"@
Add-Type -TypeDefinition $nativeWindow
$started = [DateTime]::UtcNow
$watch = [Diagnostics.Stopwatch]::StartNew()
$process = Start-Process -FilePath $exePath -ArgumentList $arguments -PassThru
$process.EnableRaisingEvents = $true

$handle = [IntPtr]::Zero
while ($handle -eq [IntPtr]::Zero -and $watch.Elapsed.TotalSeconds -lt 20) {
    Start-Sleep -Milliseconds 20
    $live = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
    if (-not $live) { break }
    $handle = $live.MainWindowHandle
}
$startupMs = [math]::Round($watch.Elapsed.TotalMilliseconds, 1)
if ($handle -eq [IntPtr]::Zero) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    throw "Main window did not appear within 20 seconds; see app logs and check the database pointer."
}
[void][QuirePerfWindow]::ShowWindow($handle, 9)
if (-not [QuirePerfWindow]::MoveWindow($handle, 40, 40, $WindowWidth, $WindowHeight, $true)) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    throw "Could not set the window to ${WindowWidth}x${WindowHeight}."
}
$rect = [QuirePerfWindow+Rect]::new()
[void][QuirePerfWindow]::GetWindowRect($handle, [ref]$rect)
$actualWindowWidth = $rect.Right - $rect.Left
$actualWindowHeight = $rect.Bottom - $rect.Top

Start-Sleep -Seconds $WarmupSeconds
$live = Get-Process -Id $process.Id
$cpuStart = $live.TotalProcessorTime.TotalMilliseconds
$sampleStart = [Diagnostics.Stopwatch]::StartNew()
$peakWorkingSet = [long]$live.WorkingSet64
$peakPrivate = [long]$live.PrivateMemorySize64
$samples = 0
while ($sampleStart.Elapsed.TotalSeconds -lt $IdleSeconds) {
    Start-Sleep -Milliseconds 250
    $live = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
    if (-not $live) { break }
    if ($live.WorkingSet64 -gt $peakWorkingSet) { $peakWorkingSet = [long]$live.WorkingSet64 }
    if ($live.PrivateMemorySize64 -gt $peakPrivate) { $peakPrivate = [long]$live.PrivateMemorySize64 }
    $samples++
}
$sampleSeconds = [math]::Max(0.001, $sampleStart.Elapsed.TotalSeconds)
$cpuEnd = $live.TotalProcessorTime.TotalMilliseconds
$cpuPct = [math]::Round((($cpuEnd - $cpuStart) / ($sampleSeconds * 1000)) * 100, 2)
$endWorkingSet = [long]$live.WorkingSet64
$endPrivate = [long]$live.PrivateMemorySize64

# The app normally hides to tray on WM_CLOSE. This is an isolated measurement
# process and has no pending edits, so end it after sampling rather than alter
# the user's tray/session state.
Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue

[ordered]@{
    label = $Label
    exe = $exePath
    database = "<isolated temp database>"
    started_at_utc = $started.ToString("o")
    window_handle_up_ms = $startupMs
    window_width = $actualWindowWidth
    window_height = $actualWindowHeight
    warmup_seconds = $WarmupSeconds
    idle_sample_seconds = [math]::Round($sampleSeconds, 2)
    idle_cpu_percent_one_core = $cpuPct
    end_working_set_mb = [math]::Round($endWorkingSet / 1MB, 1)
    peak_working_set_mb = [math]::Round($peakWorkingSet / 1MB, 1)
    end_private_mb = [math]::Round($endPrivate / 1MB, 1)
    peak_private_mb = [math]::Round($peakPrivate / 1MB, 1)
    memory_sample_interval_ms = 250
    memory_sample_count = $samples
    process_exit_method = "forced after idle sample; isolated DB, no edits"
} | ConvertTo-Json -Depth 3
