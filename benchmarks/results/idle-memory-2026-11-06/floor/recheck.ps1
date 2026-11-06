param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$Scratch = "C:%TEMP%\quire-floor-2026-11-06"
)

$ErrorActionPreference = "Stop"
$harness = "C:<repo>\tools\measure-runtime.ps1"

$arms = @(
    @{ W = 1600; H = 900; Name = "recheck-1600x900-a" },
    @{ W = 924;  H = 591; Name = "recheck-924x591-a" },
    @{ W = 1600; H = 900; Name = "recheck-1600x900-b" },
    @{ W = 924;  H = 591; Name = "recheck-924x591-b" }
)

foreach ($a in $arms) {
    $db = Join-Path $Scratch ($a.Name + ".db")
    if (Test-Path -LiteralPath $db) { Remove-Item -LiteralPath $db -Force }
    $raw = & $harness -Exe $Exe -Database $db -Label $a.Name -WindowWidth $a.W -WindowHeight $a.H
    $r = $raw | ConvertFrom-Json
    Write-Host ("{0,-22} actual={1}x{2} endPriv={3,-6} endWS={4,-6} peakPriv={5,-6}" -f `
        $a.Name, $r.window_width, $r.window_height, $r.end_private_mb, $r.end_working_set_mb, $r.peak_private_mb)
    $raw | Set-Content -LiteralPath (Join-Path $Scratch ($a.Name + ".json")) -Encoding UTF8
}
