param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$Scratch = "C:%TEMP%\quire-floor-2026-11-06"
)

$ErrorActionPreference = "Stop"
$harness = "C:<repo>\tools\measure-runtime.ps1"

$arms = @(
    @{ Name = "dcomp-on";  Env = @{} },
    @{ Name = "dcomp-off"; Env = @{ "GPUI_DISABLE_DIRECT_COMPOSITION" = "1" } },
    @{ Name = "dcomp-on-control"; Env = @{} }
)

foreach ($a in $arms) {
    Get-ChildItem env: | Where-Object { $_.Name -like "GPUI_*" } | ForEach-Object { Remove-Item "env:$($_.Name)" }
    foreach ($k in $a.Env.Keys) { Set-Item -Path ("env:" + $k) -Value $a.Env[$k] }
    $db = Join-Path $Scratch ("dcomp-" + $a.Name + ".db")
    if (Test-Path -LiteralPath $db) { Remove-Item -LiteralPath $db -Force }
    $raw = & $harness -Exe $Exe -Database $db -Label $a.Name -WindowWidth 1600 -WindowHeight 900
    $r = $raw | ConvertFrom-Json
    Write-Host ("{0,-18} endPriv={1,-6} endWS={2,-6} peakPriv={3,-6} peakWS={4,-6} cpu={5}" -f `
        $a.Name, $r.end_private_mb, $r.end_working_set_mb, $r.peak_private_mb, $r.peak_working_set_mb, $r.idle_cpu_percent_one_core)
    $raw | Set-Content -LiteralPath (Join-Path $Scratch ("dcomp-" + $a.Name + ".json")) -Encoding UTF8
}
