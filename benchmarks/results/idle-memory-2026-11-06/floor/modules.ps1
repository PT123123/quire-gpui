param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Database,
    [int]$Width = 1600,
    [int]$Height = 900,
    [int]$SettleSeconds = 12,
    [string]$Tag = "run"
)

$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class QW3 {
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int ht, bool r);
}
'@ -Language CSharp

$dbPath = [IO.Path]::GetFullPath($Database)
if (Test-Path -LiteralPath $dbPath) { Remove-Item -LiteralPath $dbPath -Force }
$exePath = (Resolve-Path -LiteralPath $Exe).Path
$p = Start-Process -FilePath $exePath -ArgumentList @("--db", $dbPath) -PassThru

$h = [IntPtr]::Zero
$sw = [Diagnostics.Stopwatch]::StartNew()
while ($h -eq [IntPtr]::Zero -and $sw.Elapsed.TotalSeconds -lt 25) {
    Start-Sleep -Milliseconds 20
    $live = Get-Process -Id $p.Id -ErrorAction SilentlyContinue
    if (-not $live) { break }
    $h = $live.MainWindowHandle
}
if ($h -eq [IntPtr]::Zero) { Stop-Process -Id $p.Id -Force; throw "no window" }
[void][QW3]::ShowWindow($h, 9)
[void][QW3]::MoveWindow($h, 40, 40, $Width, $Height, $true)
Start-Sleep -Seconds $SettleSeconds

$p = Get-Process -Id $p.Id
Write-Host ("=== $Tag  ${Width}x${Height} ===")
Write-Host ("WS={0:F1} PeakWS={1:F1} Private={2:F1} PeakPrivate={3:F1} VM={4:F1} (MB)" -f `
    ($p.WorkingSet64/1MB), ($p.PeakWorkingSet64/1MB), ($p.PrivateMemorySize64/1MB),
    ($p.PeakPagefileUsage/1MB), ($p.VirtualMemorySize64/1MB))

$mods = @($p.Modules)
$img = ($mods | Measure-Object -Property ModuleMemorySize -Sum).Sum
Write-Host ("modules=" + $mods.Count + " total image bytes=" + [math]::Round($img/1MB,1) + " MB")
Write-Host "top images:"
$mods | Sort-Object ModuleMemorySize -Descending | Select-Object -First 14 | ForEach-Object {
    Write-Host ("  {0,7:F2} MB  {1}" -f ($_.ModuleMemorySize/1MB), $_.ModuleName)
}

$graphic = ($mods | Where-Object { $_.ModuleName -match "^(igd|igc|igdgmm|d3d11|dxgi|dcomp|directml|nvcu|nvoglv|atiumd|D3D12|dxcore)" } |
    Measure-Object -Property ModuleMemorySize -Sum).Sum
Write-Host ("graphics-stack images = " + [math]::Round($graphic/1MB,1) + " MB")
$sys = ($mods | Where-Object { $_.ModuleName -match "^(ntdll|kernel32|kernelbase|ucrtbase|combase|rpcrt4|shell32|ole32|oleaut32|windows\.storage|user32|gdi32|advapi32|msvcrt|vcruntime|bcrypt|sech|wintypes|clbcatq|msvcp|VCRUNTIME|IMM32|MSCTF|UxTheme|dwrite|d2d1)" } |
    Measure-Object -Property ModuleMemorySize -Sum).Sum
Write-Host ("windows-core images = " + [math]::Round($sys/1MB,1) + " MB")
Write-Host ("self image = " + [math]::Round(($mods | Where-Object { $_.FileName -eq $exePath } | Measure-Object -Property ModuleMemorySize -Sum).Sum/1MB,1) + " MB")

Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
