# Capture reference screenshots from the frozen Slint build (headless software renderer).
# quire-shot renders into memory only: backlink/database scenes open an *in-memory* SQLite
# repository (src/bin/quire_shot.rs), every other scene runs repo-less. Nothing here reads
# or writes a user library.
#
# Usage (PowerShell 5.1+):
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools/capture-reference-shots.ps1 `
#     -Shot ..\quire-desktop\target-shot\debug\quire-shot.exe -Mode Matrix
#
# -Mode Matrix : every theme id x -Scenes, plus one baseline shot per theme (no scene).
# -Mode Sweep  : every scene name from docs/15-SCENE-CATALOG.md at its own palette.
# -Mode Tabs   : settings-dialog tabs, reached with --click because no scene moves
#                settings-tab off 0 (see docs/14 finding 2). Coordinates are logical
#                pixels measured for a ${W}x${H} window; -TabCoords overrides the list.
# A scene shot whose bytes equal that theme's baseline is flagged NO-EFFECT in the manifest:
# quire-shot silently ignores an unknown --scene name, so a typo otherwise becomes a
# plausible-looking reference image.

param(
    [Parameter(Mandatory = $true)][string]$Shot,
    [string]$Out = "reference\shots",
    [string]$Mode = "Matrix",
    [int]$W = 1280,
    [int]$H = 800,
    [string]$Themes = "midnight,graphite,violet,emerald,amber,ocean,rose,light,jade,deepblue,twilight,crimson",
    [string]$Scenes = "edit,page-cover,marks,notes,tasks,database-table,settings,sync",
    [string]$SceneCatalog = "docs\15-SCENE-CATALOG.md",
    [string]$TabThemes = "light,midnight",
    [string]$TabCoords = "appearance=418,205;general=500,205;sync=585,205:sync;mcp=669,205;keys=753,205;about=845,205;sync-stats=585,205:sync-stats;sync-log=585,205:sync-log",
    [switch]$Force
)

# -File passes "a,b" as one string, so split here instead of relying on array binding.
$themeList = @($Themes -split '[,;]\s*' | Where-Object { $_ })
$sceneList = @($Scenes -split '[,;]\s*' | Where-Object { $_ })

$ErrorActionPreference = "Continue"
Add-Type -AssemblyName System.Drawing

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$shotFull = (Resolve-Path (Join-Path $root $Shot) -ErrorAction SilentlyContinue)
if (-not $shotFull) { Write-Output "shot binary not found: $Shot"; exit 1 }
$shotExe = $shotFull.Path
$exeStamp = (Get-Item $shotExe).LastWriteTime.ToString("yyyy-MM-dd HH:mm:ss")
$outDir = Join-Path $root $Out
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$tmp = Join-Path $env:TEMP "quire-shot-cap.bmp"
$manifest = Join-Path $outDir "manifest.tsv"
if (-not (Test-Path $manifest)) {
    "name`tmode`tscene`ttheme`tsize`tbytes`tsha256`tstatus" | Out-File -Encoding utf8 -FilePath $manifest
}

function Get-Sha([string]$p) {
    $s = [System.IO.File]::OpenRead($p)
    $h = (Get-FileHash -InputStream $s -Algorithm SHA256).Hash
    $s.Dispose(); $s = $null
    [GC]::Collect()
    $h
}

function Save-Shot([string]$name, [string[]]$callArgs, [string]$mode, [string]$scene, [string]$theme) {
    $png = Join-Path $outDir "$name.png"
    if ((Test-Path $png) -and -not $Force) { return }
    $argv = @("--out", $tmp) + $callArgs
    & $shotExe @argv 2>&1 | Out-Null
    if (-not (Test-Path $tmp)) {
        "$name`t$mode`t$scene`t$theme`t-`t0`t`tFAILED" | Out-File -Encoding utf8 -Append -FilePath $manifest
        return
    }
    $img = [System.Drawing.Image]::FromFile($tmp)
    $img.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
    $img.Dispose()
    Remove-Item $tmp -Force
    $bytes = (Get-Item $png).Length
    "$name`t$mode`t$scene`t$theme`t${W}x${H}`t$bytes`t$(Get-Sha $png)`tOK" |
        Out-File -Encoding utf8 -Append -FilePath $manifest
}

$jobs = @()
if ($Mode -eq "Matrix" -or $Mode -eq "Both") {
    foreach ($t in $themeList) {
        $jobs += , @("t-${t}__baseline", @("--w", $W, "--h", $H, "--theme", $t), "matrix", "baseline", $t)
        foreach ($s in $sceneList) {
            $jobs += , @("t-${t}__${s}", @("--w", $W, "--h", $H, "--theme", $t, "--scene", $s), "matrix", $s, $t)
        }
    }
}
if ($Mode -eq "Sweep" -or $Mode -eq "Both") {
    $cat = Join-Path $root $SceneCatalog
    if (-not (Test-Path $cat)) { Write-Output "catalog missing: $SceneCatalog (run node tools/extract-parity.mjs)"; exit 1 }
    $all = Select-String -Path $cat -Pattern '^\| `([a-z0-9_-]+)`' | ForEach-Object { $_.Matches[0].Groups[1].Value }
    foreach ($s in $all) {
        $jobs += , @("s-${s}", @("--w", $W, "--h", $H, "--scene", $s), "sweep", $s, "scene-default")
    }
}

if ($Mode -eq "Tabs") {
    $tabList = @($TabCoords -split ';' | Where-Object { $_ })
    foreach ($t in ($TabThemes -split '[,;]\s*' | Where-Object { $_ })) {
        foreach ($pair in $tabList) {
            $kv = $pair -split '='
            $tab = $kv[0].Trim()
            # value is "x,y" or "x,y:scene" - the sync sub-tabs need their seeding scene
            $cv = ($kv[1] -split ':')
            $xy = $cv[0].Trim()
            $base = if ($cv.Count -gt 1) { $cv[1].Trim() } else { "settings" }
            $jobs += , @("t-${t}__tab-${tab}",
                @("--w", $W, "--h", $H, "--theme", $t, "--scene", $base, "--click", $xy),
                "tab", "tab-${tab}", $t)
        }
    }
}

Write-Output "planned $($jobs.Count) jobs (mode=$Mode, ${W}x${H})"
$baseline = @{}
$done = 0
foreach ($j in $jobs) {
    $nm = $j[0]; $ar = $j[1]; $md = $j[2]; $sc = $j[3]; $th = $j[4]
    Save-Shot $nm $ar $md $sc $th
    $done++
    if ($md -eq "matrix" -and $sc -eq "baseline") {
        $p = Join-Path $outDir "$nm.png"
        if (Test-Path $p) { $baseline[$th] = Get-Sha $p }
    }
    if ($done % 10 -eq 0) { Write-Output "captured $done/$($jobs.Count)" }
}

# Flag scene shots that came out byte-identical to their theme baseline.
foreach ($th in $baseline.Keys) {
    $b = $baseline[$th]
    $rows = Import-Csv -Path $manifest -Delimiter "`t"
    foreach ($r in $rows) {
        if ($r.theme -eq $th -and $r.scene -ne "baseline" -and $r.sha256 -eq $b) {
            Write-Output "NO-EFFECT: $($r.name) looks identical to the $th baseline (scene ignored?)"
        }
    }
}
# A tab shot that matches the plain settings shot means the click missed the tab strip.
if ($Mode -eq "Tabs") {
    foreach ($t in ($TabThemes -split '[,;]\s*' | Where-Object { $_ })) {
        $ref = Join-Path $outDir "t-${t}__settings.png"
        if (-not (Test-Path $ref)) { Write-Output "NO-BASE: $ref missing, run -Mode Matrix first"; continue }
        $rh = Get-Sha $ref
        foreach ($j in $jobs) {
            if ($j[4] -ne $t) { continue }
            $p = Join-Path $outDir "$($j[0]).png"
            if ((Test-Path $p) -and (Get-Sha $p) -eq $rh) {
                Write-Output "NO-CHANGE: $($j[0]) equals the settings shot - click coordinate missed"
            }
        }
    }
}

Write-Output "done: $done jobs, exe stamp $exeStamp, images in $Out"
