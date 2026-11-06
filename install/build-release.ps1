param(
    [switch] $SkipBuild,
    [switch] $BuildInstaller,
    [string] $TargetDir = 'target'
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$manifest = Join-Path $root 'Cargo.toml'
$buildTarget = if ([IO.Path]::IsPathRooted($TargetDir)) {
    [IO.Path]::GetFullPath($TargetDir)
} else {
    [IO.Path]::GetFullPath((Join-Path $root $TargetDir))
}
$exe = Join-Path $buildTarget 'release\quire-gpui.exe'
$cargoArgs = @('build', '--release', '--locked', '--manifest-path', $manifest, '--target-dir', $buildTarget)

if (-not $SkipBuild) {
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
    throw "Release executable not found in target directory '$buildTarget': $exe"
}
$manifestCheck = Join-Path $PSScriptRoot '..\tools\verify-windows-manifest.ps1'
& $manifestCheck -Path $exe

$version = [regex]::Match((Get-Content -LiteralPath $manifest -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $version) { throw 'Could not read package version from Cargo.toml' }
$dist = Join-Path $root 'dist'
$stage = Join-Path $dist "Quire-GPUI-$version-windows-x64"
$zip = "$stage.zip"
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item -LiteralPath $exe -Destination $stage -Force
& $manifestCheck -Path (Join-Path $stage 'quire-gpui.exe')
Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $stage -Force
Copy-Item -LiteralPath (Join-Path $root 'THIRD-PARTY-NOTICES.md') -Destination $stage -Force
@'
Quire GPUI portable build

Run quire-gpui.exe to start. Since 2026-11-06 the GPUI and Slint desktops share one data
backend, so the default library is the one the Slint app already uses:
%APPDATA%\Quire\quire.db

Only one of the two apps may hold that library open at a time. GPUI refuses to start while
the Slint desktop is attached to it; the Slint side has no matching check yet, so close
GPUI (including its tray-resident process) before launching the Slint app.

To work on another database, launch with --db PATH. To fold an older separate GPUI
library into the shared one, run once:
quire-gpui.exe --import-library %LOCALAPPDATA%\QuireGPUI\quire.db
That mode opens no window, prints a JSON report of what arrived, only adds rows, and exits.
Uninstalling or deleting this application folder does not remove any data.
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
