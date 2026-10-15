param(
    [Parameter(Mandatory = $true)][string]$Repo,
    [string]$TouchFile = "",
    [string]$Output = ""
)

$ErrorActionPreference = "Stop"
$repoPath = (Resolve-Path -LiteralPath $Repo).Path
if ($Output -eq "") {
    $Output = Join-Path $env:TEMP ("quire-build-measure-" + [guid]::NewGuid().ToString("N"))
}
$outputPath = [IO.Path]::GetFullPath($Output)
$outputDir = Split-Path -Parent $outputPath
if (-not (Test-Path -LiteralPath $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir | Out-Null
}

$touchedPath = ""
if ($TouchFile -ne "") {
    $touchedPath = Join-Path $repoPath $TouchFile
    if (-not (Test-Path -LiteralPath $touchedPath -PathType Leaf)) {
        throw "Touch file not found: $touchedPath"
    }
    (Get-Item -LiteralPath $touchedPath).LastWriteTimeUtc = [DateTime]::UtcNow
}

$stdoutPath = "$outputPath.stdout.log"
$stderrPath = "$outputPath.stderr.log"
$cargoPath = (Get-Command cargo.exe).Source
$commit = (& git -C $repoPath rev-parse HEAD).Trim()
$cargoVersion = (& cargo --version).Trim()
$rustcVersion = (& rustc --version).Trim()
$startedAt = [DateTime]::UtcNow
$watch = [Diagnostics.Stopwatch]::StartNew()
$process = Start-Process -FilePath $cargoPath `
    -ArgumentList "build --release --locked" `
    -WorkingDirectory $repoPath -PassThru `
    -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath

$peakWorkingSet = 0L
$peakPrivate = 0L
$samples = 0
$sampleMs = 250
while (-not $process.HasExited) {
    $procs = @(Get-Process -Name cargo,rustc,link,lld-link -ErrorAction SilentlyContinue)
    if ($procs.Count -gt 0) {
        $working = ($procs | Measure-Object -Property WorkingSet64 -Sum).Sum
        $private = ($procs | Measure-Object -Property PrivateMemorySize64 -Sum).Sum
        if ($working -gt $peakWorkingSet) { $peakWorkingSet = [long]$working }
        if ($private -gt $peakPrivate) { $peakPrivate = [long]$private }
        $samples++
    }
    Start-Sleep -Milliseconds $sampleMs
    $process.Refresh()
}
$process.WaitForExit()
$watch.Stop()

$record = [ordered]@{
    repo = $repoPath
    commit = $commit
    command = "cargo build --release --locked"
    touched_source = $touchedPath
    started_at_utc = $startedAt.ToString("o")
    elapsed_seconds = [math]::Round($watch.Elapsed.TotalSeconds, 2)
    exit_code = $process.ExitCode
    sample_interval_ms = $sampleMs
    memory_sample_count = $samples
    peak_compiler_working_set_mb = [math]::Round($peakWorkingSet / 1MB, 1)
    peak_compiler_private_mb = [math]::Round($peakPrivate / 1MB, 1)
    cargo = $cargoVersion
    rustc = $rustcVersion
    stdout_log = $stdoutPath
    stderr_log = $stderrPath
}
$record | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $outputPath -Encoding utf8
Get-Content -LiteralPath $outputPath
if ($process.ExitCode -ne 0) { exit $process.ExitCode }
