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
$gpuiAttachmentDb = Join-Path $fixture 'gpui-attachment\quire.db'
$legacyAttachmentDb = Join-Path $fixture 'slint-attachment\quire.db'
$gpuiPushDb = Join-Path $fixture 'gpui-push\quire.db'
$legacyPushDb = Join-Path $fixture 'slint-push\quire.db'
$gpuiDir = Split-Path -Parent $gpuiDb
$legacyDir = Split-Path -Parent $legacyDb
$gpuiAttachmentDir = Split-Path -Parent $gpuiAttachmentDb
$legacyAttachmentDir = Split-Path -Parent $legacyAttachmentDb
$gpuiPushDir = Split-Path -Parent $gpuiPushDb
$legacyPushDir = Split-Path -Parent $legacyPushDb
New-Item -ItemType Directory -Path $gpuiDir, $legacyDir, $gpuiAttachmentDir, $legacyAttachmentDir, $gpuiPushDir, $legacyPushDir | Out-Null
$snapshotPath = Join-Path $fixture 'gpui-snapshot.json'
$replyPath = Join-Path $fixture 'legacy-reply.json'
$legacyProcess = $null
$gpuiServerProcess = $null
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

    # First verify attachment transfer in the pull direction. The isolated
    # sender library is seeded with two arbitrary binary files, then Slint's
    # real sync service exposes its snapshot and attachment endpoints.
    Invoke-GpuiProbe @('write', $legacyAttachmentDb)
    $legacyProcess = Start-Process -FilePath $legacyPath -ArgumentList @(
        '--db', $legacyAttachmentDb, '--auto-exit', '20'
    ) -PassThru -WindowStyle Hidden
    $ready = $false
    for ($i = 0; $i -lt 150; $i++) {
        try {
            $info = Invoke-RestMethod -Uri "http://127.0.0.1:$port/sync/info" -TimeoutSec 1
            if ($info.id) { $ready = $true; break }
        } catch {
            $legacyProcess.Refresh()
            if ($legacyProcess.HasExited) {
                throw "Slint attachment source exited before sync HTTP became ready ($($legacyProcess.ExitCode))"
            }
            Start-Sleep -Milliseconds 100
        }
    }
    if (-not $ready) { throw "Slint attachment source did not listen on loopback port $port" }
    Invoke-GpuiProbe @('sync-pull', $gpuiAttachmentDb, "http://127.0.0.1:$port")
    Wait-Process -Id $legacyProcess.Id -Timeout 25
    $legacyProcess.Refresh()
    if (-not $legacyProcess.HasExited -or $legacyProcess.ExitCode -ne 0) {
        throw 'Slint attachment source did not shut down cleanly'
    }
    $legacyProcess = $null

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
    # The POST/response above completes the inbound probe. Stop that Slint
    # instance before the outbound probe, since both instances bind the same
    # fixed LAN sync port even though they use different fixture databases.
    $legacyProcess.Refresh()
    if (-not $legacyProcess.HasExited) {
        Stop-Process -Id $legacyProcess.Id
        [void]$legacyProcess.WaitForExit(5000)
    }
    $legacyProcess = $null
    Start-Sleep -Milliseconds 500

    # Then exercise the reverse direction through the actual sync worker in
    # Slint. Its isolated database is pre-paired with a GPUI HTTP sync server;
    # the Slint app's normal auto-sync loop must fetch the snapshot and binary
    # attachment bytes and persist them into the receiver library.
    Invoke-GpuiProbe @('write', $gpuiPushDb)
    # Let Slint create its own empty-library schema first. GPUI then only adds
    # sync settings to that fixture, which avoids making Slint start from a DB
    # bootstrapped by the other frontend.
    $legacyProcess = Start-Process -FilePath $legacyPath -ArgumentList @(
        '--db', $legacyPushDb, '--auto-exit', '5'
    ) -PassThru -WindowStyle Hidden
    Wait-Process -Id $legacyProcess.Id -Timeout 12
    $legacyProcess.Refresh()
    if (-not $legacyProcess.HasExited -or $legacyProcess.ExitCode -ne 0) {
        throw 'Slint could not initialize its isolated push receiver database'
    }
    $legacyProcess = $null

    & cargo build --locked --manifest-path (Join-Path $repoRoot 'Cargo.toml') --example legacy_schema_probe
    if ($LASTEXITCODE -ne 0) { throw "Could not build GPUI sync fixture server ($LASTEXITCODE)" }
    $probeExe = Join-Path $repoRoot 'target\debug\examples\legacy_schema_probe.exe'
    $stopFile = Join-Path $fixture 'stop-gpui-server'
    $serverLog = Join-Path $fixture 'gpui-server.log'
    $serverErrorLog = Join-Path $fixture 'gpui-server-error.log'
    $legacyPushLog = Join-Path $fixture 'slint-push.log'
    $legacyPushErrorLog = Join-Path $fixture 'slint-push-error.log'
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $pushPort = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    $listener.Stop()
    $gpuiServerProcess = Start-Process -FilePath $probeExe -ArgumentList @(
        'sync-serve-push', $gpuiPushDb, $legacyPushDb, $pushPort, $stopFile
    ) -PassThru -WindowStyle Hidden -RedirectStandardOutput $serverLog -RedirectStandardError $serverErrorLog

    $ready = $false
    for ($i = 0; $i -lt 150; $i++) {
        try {
            $pushInfo = Invoke-RestMethod -Uri "http://127.0.0.1:$pushPort/sync/info" -TimeoutSec 1
            if ($pushInfo.id) { $ready = $true; break }
        } catch {
            $gpuiServerProcess.Refresh()
            if ($gpuiServerProcess.HasExited) {
                throw "GPUI fixture sync server exited before serving requests ($($gpuiServerProcess.ExitCode)): $(Get-Content -LiteralPath $serverErrorLog -Raw)"
            }
            Start-Sleep -Milliseconds 100
        }
    }
    if (-not $ready) { throw "GPUI fixture sync server did not listen on port $pushPort" }

    $legacyProcess = Start-Process -FilePath $legacyPath -ArgumentList @(
        '--db', $legacyPushDb, '--auto-exit', '60'
    ) -PassThru -WindowStyle Hidden -RedirectStandardOutput $legacyPushLog -RedirectStandardError $legacyPushErrorLog
    $pushSynced = $false
    for ($i = 0; $i -lt 90; $i++) {
        $legacyProcess.Refresh()
        $serverOutput = if (Test-Path $serverLog) { Get-Content -LiteralPath $serverLog -Raw } else { '' }
        if ($serverOutput -match 'SYNC_PUSH_JOB=AttachmentBytes id=1' -and
            $serverOutput -match 'SYNC_PUSH_JOB=AttachmentBytes id=2' -and
            $serverOutput -match 'SYNC_PUSH_JOB=ApplyRemote') {
            $pushSynced = $true
            break
        }
        if ($legacyProcess.HasExited) { break }
        Start-Sleep -Milliseconds 500
    }
    if (-not $pushSynced) {
        $legacyProcess.Refresh()
        throw "Slint did not pull both GPUI attachments and post the merge. GPUI stdout: $(Get-Content -LiteralPath $serverLog -Raw); GPUI stderr: $(Get-Content -LiteralPath $serverErrorLog -Raw); Slint stdout: $(Get-Content -LiteralPath $legacyPushLog -Raw); Slint stderr: $(Get-Content -LiteralPath $legacyPushErrorLog -Raw)"
    }
    $legacyProcess.Refresh()
    if (-not $legacyProcess.HasExited) { Stop-Process -Id $legacyProcess.Id }
    $legacyProcess = $null
    [IO.File]::WriteAllText($stopFile, 'stop')
    Wait-Process -Id $gpuiServerProcess.Id -Timeout 10
    $gpuiServerProcess.Refresh()
    if (-not $gpuiServerProcess.HasExited -or $gpuiServerProcess.ExitCode -ne 0) {
        throw "GPUI fixture sync server did not shut down cleanly: $(Get-Content -LiteralPath $serverLog -Raw)"
    }
    $gpuiServerProcess = $null
    Invoke-GpuiProbe @('verify-attachments', $legacyPushDb)
    Write-Output 'SYNC_LEGACY_PUSH_ATTACHMENTS=PASS files=2 binary_payloads=preserved'

    Write-Output 'LEGACY_SYNC_WIRE=PASS'
    Write-Output "Legacy sync info: $($info.name) ($($info.id))"
    Write-Output "Isolated fixture retained at $fixture"
} finally {
    if ($legacyProcess) {
        $legacyProcess.Refresh()
        if (-not $legacyProcess.HasExited) { Stop-Process -Id $legacyProcess.Id }
    }
    if ($gpuiServerProcess) {
        [IO.File]::WriteAllText((Join-Path $fixture 'stop-gpui-server'), 'stop')
        $gpuiServerProcess.Refresh()
        if (-not $gpuiServerProcess.HasExited) { Stop-Process -Id $gpuiServerProcess.Id }
    }
}
