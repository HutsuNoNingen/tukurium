param([string]$OutputDirectory = '', [switch]$SkipBuild, [string]$Version = '')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    . ./scripts/env.ps1
    if (-not $OutputDirectory) { $OutputDirectory = Join-Path $projectRoot 'dist' }
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    $OutputDirectory = (Resolve-Path -LiteralPath $OutputDirectory).Path
    if ($Version -and $Version -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$') { throw 'Version must contain only letters, numbers, dots, underscores and hyphens.' }
    $name = 'Kokoverse-Windows-x64-' + $(if ($Version) { $Version } else { Get-Date -Format 'yyyyMMdd-HHmmss' })
    $sourceCommit = (git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Could not read source commit.' }
    $sourceTreeClean = @(git status --porcelain --untracked-files=all).Count -eq 0
    if ($LASTEXITCODE -ne 0) { throw 'Could not read source status.' }
    $includeLogin = $false
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
    Copy-Item -LiteralPath 'target/release/kokoverse-asset-server.exe' -Destination (Join-Path $packageRoot 'bin')
    $goTool = Join-Path $projectRoot '.tools/go/bin/go.exe'
    if (-not (Test-Path -LiteralPath $goTool)) { $goTool = Join-Path $projectRoot '../kokosync/.tools/go/bin/go.exe' }
    $goTool = (Resolve-Path -LiteralPath $goTool).Path
    Push-Location tools/kokosync-adapter
    try {
        & $goTool test -mod=readonly ./...
        if ($LASTEXITCODE -ne 0) { throw 'Adapter tests failed' }
        & $goTool build -mod=readonly -trimpath '-ldflags=-s -w' -o (Join-Path $packageRoot 'bin/kokosync-adapter.exe') .
        if ($LASTEXITCODE -ne 0) { throw 'Adapter build failed' }
        & $goTool build -mod=readonly -trimpath '-ldflags=-s -w' -o (Join-Path $packageRoot 'bin/kokosync-relay.exe') ./cmd/relay
        if ($LASTEXITCODE -ne 0) { throw 'Local relay build failed' }
        $goModules = & $goTool list -m -mod=readonly -f '{{.Path}}|{{.Version}}|{{.Dir}}' all
        if ($LASTEXITCODE -ne 0) { throw 'Could not list Go dependencies' }
    } finally { Pop-Location }
    foreach ($file in @('Godot_v4.7.2-stable_win64.exe','Godot_v4.7.2-stable_win64_console.exe')) {
        Copy-Item -LiteralPath (Join-Path '.tools/godot' $file) -Destination (Join-Path $packageRoot '.tools/godot')
    }
    # Include the complete runtime so newly added inspector dependencies cannot be omitted.
    $clientFiles = @(Get-ChildItem -LiteralPath 'client-godot' -File | Where-Object {
        ($_.Extension -in @('.gd','.tscn','.tres','.gdshader') -or $_.Name -eq 'project.godot') -and $_.BaseName -notmatch '(^|_)smoke$'
    })
    foreach ($file in $clientFiles) {
        Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $packageRoot 'client-godot')
        if (Test-Path -LiteralPath ($file.FullName + '.uid')) {
            Copy-Item -LiteralPath ($file.FullName + '.uid') -Destination (Join-Path $packageRoot 'client-godot')
        }
    }
    New-Item -ItemType Directory -Force -Path (Join-Path $packageRoot 'client-godot/assets/ui') | Out-Null
    Copy-Item -LiteralPath 'client-godot/assets/ui/selection-cursor.png','client-godot/assets/ui/selection-cursor.png.import' -Destination (Join-Path $packageRoot 'client-godot/assets/ui')
    Copy-Item -LiteralPath 'client-godot/assets/branding' -Destination (Join-Path $packageRoot 'client-godot/assets') -Recurse
    $displayVersion = $name.Substring('Kokoverse-Windows-x64-'.Length)
    $displayVersion | Set-Content -LiteralPath (Join-Path $packageRoot 'client-godot/version.txt') -Encoding Ascii -NoNewline
    $godotOutput = & .tools/godot/Godot_v4.7.2-stable_win64_console.exe --headless --path client-godot --editor --import --quit 2>&1
    if ($LASTEXITCODE -ne 0 -or ($godotOutput | Select-String -Pattern 'SCRIPT ERROR:|^ERROR:')) { throw 'Godot resource import failed' }
    & ./scripts/copy-avatar.ps1 -ClientDirectory (Join-Path $packageRoot 'client-godot')
    $packageImport = & (Join-Path $packageRoot '.tools/godot/Godot_v4.7.2-stable_win64_console.exe') --headless --path (Join-Path $packageRoot 'client-godot') --editor --import --quit 2>&1
    if ($LASTEXITCODE -ne 0 -or ($packageImport | Select-String -Pattern 'SCRIPT ERROR:|^ERROR:')) {
        $packageImport | Write-Output
        throw 'Packaged Godot project did not import successfully'
    }
    Get-ChildItem -LiteralPath 'distribution' -File | Copy-Item -Destination $packageRoot
    Get-ChildItem -LiteralPath 'distribution/scripts' -File | Copy-Item -Destination (Join-Path $packageRoot 'scripts')
    Copy-Item -LiteralPath 'scripts/setup-media.ps1' -Destination (Join-Path $packageRoot 'scripts')
    Copy-Item -LiteralPath 'scripts/start-instance.ps1' -Destination (Join-Path $packageRoot 'scripts')
    Copy-Item -LiteralPath 'scripts/start-local.ps1','scripts/stop-local.ps1','scripts/start-online.ps1','scripts/stop-online.ps1','scripts/connect-relay.ps1','scripts/process-lifecycle.ps1' -Destination (Join-Path $packageRoot 'scripts')
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
tukurium embeds the Rust crates and Go modules enumerated in build-info.json.
Their upstream license files are included in this directory when supplied.
Godot 4.7.2 and its third-party notices are included in Godot-*.txt/AUTHORS.md.
The KokoSync protocol and SDK are from:
https://github.com/katteni-kaihatu/kokosync/tree/5a171af078d3cf85cc3364c9358e1b993f4f9835
FFmpeg is NOT included in this archive. Optional video tools are downloaded
from GyanD's GitHub release mirror, with https://www.gyan.dev/ffmpeg/builds/
as fallback. setup-media.ps1 verifies a pinned SHA-256 and preserves the
downloaded license alongside those tools.
Oekaki avatar source attribution: client-godot/assets/oekaki/README.md.
User-imported world assets are NOT included in this archive.
'@ | Set-Content -LiteralPath (Join-Path $licenseRoot 'NOTICE.txt') -Encoding UTF8

    foreach ($directory in @('crates','tools/kokosync-adapter','distribution','deploy','docs')) {
        $target = Join-Path $packageRoot ('source/' + $directory)
        New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
        Copy-Item -LiteralPath $directory -Destination $target -Recurse
    }
    foreach ($file in @('Cargo.toml','Cargo.lock','README.md')) { Copy-Item -LiteralPath $file -Destination (Join-Path $packageRoot 'source') }
    New-Item -ItemType Directory -Path (Join-Path $packageRoot 'source/client-godot') -Force | Out-Null
    Get-ChildItem -LiteralPath (Join-Path $packageRoot 'client-godot') -File | Copy-Item -Destination (Join-Path $packageRoot 'source/client-godot')
    New-Item -ItemType Directory -Path (Join-Path $packageRoot 'source/client-godot/assets') -Force | Out-Null
    Copy-Item -LiteralPath 'client-godot/assets/oekaki' -Destination (Join-Path $packageRoot 'source/client-godot/assets') -Recurse
    Copy-Item -LiteralPath 'client-godot/assets/ui' -Destination (Join-Path $packageRoot 'source/client-godot/assets') -Recurse
    Copy-Item -LiteralPath 'client-godot/assets/branding' -Destination (Join-Path $packageRoot 'source/client-godot/assets') -Recurse
    $files = @(Get-ChildItem -LiteralPath $packageRoot -File -Recurse -Force | ForEach-Object {
        @{ path=$_.FullName.Substring($packageRoot.Length+1).Replace('\','/'); size=$_.Length; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
    $connection = Get-Content -LiteralPath 'distribution/connection.json' -Raw | ConvertFrom-Json
    @{ version=$name; sourceCommit=$sourceCommit; sourceTreeClean=$sourceTreeClean; protocol=3; service=$connection.service; relay=$connection.relay; sessionMode='managed'; legacyRoom='kokoverse/workshop'; avatar='oekaki'; includesUserWorld=$false; includesCredentials=$includeLogin; optionalVideoTools='FFmpeg 9.0.2 via Install-Video-Tools.bat'; rustDependencies=@($metadata.packages | Where-Object source | Select-Object name,version,license); goDependencies=$goModules | ForEach-Object { ($_ -split '\|')[0..1] -join '@' }; files=$files } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $packageRoot 'build-info.json') -Encoding UTF8
    $archive = Join-Path $OutputDirectory ($name + '.zip')
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($stagingParent,$archive,[IO.Compression.CompressionLevel]::Optimal,$false)
    (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() + '  ' + (Split-Path -Leaf $archive) | Set-Content -LiteralPath ($archive + '.sha256') -Encoding Ascii
    Write-Output "PACKAGE_ROOT=$packageRoot"
    Write-Output "PACKAGE_ZIP=$archive"
} finally { Pop-Location }
