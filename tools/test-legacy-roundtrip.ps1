param(
    [string]$LegacyExe = (Join-Path $PSScriptRoot '..\..\quire-desktop\target\release\quire.exe')
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$legacyPath = (Resolve-Path $LegacyExe).Path
$fixture = Join-Path $env:TEMP ('quire-legacy-roundtrip-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
$db = Join-Path $fixture 'schema.db'
$token = 'legacy-probe-' + [guid]::NewGuid().ToString('N')
$portListener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$portListener.Start()
$port = $portListener.LocalEndpoint.Port
$portListener.Stop()
$legacyProcess = $null

function Invoke-GpuiProbe([string[]]$Arguments) {
    & cargo run --locked --manifest-path (Join-Path $repoRoot 'Cargo.toml') --example legacy_schema_probe -- @Arguments
    if ($LASTEXITCODE -ne 0) { throw "GPUI compatibility probe failed ($LASTEXITCODE): $Arguments" }
}

function Invoke-JsonPost([string]$Uri, [hashtable]$Headers, [string]$Body) {
    $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($Body)
    Invoke-RestMethod -Uri $Uri -Method Post -Headers $Headers `
        -ContentType 'application/json; charset=utf-8' -Body $bodyBytes
}

try {
    Invoke-GpuiProbe @('write', $db)
    Invoke-GpuiProbe @('configure', $db, [string]$port, $token)

    # Only this isolated Slint process runs hidden; it closes itself after the
    # MCP write so the final GPUI reader can prove the change survived shutdown.
    $legacyProcess = Start-Process -FilePath $legacyPath -ArgumentList @(
        '--db', $db, '--mcp', '--auto-exit', '8'
    ) -PassThru -WindowStyle Hidden

    $ready = $false
    for ($i = 0; $i -lt 100; $i++) {
        $client = [System.Net.Sockets.TcpClient]::new()
        try {
            $client.Connect('127.0.0.1', $port)
            $ready = $true
            break
        } catch {
            $legacyProcess.Refresh()
            if ($legacyProcess.HasExited) { throw "Slint exited before MCP became ready ($($legacyProcess.ExitCode))" }
            Start-Sleep -Milliseconds 100
        } finally {
            $client.Dispose()
        }
    }
    if (-not $ready) { throw "Slint MCP did not listen on isolated loopback port $port" }

    $headers = @{
        Authorization = "Bearer $token"
        'MCP-Protocol-Version' = '2025-06-18'
    }
    $initialize = @{
        jsonrpc = '2.0'; id = 1; method = 'initialize'
        params = @{
            protocolVersion = '2025-06-18'; capabilities = @{}
            clientInfo = @{ name = 'quire-legacy-roundtrip-probe'; version = '1' }
        }
    } | ConvertTo-Json -Compress -Depth 8
    $initialized = Invoke-JsonPost -Uri "http://127.0.0.1:$port/mcp" -Headers $headers -Body $initialize
    if (-not $initialized.result.protocolVersion) { throw 'Slint MCP initialization failed' }

    $createNote = @{
        jsonrpc = '2.0'; id = 2; method = 'tools/call'
        params = @{
            name = 'create_note'
            arguments = @{
                title = '旧版 MCP 写入验收'
                body = 'Slint → GPUI 隔离库往返通过'
                tags = '互通验收'
            }
        }
    } | ConvertTo-Json -Compress -Depth 8
    $created = Invoke-JsonPost -Uri "http://127.0.0.1:$port/mcp" -Headers $headers -Body $createNote
    if ($created.result.isError) { throw "Slint MCP rejected create_note: $($created.result.content[0].text)" }
    Write-Output "Slint MCP create_note result: $($created.result.content[0].text)"
    $noteId = [int64](($created.result.content[0].text | ConvertFrom-Json).id)

    foreach ($operation in @(
        @{ Id = 3; Name = 'trash_note' },
        @{ Id = 4; Name = 'restore_note' }
    )) {
        $transition = @{
            jsonrpc = '2.0'; id = $operation.Id; method = 'tools/call'
            params = @{ name = $operation.Name; arguments = @{ note_id = $noteId } }
        } | ConvertTo-Json -Compress -Depth 8
        $transitionResult = Invoke-JsonPost -Uri "http://127.0.0.1:$port/mcp" -Headers $headers -Body $transition
        if ($transitionResult.result.isError) {
            throw "Slint MCP $($operation.Name) failed: $($transitionResult.result.content[0].text)"
        }
        Write-Output "Slint MCP $($operation.Name) for isolated note ${noteId}: PASS"
    }

    $createTask = @{
        jsonrpc = '2.0'; id = 5; method = 'tools/call'
        params = @{
            name = 'create_task'
            arguments = @{
                title = '旧版 MCP 任务恢复验收'
                notes = 'Slint → GPUI 隔离任务回收站往返通过'
                tags = '互通验收'
            }
        }
    } | ConvertTo-Json -Compress -Depth 8
    $createdTask = Invoke-JsonPost -Uri "http://127.0.0.1:$port/mcp" -Headers $headers -Body $createTask
    if ($createdTask.result.isError) {
        throw "Slint MCP rejected create_task: $($createdTask.result.content[0].text)"
    }
    $taskId = [int64](($createdTask.result.content[0].text | ConvertFrom-Json).id)

    foreach ($operation in @(
        @{ Id = 6; Name = 'trash_task' },
        @{ Id = 7; Name = 'restore_task' }
    )) {
        $transition = @{
            jsonrpc = '2.0'; id = $operation.Id; method = 'tools/call'
            params = @{ name = $operation.Name; arguments = @{ task_id = $taskId } }
        } | ConvertTo-Json -Compress -Depth 8
        $transitionResult = Invoke-JsonPost -Uri "http://127.0.0.1:$port/mcp" -Headers $headers -Body $transition
        if ($transitionResult.result.isError) {
            throw "Slint MCP $($operation.Name) failed: $($transitionResult.result.content[0].text)"
        }
        Write-Output "Slint MCP $($operation.Name) for isolated task ${taskId}: PASS"
    }

    Wait-Process -Id $legacyProcess.Id -Timeout 15
    $legacyProcess.Refresh()
    if (-not $legacyProcess.HasExited) { throw 'Slint fixture did not close cleanly' }
    if ($legacyProcess.ExitCode -ne 0) { throw "Slint fixture exited with code $($legacyProcess.ExitCode)" }

    Invoke-GpuiProbe @('verify', $db)
    Write-Output "LEGACY_ROUNDTRIP=PASS"
    Write-Output "Isolated fixture retained at $fixture"
} finally {
    if ($legacyProcess) {
        $legacyProcess.Refresh()
        if (-not $legacyProcess.HasExited) { Stop-Process -Id $legacyProcess.Id }
    }
}
