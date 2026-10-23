param(
    [string]$DestinationDirectory = '',
    [string]$CacheDirectory = ''
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
if (-not $DestinationDirectory) { $DestinationDirectory = Join-Path $projectRoot '.tools/ffmpeg' }
if (-not $CacheDirectory) { $CacheDirectory = Join-Path $env:LOCALAPPDATA 'Kokoverse/tool-cache' }

$version = '9.0.2'
$archiveName = "ffmpeg-$version-essentials_build.zip"
$archiveHash = '60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba'
$binaryHashes = @{
    'ffmpeg.exe' = '3256173f3f8bffd7df12227c68adf68025edb1832273a9530688a7bb1ed8edec'
    'ffprobe.exe' = 'f0d36ecbbdd3bcfac3efa078c96c7271c2e68b3810595552ac3b7f17e9a65c52'
}

function Test-FileHash([string]$Path, [string]$ExpectedHash) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -eq $ExpectedHash
}

New-Item -ItemType Directory -Force -Path $DestinationDirectory | Out-Null
$ffmpegPath = Join-Path $DestinationDirectory 'ffmpeg.exe'
$ffprobePath = Join-Path $DestinationDirectory 'ffprobe.exe'
$licensePath = Join-Path $DestinationDirectory 'LICENSE'
if ((Test-FileHash $ffmpegPath $binaryHashes['ffmpeg.exe']) -and
    (Test-FileHash $ffprobePath $binaryHashes['ffprobe.exe']) -and
    (Test-Path -LiteralPath $licensePath -PathType Leaf)) {
    Write-Host "FFmpeg $version video import tools are already installed."
    return
}

New-Item -ItemType Directory -Force -Path $CacheDirectory | Out-Null
$archive = Join-Path $CacheDirectory $archiveName
$oldArchive = Join-Path $DestinationDirectory 'download.zip'
if (-not (Test-FileHash $archive $archiveHash)) {
    if (Test-FileHash $oldArchive $archiveHash) {
        Copy-Item -LiteralPath $oldArchive -Destination $archive -Force
    } else {
        $sources = @(
            "https://github.com/GyanD/codexffmpeg/releases/download/$version/$archiveName",
            "https://www.gyan.dev/ffmpeg/builds/packages/$archiveName"
        )
        $downloaded = $false
        foreach ($source in $sources) {
            $partial = Join-Path $CacheDirectory ($archiveName + '.' + [guid]::NewGuid().ToString('N') + '.partial')
            try {
                Write-Host "Downloading FFmpeg $version from $(([uri]$source).Host)..."
                & curl.exe --location --fail --silent --connect-timeout 15 --max-time 600 --output $partial $source
                if ($LASTEXITCODE -ne 0) { throw "curl.exe failed with exit code $LASTEXITCODE" }
                if (-not (Test-FileHash $partial $archiveHash)) { throw 'Archive SHA-256 mismatch' }
                Move-Item -LiteralPath $partial -Destination $archive -Force
                $downloaded = $true
                break
            } catch {
                Write-Warning "Download from $(([uri]$source).Host) failed: $_"
            } finally {
                if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial -Force }
            }
        }
        if (-not $downloaded) { throw 'Could not download and verify the FFmpeg archive from either source.' }
    }
}

if (-not (Test-FileHash $archive $archiveHash)) { throw "FFmpeg $version archive checksum mismatch" }

Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
$prefix = "ffmpeg-$version-essentials_build/"
$files = @(
    @{ Entry = "${prefix}bin/ffmpeg.exe"; Name = 'ffmpeg.exe'; Hash = $binaryHashes['ffmpeg.exe'] },
    @{ Entry = "${prefix}bin/ffprobe.exe"; Name = 'ffprobe.exe'; Hash = $binaryHashes['ffprobe.exe'] },
    @{ Entry = "${prefix}LICENSE"; Name = 'LICENSE'; Hash = '' }
)
$staged = @()
try {
    foreach ($file in $files) {
        $entry = $zip.GetEntry($file.Entry)
        if ($null -eq $entry) { throw "Missing FFmpeg archive entry: $($file.Entry)" }
        $temporary = Join-Path $DestinationDirectory ($file.Name + '.' + [guid]::NewGuid().ToString('N') + '.partial')
        $staged += @{ Temporary = $temporary; Destination = (Join-Path $DestinationDirectory $file.Name) }
        $inputStream = $entry.Open()
        try {
            $outputStream = [IO.File]::Create($temporary)
            try { $inputStream.CopyTo($outputStream) }
            finally { $outputStream.Dispose() }
        } finally { $inputStream.Dispose() }
        if ($file.Hash -and -not (Test-FileHash $temporary $file.Hash)) {
            throw "FFmpeg executable checksum mismatch: $($file.Name)"
        }
    }
    foreach ($file in $staged) {
        Move-Item -LiteralPath $file.Temporary -Destination $file.Destination -Force
    }
} finally {
    $zip.Dispose()
    foreach ($file in $staged) {
        if (Test-Path -LiteralPath $file.Temporary) { Remove-Item -LiteralPath $file.Temporary -Force }
    }
}
Write-Host "FFmpeg $version video import tools are ready."
