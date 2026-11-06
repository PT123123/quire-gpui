param(
    [string]$Exe,
    [string]$OutDir
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$harness = "C:<repo>\tools\measure-runtime.ps1"

$sizes = @(
    @{ W = 1600; H = 900; Tag = "a1600x900" },
    @{ W = 1600; H = 900; Tag = "b1600x900" },
    @{ W = 1280; H = 800; Tag = "c1280x800" },
    @{ W = 800;  H = 600; Tag = "d0800x600" },
    @{ W = 400;  H = 300; Tag = "e0400x300" }
)

$rows = @()
foreach ($s in $sizes) {
    $db = Join-Path $OutDir ("blank-" + $s.Tag + ".db")
    if (Test-Path -LiteralPath $db) { Remove-Item -LiteralPath $db -Force }
    Write-Host ("== arm " + $s.Tag + " ==")
    $r = & $harness -Exe $Exe -Database $db -Label ("blank-" + $s.Tag) `
        -WindowWidth $s.W -WindowHeight $s.H
    $r | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutDir ($s.Tag + ".json")) -Encoding UTF8
    $px = $s.W * $s.H
    $gpuPerBufferMb = [math]::Round($px * 4 / 1MB, 2)
    $rows += [pscustomobject]@{
        tag        = $s.Tag
        window     = $s.W.ToString() + "x" + $s.H
        megapixels = [math]::Round($px / 1e6, 2)
        oneBufMb   = $gpuPerBufferMb
        endPriv    = $r.end_private_mb
        endWS      = $r.end_working_set_mb
        peakPriv   = $r.peak_private_mb
        cpu        = $r.idle_cpu_percent_one_core
    }
    Write-Host ("   endPriv=" + $r.end_private_mb + " endWS=" + $r.end_working_set_mb +
        " peakPriv=" + $r.peak_private_mb + " cpu=" + $r.idle_cpu_percent_one_core)
}

$rows | Format-Table -AutoSize | Out-String -Width 200 | Write-Host
$rows | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $OutDir "sweep.json") -Encoding UTF8

Write-Host "== GPU adapter / process memory counters (best effort) =="
try {
    Get-Counter '\GPU Adapter Memory(*)\Dedicated Usage', '\GPU Process Memory(*)\Dedicated Usage',
        '\GPU Process Memory(*)\Shared Usage' -ErrorAction Stop |
        ForEach-Object { $_.CounterSamples } |
        ForEach-Object { [pscustomobject]@{ Path = $_.Path; Mb = [math]::Round($_.CookedValue / 1MB, 1) } } |
        Format-Table -AutoSize | Out-String -Width 240 | Write-Host
} catch {
    Write-Host ("   GPU counters unavailable: " + $_.Exception.Message)
}
