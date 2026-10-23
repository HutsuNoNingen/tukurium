param([string]$OutputDirectory = '', [switch]$SkipBuild, [string]$RelayCredentialsFile = '')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    . ./scripts/env.ps1
    if (-not $OutputDirectory) { $OutputDirectory = Join-Path $projectRoot 'dist' }
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    $OutputDirectory = (Resolve-Path -LiteralPath $OutputDirectory).Path
    $name = 'Kokoverse-Windows-x64-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
    $includeLogin = -not [string]::IsNullOrWhiteSpace($RelayCredentialsFile)
    $bundledAccess = $null
    if ($includeLogin) {
        $accessText = Get-Content -LiteralPath $RelayCredentialsFile -Raw
        $username = [regex]::Match($accessText,'(?m)^Username:\s*(.+)$').Groups[1].Value.Trim()
        $password = [regex]::Match($accessText,'(?m)^Password:\s*(.+)$').Groups[1].Value.Trim()
        if (-not $username -or -not $password) { throw 'Relay credentials file must contain Username and Password lines.' }
        $bundledAccess = @{ username=$username; password=$password }
    }
    $stagingParent = Join-Path $projectRoot ('.run/packages/' + [guid]::NewGuid().ToString('N'))
    $packageRoot = Join-Path $stagingParent $name
    foreach ($directory in @('bin','scripts','client-godot','.tools/godot','licenses','source')) {
        New-Item -ItemType Directory -Force -Path (Join-Path $packageRoot $directory) | Out-Null
    }
    if (-not $SkipBuild) {
        & cargo build --release --workspace --locked
        if ($LASTEXITCODE -ne 0) { throw 'Rust release build failed' }
    }
    Copy-Item -LiteralPath 'target/release/kokoverse-engine.exe' -Destination (Join-Path $packageRoot 'bin')
    $goTool = Join-Path $projectRoot '.tools/go/bin/go.exe'
    if (-not (Test-Path -LiteralPath $goTool)) { $goTool = Join-Path $projectRoot '../kokosync/.tools/go/bin/go.exe' }
    $goTool = (Resolve-Path -LiteralPath $goTool).Path
    Push-Location tools/kokosync-adapter
    try {
        & $goTool test -mod=readonly ./...
        if ($LASTEXITCODE -ne 0) { throw 'Adapter tests failed' }
        & $goTool build -mod=readonly -trimpath '-ldflags=-s -w' -o (Join-Path $packageRoot 'bin/kokosync-adapter.exe') .
        if ($LASTEXITCODE -ne 0) { throw 'Adapter build failed' }
        $goModules = & $goTool list -m -mod=readonly -f '{{.Path}}|{{.Version}}|{{.Dir}}' all
        if ($LASTEXITCODE -ne 0) { throw 'Could not list Go dependencies' }
    } finally { Pop-Location }
    foreach ($file in @('Godot_v4.7.2-stable_win64.exe','Godot_v4.7.2-stable_win64_console.exe')) {
        Copy-Item -LiteralPath (Join-Path '.tools/godot' $file) -Destination (Join-Path $packageRoot '.tools/godot')
    }
    foreach ($file in @('main.gd','main.tscn','project.godot','avatar.gd','translation_gizmo.gd','asset_importer.gd','asset_status.gd','script_editor.gd','world_panel.gd','world_panels.gd','inspector_view.gd','request_broker.gd')) {
        Copy-Item -LiteralPath (Join-Path 'client-godot' $file) -Destination (Join-Path $packageRoot 'client-godot')
    }
    $godotOutput = & .tools/godot/Godot_v4.7.2-stable_win64_console.exe --headless --path client-godot --editor --import 2>&1
    if ($LASTEXITCODE -ne 0 -or ($godotOutput | Select-String -Pattern 'SCRIPT ERROR:|^ERROR:')) { throw 'Godot resource import failed' }
    & ./scripts/copy-avatar.ps1 -ClientDirectory (Join-Path $packageRoot 'client-godot')
    Get-ChildItem -LiteralPath 'distribution' -File | Copy-Item -Destination $packageRoot
    if ($includeLogin) { $bundledAccess | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $packageRoot 'relay-access.json') -Encoding UTF8 }
    Get-ChildItem -LiteralPath 'distribution/scripts' -File | Copy-Item -Destination (Join-Path $packageRoot 'scripts')
    Copy-Item -LiteralPath 'scripts/setup-media.ps1' -Destination (Join-Path $packageRoot 'scripts')
    Copy-Item -LiteralPath 'scripts/stop-demo.ps1' -Destination (Join-Path $packageRoot 'scripts/stop.ps1')

    # Preserve license notices without copying toolchains, credentials or user assets.
    $licenseRoot = Join-Path $packageRoot 'licenses'
    foreach ($file in @('LICENSE.txt','COPYRIGHT.txt','AUTHORS.md')) {
        Invoke-WebRequest -Uri ('https://raw.githubusercontent.com/godotengine/godot/4.7.2-stable/' + $file) -OutFile (Join-Path $licenseRoot ('Godot-' + $file))
    }
    function Copy-Licenses([string]$Directory,[string]$Label) {
        if (-not $Directory -or -not (Test-Path -LiteralPath $Directory)) { return }
        $files = @(Get-ChildItem -LiteralPath $Directory -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|COPYRIGHT|NOTICE)' })
        if ($files.Count -eq 0) { return }
        $destination = Join-Path $licenseRoot $Label
        New-Item -ItemType Directory -Path $destination -Force | Out-Null
        $files | Copy-Item -Destination $destination
    }
    $metadataText = & cargo metadata --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw 'Could not list Rust dependencies' }
    $metadata = $metadataText | ConvertFrom-Json
    foreach ($package in $metadata.packages) {
        if ($package.source) { Copy-Licenses (Split-Path -Parent $package.manifest_path) ('rust-' + $package.name + '-' + $package.version) }
    }
    foreach ($line in $goModules) {
        $parts = $line -split '\|',3
        if ($parts.Length -eq 3 -and $parts[1]) { Copy-Licenses $parts[2] ('go-' + (($parts[0] + '-' + $parts[1]) -replace '[/\\:]','_')) }
    }
    Copy-Licenses (Split-Path -Parent (Split-Path -Parent $goTool)) 'Go-runtime'
    @'
Kokoverse embeds the Rust crates and Go modules enumerated in build-info.json.
Their upstream license files are included in this directory when supplied.
Godot 4.7.2 and its third-party notices are included in Godot-*.txt/AUTHORS.md.
The KokoSync protocol and SDK are from:
https://github.com/katteni-kaihatu/kokosync/tree/5a171af078d3cf85cc3364c9358e1b993f4f9835
FFmpeg is NOT included in this archive. Optional video tools are downloaded
directly from https://www.gyan.dev/ffmpeg/builds/ by setup-media.ps1 with a
pinned SHA-256; the downloaded license is preserved alongside those tools.
Oekaki avatar source attribution: client-godot/assets/oekaki/README.md.
User-imported world assets are NOT included in this archive.
'@ | Set-Content -LiteralPath (Join-Path $licenseRoot 'NOTICE.txt') -Encoding UTF8

    foreach ($directory in @('crates','tools/kokosync-adapter','distribution')) {
        $target = Join-Path $packageRoot ('source/' + $directory)
        New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
        Copy-Item -LiteralPath $directory -Destination $target -Recurse
    }
    foreach ($file in @('Cargo.toml','Cargo.lock','README.md')) { Copy-Item -LiteralPath $file -Destination (Join-Path $packageRoot 'source') }
    New-Item -ItemType Directory -Path (Join-Path $packageRoot 'source/client-godot') -Force | Out-Null
    Get-ChildItem -LiteralPath (Join-Path $packageRoot 'client-godot') -File | Copy-Item -Destination (Join-Path $packageRoot 'source/client-godot')
    New-Item -ItemType Directory -Path (Join-Path $packageRoot 'source/client-godot/assets') -Force | Out-Null
    Copy-Item -LiteralPath 'client-godot/assets/oekaki' -Destination (Join-Path $packageRoot 'source/client-godot/assets') -Recurse
    $files = @(Get-ChildItem -LiteralPath $packageRoot -File -Recurse -Force | ForEach-Object {
        @{ path=$_.FullName.Substring($packageRoot.Length+1).Replace('\','/'); size=$_.Length; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
    @{ version=$name; protocol=3; relay='wss://45.77.130.146/ws'; room='kokoverse/workshop'; avatar='oekaki'; includesUserWorld=$false; includesCredentials=$includeLogin; optionalVideoTools='FFmpeg 9.0.2 downloaded in background on first launch'; rustDependencies=@($metadata.packages | Where-Object source | Select-Object name,version,license); goDependencies=$goModules | ForEach-Object { ($_ -split '\|')[0..1] -join '@' }; files=$files } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $packageRoot 'build-info.json') -Encoding UTF8
    $archive = Join-Path $OutputDirectory ($name + '.zip')
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($stagingParent,$archive,[IO.Compression.CompressionLevel]::Optimal,$false)
    (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + (Split-Path -Leaf $archive) | Set-Content -LiteralPath ($archive + '.sha256') -Encoding Ascii
    Write-Output "PACKAGE_ROOT=$packageRoot"
    Write-Output "PACKAGE_ZIP=$archive"
} finally { Pop-Location }
