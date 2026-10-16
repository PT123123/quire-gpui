$ErrorActionPreference = 'Stop'

$gpuiRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$legacyRoot = (Resolve-Path (Join-Path $gpuiRoot '..\quire-desktop')).Path
$coreRoot = (Resolve-Path (Join-Path $gpuiRoot '..\quire-core')).Path

function Get-CoreRevision([string] $ManifestPath) {
    $manifest = Get-Content -LiteralPath $ManifestPath -Raw
    $match = [regex]::Match(
        $manifest,
        '(?m)^quire-core\s*=\s*\{[^\r\n]*?rev\s*=\s*"([0-9a-f]{7,40})"'
    )
    if (-not $match.Success) {
        throw "Could not read pinned quire-core revision from $ManifestPath"
    }
    return $match.Groups[1].Value
}

$gpuiRevision = Get-CoreRevision (Join-Path $gpuiRoot 'Cargo.toml')
$legacyRevision = Get-CoreRevision (Join-Path $legacyRoot 'Cargo.toml')

& git -C $coreRoot cat-file -e "$legacyRevision^{commit}"
if ($LASTEXITCODE -ne 0) { throw "Legacy core revision is unavailable: $legacyRevision" }
& git -C $coreRoot cat-file -e "$gpuiRevision^{commit}"
if ($LASTEXITCODE -ne 0) { throw "GPUI core revision is unavailable: $gpuiRevision" }

& git -C $coreRoot diff --quiet $legacyRevision $gpuiRevision -- src/services/sync
if ($LASTEXITCODE -ne 0) {
    throw "Sync source differs between locked core revisions $legacyRevision and $gpuiRevision"
}

function Get-SnapshotVersion([string] $Revision) {
    $source = & git -C $coreRoot show "${Revision}:src/services/sync/model.rs"
    if ($LASTEXITCODE -ne 0) { throw "Could not read sync model at $Revision" }
    $text = $source -join "`n"
    $match = [regex]::Match($text, 'pub const SNAPSHOT_VERSION: u32 = (\d+);')
    if (-not $match.Success) { throw "Could not find snapshot version at $Revision" }
    return [int]$match.Groups[1].Value
}

$legacyVersion = Get-SnapshotVersion $legacyRevision
$gpuiVersion = Get-SnapshotVersion $gpuiRevision
if ($legacyVersion -ne $gpuiVersion) {
    throw "Snapshot protocol version differs: legacy=$legacyVersion GPUI=$gpuiVersion"
}

Write-Output "SYNC_CORE_SOURCE=IDENTICAL"
Write-Output "LEGACY_CORE=$legacyRevision SNAPSHOT_VERSION=$legacyVersion"
Write-Output "GPUI_CORE=$gpuiRevision SNAPSHOT_VERSION=$gpuiVersion"
Write-Output 'LIMIT=Core wire/merge/transport source only; desktop-process exchange still requires integration verification.'
