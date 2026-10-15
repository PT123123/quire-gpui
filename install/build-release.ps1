param(
    [switch] $SkipBuild,
    [switch] $BuildInstaller
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$manifest = Join-Path $root 'Cargo.toml'
$exe = Join-Path $root 'target\release\quire-gpui.exe'
$cargoArgs = @('build', '--release', '--locked', '--manifest-path', $manifest)

if (-not $SkipBuild) {
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
    throw "Release executable not found: $exe"
}

$version = [regex]::Match((Get-Content -LiteralPath $manifest -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $version) { throw 'Could not read package version from Cargo.toml' }
$dist = Join-Path $root 'dist'
$stage = Join-Path $dist "Quire-GPUI-$version-windows-x64"
$zip = "$stage.zip"
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item -LiteralPath $exe -Destination $stage -Force
Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $stage -Force
Copy-Item -LiteralPath (Join-Path $root 'THIRD-PARTY-NOTICES.md') -Destination $stage -Force
@'
Quire GPUI portable build

Run quire-gpui.exe to start. This build stores data separately from the Slint Quire app at:
%LOCALAPPDATA%\QuireGPUI\quire.db

To use another database, launch with --db PATH. Never point this build at a live Slint database.
Uninstalling or deleting this application folder does not remove the data directory.
'@ | Set-Content -LiteralPath (Join-Path $stage 'PORTABLE-README.txt') -Encoding UTF8
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip -CompressionLevel Optimal
Write-Output "Portable package: $zip"

if ($BuildInstaller) {
    $iscc = (Get-Command 'ISCC.exe' -ErrorAction SilentlyContinue).Source
    if (-not $iscc) {
        $userInstall = Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'
        if (Test-Path -LiteralPath $userInstall -PathType Leaf) { $iscc = $userInstall }
    }
    if (-not $iscc) { throw 'Inno Setup was requested but ISCC.exe was not found on PATH or in the standard per-user install directory.' }
    $iss = Join-Path $PSScriptRoot 'quire-gpui.iss'
    & $iscc $iss
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }
}
