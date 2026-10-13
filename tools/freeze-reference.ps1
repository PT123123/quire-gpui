# One-time reference capture. Never overwrites an existing frozen snapshot.
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$sourceRoot = [IO.Path]::GetFullPath((Join-Path $projectRoot '../quire-desktop'))
$referenceRoot = Join-Path $projectRoot 'reference/slint-desktop'
if (Test-Path -LiteralPath $referenceRoot) { throw 'Reference exists. Preserve it; create a separately named revision instead.' }
New-Item -ItemType Directory -Path $referenceRoot -Force | Out-Null
$sourceFiles = [Collections.Generic.List[IO.FileInfo]]::new()
foreach ($folder in @('ui', 'src')) {
    foreach ($file in Get-ChildItem -LiteralPath (Join-Path $sourceRoot $folder) -Recurse -File) { $sourceFiles.Add($file) }
}
foreach ($file in Get-ChildItem -LiteralPath (Join-Path $sourceRoot 'docs') -File -Filter '*.md') { $sourceFiles.Add($file) }
foreach ($name in @('Cargo.toml','Cargo.lock','build.rs','rust-toolchain.toml','justfile','README.md','PLAN.md','CHANGELOG.md')) {
    $path = Join-Path $sourceRoot $name
    if (Test-Path -LiteralPath $path) { $sourceFiles.Add((Get-Item -LiteralPath $path)) }
}
foreach ($folder in @('tests','benchmarks/scripts')) {
    $path = Join-Path $sourceRoot $folder
    if (Test-Path -LiteralPath $path) {
        foreach ($file in Get-ChildItem -LiteralPath $path -Recurse -File) {
            if ($file.Extension -in @('.rs','.ps1','.md','.json','.toml')) { $sourceFiles.Add($file) }
        }
    }
}
$entries = @()
foreach ($file in $sourceFiles) {
    $relative = $file.FullName.Substring($sourceRoot.Length + 1).Replace('\','/')
    $destination = Join-Path $referenceRoot $relative
    New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
    Copy-Item -LiteralPath $file.FullName -Destination $destination
    $sourceHash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $copyHash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($sourceHash -ne $copyHash) { throw "Source changed during capture: $relative" }
    $entries += [ordered]@{ path = $relative; bytes = $file.Length; sha256 = $copyHash }
}
$commit = (git -C $sourceRoot rev-parse HEAD).Trim()
$dirty = @(git -C $sourceRoot status --short)
$manifest = [ordered]@{
    created_at = [DateTimeOffset]::UtcNow.ToOffset([TimeSpan]::FromHours(8)).ToString('o')
    source = '../quire-desktop'; git_head = $commit; git_status_short = $dirty
    reference_kind = 'working-tree snapshot including uncommitted modifications'
    excluded = @('user databases','attachments','runtime settings','logs','target','binaries','screenshots','clips','git metadata')
    files = $entries
}
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $projectRoot 'docs/REFERENCE-MANIFEST.json') -Encoding utf8

$colors = Get-Content -LiteralPath (Join-Path $referenceRoot 'ui/Colors.slint') -Raw -Encoding utf8
$palettes = @()
foreach ($match in [regex]::Matches($colors, '\{\s*id:\s*"(?<id>[^"]+)"(?<body>.*?)\}', 'Singleline')) {
    $palette = [ordered]@{ id = $match.Groups['id'].Value }
    foreach ($property in [regex]::Matches($match.Groups['body'].Value, '(?<key>[\w-]+):\s*(?<value>"[^"]*"|#[0-9a-fA-F]+|true|false)')) {
        $value = $property.Groups['value'].Value
        if ($value -eq 'true') { $value = $true } elseif ($value -eq 'false') { $value = $false } else { $value = $value.Trim('"') }
        $palette[$property.Groups['key'].Value] = $value
    }
    $palettes += $palette
}
if ($palettes.Count -ne 12) { throw "Expected 12 palettes, found $($palettes.Count)" }
$literalTokens = @()
foreach ($name in @('Theme.slint','Typography.slint')) {
    $content = Get-Content -LiteralPath (Join-Path $referenceRoot "ui/$name") -Raw -Encoding utf8
    foreach ($match in [regex]::Matches($content, 'out property <(?<type>[^>]+)> (?<name>[\w-]+):\s*(?<value>[^;]+);')) {
        $literalTokens += [ordered]@{ source = "ui/$name"; name = $match.Groups['name'].Value; type = $match.Groups['type'].Value; expression = $match.Groups['value'].Value.Trim() }
    }
}
[ordered]@{ palettes = $palettes; token_expressions = $literalTokens; note = 'Expressions are Slint source; translate semantics, not syntax. Derived Colors functions remain authoritative.' } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $projectRoot 'reference/design-tokens.json') -Encoding utf8

$lines = [Collections.Generic.List[string]]::new()
$lines.Add('# Reference source inventory')
$lines.Add('')
$lines.Add('Generated from the frozen working tree. Line numbers refer to reference/slint-desktop, not a future implementation.')
$lines.Add('')
$lines.Add("Git HEAD: $commit")
$lines.Add('')
$lines.Add('## UI files and declared components')
$lines.Add('')
foreach ($file in Get-ChildItem -LiteralPath (Join-Path $referenceRoot 'ui') -Recurse -File -Filter '*.slint') {
    $relative = $file.FullName.Substring($referenceRoot.Length + 1).Replace('\','/')
    $content = @(Get-Content -LiteralPath $file.FullName -Encoding utf8)
    $lines.Add("### $relative ($($content.Count) lines)")
    $lines.Add('')
    for ($i = 0; $i -lt $content.Count; $i++) {
        if ($content[$i] -match '^\s*(export )?(component|global|struct) ') { $lines.Add("- line $($i+1): $($content[$i].Trim())") }
    }
    $lines.Add('')
}
$lines.Add('## UI callbacks')
$lines.Add('')
$types = @(Get-Content -LiteralPath (Join-Path $referenceRoot 'ui/Types.slint') -Encoding utf8)
for ($i = 0; $i -lt $types.Count; $i++) {
    if ($types[$i] -match '\bcallback\b') { $lines.Add("- ui/Types.slint:$($i+1): $($types[$i].Trim())") }
}
$lines.Add('')
$lines.Add('## Scene routing and seeds (candidate index)')
$lines.Add('')
$controller = @(Get-Content -LiteralPath (Join-Path $referenceRoot 'src/app/controller.rs') -Encoding utf8)
$sceneStart = -1
for ($i = 0; $i -lt $controller.Count; $i++) {
    if ($controller[$i] -match 'fn apply_scene') { if ($sceneStart -lt 0) { $sceneStart = $i }; $lines.Add("- src/app/controller.rs:$($i+1): $($controller[$i].Trim())") }
    if ($sceneStart -ge 0 -and $controller[$i] -match '^\s*"[^"\r\n]+".*=>') { $lines.Add("- src/app/controller.rs:$($i+1): $($controller[$i].Trim())") }
}
$lines.Add('')
$lines.Add('This index is a discovery aid. Inspect full branches, runtime preconditions and controller registration; it is not a completeness proof.')
$lines | Set-Content -LiteralPath (Join-Path $projectRoot 'docs/10-SOURCE-INVENTORY.md') -Encoding utf8
Write-Output "Captured $($entries.Count) files; $($palettes.Count) palettes; commit $commit; dirty entries $($dirty.Count)."
