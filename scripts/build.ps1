$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    . ./scripts/env.ps1
    & cargo build --workspace --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
    $goExecutable = Join-Path $projectRoot '.tools/go/bin/go.exe'
    if (-not (Test-Path $goExecutable)) {
        $goExecutable = Join-Path $projectRoot '../kokosync/.tools/go/bin/go.exe'
    }
    if (-not (Test-Path $goExecutable)) { throw 'Run scripts/setup.ps1 first' }
    Push-Location tools/kokosync-adapter
    try {
        & $goExecutable build -mod=readonly -o ../../.tools/kokosync-adapter.exe .
        if ($LASTEXITCODE -ne 0) { throw 'Adapter build failed' }
        & $goExecutable build -mod=readonly -o ../../.tools/kokosync-relay.exe ./cmd/relay
        if ($LASTEXITCODE -ne 0) { throw 'Relay build failed' }
    } finally { Pop-Location }
    $godotOutput = & .tools/godot/Godot_v4.7.2-stable_win64_console.exe --headless --path client-godot --editor --import 2>&1
    $godotCode = $LASTEXITCODE
    $godotOutput | ForEach-Object { Write-Host $_ }
    if ($godotCode -ne 0 -or ($godotOutput | Select-String -Pattern 'SCRIPT ERROR:|^ERROR:')) { throw 'Godot import failed' }
} finally { Pop-Location }
