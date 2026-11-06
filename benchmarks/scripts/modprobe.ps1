param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$Database = "C:%TEMP%\quire-floor-2026-11-06\modprobe.db",
    [int]$Width = 1600,
    [int]$Height = 900,
    [int]$WarmupSeconds = 2,
    [int]$IdleSeconds = 6,
    [string]$Label = "modprobe"
)

$ErrorActionPreference = "Stop"

Add-Type -Namespace MP -Name P -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
[DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hWnd, int X, int Y, int W, int H, bool repaint);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RC rect);
[StructLayout(LayoutKind.Sequential)] public struct RC { public int L; public int T; public int R; public int B; }
'@

$probeDir = Split-Path -Parent $Database
if (-not (Test-Path -LiteralPath $probeDir)) { New-Item -ItemType Directory -Path $probeDir | Out-Null }
foreach ($stale in @($Database, "$Database-wal", "$Database-shm")) {
    if (Test-Path -LiteralPath $stale) { Remove-Item -LiteralPath $stale -Force }
}
$exePath = (Resolve-Path -LiteralPath $Exe).Path
$item = Get-Item $exePath
Write-Host ("exe identity: {0} bytes, sha256 {1}, mtime {2}" -f $item.Length, `
    ((Get-FileHash -Algorithm SHA256 -LiteralPath $exePath).Hash.Substring(0, 12)), $item.LastWriteTime.ToString("o"))

$process = Start-Process -FilePath $exePath -ArgumentList @("--db", $Database) -PassThru
$owned = $true
try {
    $handle = [IntPtr]::Zero
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($handle -eq [IntPtr]::Zero -and $sw.Elapsed.TotalSeconds -lt 25) {
        Start-Sleep -Milliseconds 20
        $live = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
        if (-not $live) { break }
        $handle = $live.MainWindowHandle
    }
    if ($handle -eq [IntPtr]::Zero) { throw "no window appeared within 25s" }
    [void][MP.P]::ShowWindow($handle, 9)
    [void][MP.P]::MoveWindow($handle, 40, 40, $Width, $Height, $true)
    $rect = New-Object MP.P+RC
    [void][MP.P]::GetWindowRect($handle, [ref]$rect)
    Write-Host ("actual window {0}x{1}" -f ($rect.R - $rect.L), ($rect.B - $rect.T))
    Start-Sleep -Seconds $WarmupSeconds

    $peakPrivate = 0
    $peakWs = 0
    $samples = 0
    $last = $null
    $deadline = (Get-Date).AddSeconds($IdleSeconds)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 250
        $p = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
        if (-not $p) { break }
        $p.Refresh()
        $samples++
        if ($p.PrivateMemorySize64 -gt $peakPrivate) { $peakPrivate = $p.PrivateMemorySize64 }
        if ($p.WorkingSet64 -gt $peakWs) { $peakWs = $p.WorkingSet64 }
        $last = $p
    }
    if (-not $last) { throw "process died during idle sampling" }

    Write-Host ("end Private = {0:N1} MB | peak Private = {1:N1} MB | end WS = {2:N1} MB | samples = {3}" -f `
        ($last.PrivateMemorySize64 / 1MB), ($peakPrivate / 1MB), ($last.WorkingSet64 / 1MB), $samples)

    $mods = @()
    try { $mods = @($last.Modules) }
    catch { Write-Host ("module walk failed: " + $_.Exception.Message) }

    $rows = foreach ($m in ($mods | Sort-Object ModuleMemorySize -Descending)) {
        $mb = $m.ModuleMemorySize / 1MB
        $fn = $m.FileName
        $owner = "OTHER"
        if ($fn -like "$env:WINDIR*") { $owner = "windows" }
        elseif ($fn -like "C:<home>\Desktop\quire*") { $owner = "app" }
        elseif ($fn -like "C:\Program Files*" -or $fn -like "C:\Program Files (x86)*") { $owner = "programfiles" }
        elseif ($fn -like "$env:LOCALAPPDATA\Programs\Qoder*") { $owner = "qoder" }
        "{0}`t{1,9:N3}`t{2}`t{3}" -f $owner, $mb, $m.ModuleName, $fn
    }

    $rankPath = Join-Path $probeDir ($Label + "-modules.tsv")
    $rows | Set-Content -LiteralPath $rankPath -Encoding UTF8

    $classify = {
        param($fn)
        if ($fn -like "$env:WINDIR*") { "windows" }
        elseif ($fn -like "C:<home>\Desktop\quire*") { "app" }
        elseif ($fn -like "C:\Program Files*" -or $fn -like "C:\Program Files (x86)*") { "programfiles" }
        elseif ($fn -like "$env:LOCALAPPDATA\Programs\Qoder*") { "qoder" }
        else { "OTHER" }
    }

    Write-Host "=== module groups (count / mapped MB) ==="
    $tagged = foreach ($m in $mods) {
        [pscustomobject]@{ Owner = (& $classify $m.FileName); MB = ($m.ModuleMemorySize / 1MB) }
    }
    foreach ($g in ($tagged | Group-Object Owner | Sort-Object { ($_.Group | Measure-Object MB -Sum).Sum } -Descending)) {
        $sum = ($g.Group | Measure-Object MB -Sum).Sum
        Write-Host ("{0,-14} count={1,-4} mapped={2:N1} MB" -f $g.Name, $g.Count, $sum)
    }

    Write-Host "=== top 15 by mapped size ==="
    ($rows | Select-Object -First 15) | ForEach-Object {
        $c = $_ -split "`t"
        Write-Host ("{0,9} MB  {1,-13} {2}" -f $c[1], $c[2], $c[0])
    }
    Write-Host ("full list: " + $rankPath)
}
finally {
    if ($owned) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
    Write-Host "killed"
}
