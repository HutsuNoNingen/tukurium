$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$destination = Join-Path $projectRoot '.tools/ffmpeg'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
$archive = Join-Path $destination 'download.zip'
$expected = '60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba'
if (-not (Test-Path -LiteralPath $archive) -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected) {
    $partial = Join-Path $destination ('download-' + [guid]::NewGuid().ToString('N') + '.partial')
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest 'https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.2-essentials_build.zip' -UseBasicParsing -TimeoutSec 600 -OutFile $partial
        if ((Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash -ne $expected) { throw 'FFmpeg 9.0.2 archive checksum mismatch' }
        Move-Item -LiteralPath $partial -Destination $archive -Force
    } finally {
        if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial }
    }
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected) { throw 'FFmpeg 9.0.2 archive checksum mismatch' }
$extracted = Join-Path $destination 'extracted'
Expand-Archive -LiteralPath $archive -DestinationPath $extracted -Force
$binary = Get-ChildItem -LiteralPath $extracted -Filter ffmpeg.exe -Recurse | Select-Object -First 1
if (-not $binary) { throw 'FFmpeg executable was not found in the archive' }
Copy-Item -LiteralPath $binary.FullName -Destination (Join-Path $destination 'ffmpeg.exe') -Force
Copy-Item -LiteralPath (Join-Path $binary.DirectoryName 'ffprobe.exe') -Destination (Join-Path $destination 'ffprobe.exe') -Force
$license = Join-Path $binary.Directory.Parent.FullName 'LICENSE'
if (Test-Path -LiteralPath $license) { Copy-Item -LiteralPath $license -Destination (Join-Path $destination 'LICENSE') -Force }
Write-Host 'FFmpeg 9.0.2 video import tools are ready.'
