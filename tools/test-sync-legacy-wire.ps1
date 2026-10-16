param(
    [string]$LegacyExe = (Join-Path $PSScriptRoot '..\..\quire-desktop\target\release\quire.exe')
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$legacyPath = (Resolve-Path $LegacyExe).Path
$fixture = Join-Path $env:TEMP ('quire-sync-legacy-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
$gpuiDb = Join-Path $fixture 'gpui\quire.db'
$legacyDb = Join-Path $fixture 'slint\quire.db'
$gpuiDir = Split-Path -Parent $gpuiDb
$legacyDir = Split-Path -Parent $legacyDb
New-Item -ItemType Directory -Path $gpuiDir, $legacyDir | Out-Null
$snapshotPath = Join-Path $fixture 'gpui-snapshot.json'
$replyPath = Join-Path $fixture 'legacy-reply.json'
$legacyProcess = $null
$port = 5878

function Invoke-GpuiProbe([string[]]$Arguments) {
    & cargo run --locked --manifest-path (Join-Path $repoRoot 'Cargo.toml') --example legacy_schema_probe -- @Arguments
    if ($LASTEXITCODE -ne 0) { throw "GPUI sync probe failed ($LASTEXITCODE): $Arguments" }
}

try {
    $occupied = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $port)
    try {
        $occupied.Start()
    } catch {
        throw "Cannot run legacy sync probe: TCP port $port is already in use"
    } finally {
        $occupied.Stop()
    }

    Invoke-GpuiProbe @('sync-seed-export', $gpuiDb, $snapshotPath)
    $source = Get-Content -LiteralPath $snapshotPath -Raw | ConvertFrom-Json

    # The legacy app uses a separate empty test database. No real library is
    # opened, and no firewall/security prompt is accepted by this test.
    $legacyProcess = Start-Process -FilePath $legacyPath -ArgumentList @(
        '--db', $legacyDb, '--auto-exit', '45'
    ) -PassThru -WindowStyle Hidden

    $ready = $false
    for ($i = 0; $i -lt 150; $i++) {
        try {
            $info = Invoke-RestMethod -Uri "http://127.0.0.1:$port/sync/info" -TimeoutSec 1
            if ($info.id) { $ready = $true; break }
        } catch {
            $legacyProcess.Refresh()
            if ($legacyProcess.HasExited) {
                throw "Slint exited before sync HTTP became ready ($($legacyProcess.ExitCode))"
            }
            Start-Sleep -Milliseconds 100
        }
    }
    if (-not $ready) { throw "Slint sync HTTP did not listen on loopback port $port" }

    $pairBody = @{
        id = $source.device_id
        name = $source.device
        kind = 'windows'
        port = $port
    } | ConvertTo-Json -Compress
    $paired = Invoke-WebRequest -Uri "http://127.0.0.1:$port/sync/pair" -Method Post `
        -ContentType 'application/json' -Body $pairBody -TimeoutSec 10
    if ($paired.StatusCode -ne 200) { throw "Slint pairing returned HTTP $($paired.StatusCode)" }

    $reply = Invoke-WebRequest -Uri "http://127.0.0.1:$port/sync/snapshot" -Method Post `
        -ContentType 'application/json' -Body (Get-Content -LiteralPath $snapshotPath -Raw) -TimeoutSec 15
    if ($reply.StatusCode -ne 200) { throw "Slint snapshot merge returned HTTP $($reply.StatusCode)" }
    $replyBody = if ($reply.Content -is [byte[]]) {
        [Text.Encoding]::UTF8.GetString($reply.Content)
    } else {
        [string]$reply.Content
    }
    [IO.File]::WriteAllText($replyPath, $replyBody, [Text.UTF8Encoding]::new($false))
    $legacySnapshot = $replyBody | ConvertFrom-Json
    if ($legacySnapshot.device_id -eq $source.device_id) {
        throw 'Slint response did not identify the legacy sync endpoint as the sender'
    }
    Invoke-GpuiProbe @('sync-verify', $gpuiDb, $replyPath)

    Write-Output 'LEGACY_SYNC_WIRE=PASS'
    Write-Output "Legacy sync info: $($info.name) ($($info.id))"
    Write-Output "Isolated fixture retained at $fixture"
} finally {
    if ($legacyProcess) {
        $legacyProcess.Refresh()
        if (-not $legacyProcess.HasExited) { Stop-Process -Id $legacyProcess.Id }
    }
}
