$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    New-Item -ItemType Directory -Force .tools | Out-Null
    if (-not (Test-Path .tools/godot/Godot_v4.7.2-stable_win64_console.exe)) {
        Invoke-WebRequest 'https://github.com/godotengine/godot-builds/releases/download/4.7.2-stable/Godot_v4.7.2-stable_win64.exe.zip' -OutFile .tools/godot.zip
        Expand-Archive .tools/godot.zip .tools/godot -Force
    }
    if (-not (Test-Path .tools/w64devkit/bin/gcc.exe)) {
        Invoke-WebRequest 'https://github.com/skeeto/w64devkit/releases/download/v2.10.1/w64devkit-x64-2.10.1.7z.exe' -OutFile .tools/w64devkit.exe
        & .tools/w64devkit.exe -y -o.tools
        if ($LASTEXITCODE -ne 0) { throw 'w64devkit extraction failed' }
    }
    . ./scripts/env.ps1
    if (-not (Test-Path .tools/cargo/bin/rustup.exe)) {
        Invoke-WebRequest 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe' -OutFile .tools/rustup-init.exe
        & .tools/rustup-init.exe -y --no-modify-path --profile minimal --default-host x86_64-pc-windows-gnu --default-toolchain 1.99.1
        if ($LASTEXITCODE -ne 0) { throw 'Rust setup failed' }
    }
    # w64devkit includes EH symbols in libgcc; Rust also expects a separate archive.
    $ehArchive = Join-Path $projectRoot '.tools/w64devkit/lib/gcc/x86_64-w64-mingw32/16.2.0/libgcc_eh.a'
    if (-not (Test-Path $ehArchive)) { & .tools/w64devkit/bin/ar.exe rc $ehArchive }
    if (-not (Test-Path .tools/go/bin/go.exe)) {
        Invoke-WebRequest 'https://go.dev/dl/go1.27.3.windows-amd64.zip' -OutFile .tools/go.zip
        Expand-Archive .tools/go.zip .tools -Force
    }
    & rustup component add rustfmt clippy
    if ($LASTEXITCODE -ne 0) { throw 'Rust component setup failed' }
    if (-not (Test-Path .tools/ffmpeg/ffmpeg.exe)) { & ./scripts/setup-media.ps1 }
} finally { Pop-Location }
