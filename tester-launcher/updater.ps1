param(
    [string]$InstallRoot = '',
    [string]$FeedUrl = '',
    [switch]$NoLaunch,
    [switch]$CheckOnly,
    [switch]$Stop
)
$ErrorActionPreference = 'Stop'
if (-not $InstallRoot) { $InstallRoot = $PSScriptRoot }
$install = [IO.Path]::GetFullPath($InstallRoot)
$versions = Join-Path $install 'versions'
$dataRoot = Join-Path $install 'data'
$currentPath = Join-Path $install 'current.json'

function Read-Current {
    if (-not (Test-Path -LiteralPath $currentPath)) { return $null }
    $state = Get-Content -LiteralPath $currentPath -Raw | ConvertFrom-Json
    if ([string]$state.version -notmatch '^Kokoverse-Windows-x64-[A-Za-z0-9][A-Za-z0-9._-]{0,63}$') { throw 'Invalid current version pointer.' }
    $root = Join-Path $versions $state.version
    if (-not (Test-Path -LiteralPath (Join-Path $root 'scripts/start-instance.ps1'))) { throw 'Current application version is incomplete.' }
    return $state
}
function Test-ActiveInstance {
    $runPath = Join-Path $dataRoot '.run'
    if (-not (Test-Path -LiteralPath $runPath)) { return $false }
    foreach ($recordFile in @(Get-ChildItem -LiteralPath $runPath -Filter processes.json -File -Recurse -ErrorAction SilentlyContinue)) {
        try {
            $state = Get-Content -LiteralPath $recordFile.FullName -Raw | ConvertFrom-Json
            foreach ($record in @($state.processes)) {
                $process = Get-Process -Id $record.id -ErrorAction SilentlyContinue
                if ($process -and $process.Path -eq $record.path -and $process.StartTime.ToUniversalTime().Ticks.ToString() -eq $record.startTicks) { return $true }
            }
        } catch { throw "Could not inspect running state: $($recordFile.FullName)" }
    }
    return $false
}
function Save-Current([string]$Version) {
    $temporary = $currentPath + '.tmp'
    $value = @{version=$Version} | ConvertTo-Json -Compress
    [IO.File]::WriteAllText($temporary,$value,(New-Object Text.UTF8Encoding($false)))
    Move-Item -LiteralPath $temporary -Destination $currentPath -Force
}
function Download-File([string]$Url,[string]$Destination) {
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        try {
            Invoke-WebRequest -Uri $Url -OutFile $Destination -UseBasicParsing -TimeoutSec 120 | Out-Null
            return
        } catch {
            if ($attempt -eq 3) { throw }
            Start-Sleep -Seconds $attempt
        }
    }
}
function Test-File([string]$Path,[string]$Hash,[long]$Size) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    $file = Get-Item -LiteralPath $Path
    if ($file.Length -ne $Size) { return $false }
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -eq $Hash
}
function Expand-Blob([string]$Compressed,[string]$Destination,[long]$ExpectedSize) {
    $inputStream = $null; $gzip = $null; $outputStream = $null
    try {
        $inputStream = [IO.File]::OpenRead($Compressed)
        $gzip = New-Object IO.Compression.GZipStream($inputStream,[IO.Compression.CompressionMode]::Decompress,$true)
        $outputStream = [IO.File]::Create($Destination)
        $buffer = New-Object byte[] 65536
        $total = [long]0
        while (($count = $gzip.Read($buffer,0,$buffer.Length)) -gt 0) {
            $total += $count
            if ($total -gt $ExpectedSize) { throw 'Decompressed file exceeds manifest size.' }
            $outputStream.Write($buffer,0,$count)
        }
        if ($total -ne $ExpectedSize) { throw 'Decompressed file size mismatch.' }
    } finally {
        if ($outputStream) { $outputStream.Dispose() }
        if ($gzip) { $gzip.Dispose() }
        if ($inputStream) { $inputStream.Dispose() }
    }
}
function Test-Manifest($Manifest,[string]$Version) {
    if ($Manifest.schema -ne 1 -or $Manifest.version -ne $Version) { throw 'Invalid update manifest.' }
    $entries = @($Manifest.files)
    if ($entries.Count -lt 1 -or $entries.Count -gt 5000) { throw 'Unexpected update file count.' }
    $seen = @{}
    foreach ($entry in $entries) {
        $path = [string]$entry.path
        $hash = [string]$entry.sha256
        if ($path -match '(^|/)\.\.?($|/)' -or $path -notmatch '^[A-Za-z0-9._/+-]+$' -or $path.StartsWith('/')) { throw "Unsafe update path: $path" }
        if ($seen.ContainsKey($path)) { throw "Duplicate update path: $path" }
        $seen[$path] = $true
        if ($hash -notmatch '^[a-f0-9]{64}$' -or [string]$entry.blob -ne "blobs/$hash.gz") { throw "Invalid blob reference: $path" }
        if ([long]$entry.size -lt 0 -or [long]$entry.size -gt 1073741824) { throw "Invalid file size: $path" }
    }
    if (-not $seen.ContainsKey('build-info.json') -or -not $seen.ContainsKey('scripts/start-instance.ps1')) { throw 'Update manifest lacks required application files.' }
}

$mutexHash = [Security.Cryptography.SHA256]::Create()
try { $mutexKey = ([BitConverter]::ToString($mutexHash.ComputeHash([Text.Encoding]::UTF8.GetBytes($install.ToLowerInvariant())))).Replace('-','') }
finally { $mutexHash.Dispose() }
$mutex = New-Object Threading.Mutex($false,('Local\KokoverseUpdater-' + $mutexKey))
$held = $false
try {
    try { $held = $mutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $held = $true }
    if (-not $held) { throw 'Another update is running.' }
    $current = Read-Current
    if ($Stop) {
        if (-not $current) { throw 'No installed version to stop.' }
        $env:KOKOVERSE_DATA_ROOT = $dataRoot
        & (Join-Path (Join-Path $versions $current.version) 'scripts/stop-online.ps1')
        return
    }
    if (-not $FeedUrl) {
        $configPath = Join-Path $install 'config.json'
        if (Test-Path -LiteralPath $configPath) { $FeedUrl = [string](Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json).feedUrl }
    }
    if (-not $FeedUrl) { throw 'Set feedUrl in config.json or pass -FeedUrl.' }
    $base = $FeedUrl.TrimEnd('/')
    if ($base -notmatch '^https://[^?#]+$' -and $base -notmatch '^http://(127\.0\.0\.1|localhost)(:[0-9]+)?(/[^?#]*)?$') { throw 'Update feed must use HTTPS.' }
    New-Item -ItemType Directory -Force -Path $install,$versions,$dataRoot | Out-Null
    $latestFile = Join-Path $install 'latest.download.json'
    $remote = $null
    try {
        Download-File "$base/latest.json" $latestFile
        if ((Get-Item -LiteralPath $latestFile).Length -gt 16384) { throw 'Latest pointer is too large.' }
        $remote = Get-Content -LiteralPath $latestFile -Raw | ConvertFrom-Json
        if ($remote.schema -ne 1 -or [string]$remote.version -notmatch '^Kokoverse-Windows-x64-[A-Za-z0-9][A-Za-z0-9._-]{0,63}$') { throw 'Invalid latest pointer.' }
        if ([string]$remote.manifest -ne "manifests/$($remote.version).json" -or [string]$remote.manifestSha256 -notmatch '^[a-f0-9]{64}$') { throw 'Invalid latest manifest reference.' }
    } catch {
        if (-not $current -or $CheckOnly) { throw }
        Write-Warning "Update check failed; starting installed version: $_"
    } finally {
        if (Test-Path -LiteralPath $latestFile) { Remove-Item -LiteralPath $latestFile }
    }
    if ($remote) {
        Write-Host "Installed: $(if ($current) { $current.version } else { 'none' }); available: $($remote.version)"
        if ($CheckOnly) { return }
        if (-not $current -or $current.version -ne $remote.version) {
            if (Test-ActiveInstance) {
                if (-not $current) { throw 'Application is running but no installed version is recorded.' }
                Write-Host 'An instance is running; update will wait until it exits.'
            } else {
                $manifestFile = Join-Path $install 'manifest.download.json'
                $staging = Join-Path $versions ($remote.version + '.partial-' + [guid]::NewGuid().ToString('N'))
                try {
                    Download-File "$base/$($remote.manifest)" $manifestFile
                    if ((Get-Item -LiteralPath $manifestFile).Length -gt 4194304) { throw 'Manifest is too large.' }
                    if ((Get-FileHash -LiteralPath $manifestFile -Algorithm SHA256).Hash.ToLowerInvariant() -ne $remote.manifestSha256) { throw 'Manifest checksum mismatch.' }
                    $manifest = Get-Content -LiteralPath $manifestFile -Raw | ConvertFrom-Json
                    Test-Manifest $manifest $remote.version
                    $target = Join-Path $versions $remote.version
                    if (-not (Test-Path -LiteralPath $target)) {
                        New-Item -ItemType Directory -Path $staging | Out-Null
                        $oldRoot = if ($current) { Join-Path $versions $current.version } else { '' }
                        $reused = 0; $downloaded = 0
                        foreach ($entry in @($manifest.files)) {
                            $relative = ([string]$entry.path).Replace('/',[IO.Path]::DirectorySeparatorChar)
                            $destination = Join-Path $staging $relative
                            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
                            $oldFile = if ($oldRoot) { Join-Path $oldRoot $relative } else { '' }
                            if ($oldFile -and (Test-File $oldFile $entry.sha256 ([long]$entry.size))) {
                                Copy-Item -LiteralPath $oldFile -Destination $destination
                                $reused++
                            } else {
                                $compressed = $destination + '.download.gz'
                                try {
                                    Download-File "$base/$($entry.blob)" $compressed
                                    Expand-Blob $compressed $destination ([long]$entry.size)
                                } finally {
                                    if (Test-Path -LiteralPath $compressed) { Remove-Item -LiteralPath $compressed }
                                }
                                $downloaded++
                            }
                            if (-not (Test-File $destination $entry.sha256 ([long]$entry.size))) { throw "File checksum mismatch: $($entry.path)" }
                        }
                        Move-Item -LiteralPath $staging -Destination $target
                        Write-Host "Update ready: reused $reused files; downloaded $downloaded files."
                    }
                    foreach ($entry in @($manifest.files)) {
                        $relative = ([string]$entry.path).Replace('/',[IO.Path]::DirectorySeparatorChar)
                        if (-not (Test-File (Join-Path $target $relative) $entry.sha256 ([long]$entry.size))) { throw "Installed version is incomplete: $($entry.path)" }
                    }
                    Save-Current $remote.version
                    $current = Read-Current
                } catch {
                    Write-Warning "Update failed; installed version is unchanged: $_"
                    if (-not $current) { throw }
                } finally {
                    if (Test-Path -LiteralPath $manifestFile) { Remove-Item -LiteralPath $manifestFile }
                    $stagingPrefix = $versions.TrimEnd('\') + [IO.Path]::DirectorySeparatorChar
                    if ($staging.StartsWith($stagingPrefix,[StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $staging)) { Remove-Item -LiteralPath $staging -Recurse -Force }
                }
            }
        }
    }
    if (-not $NoLaunch) {
        if (-not $current) { throw 'No installed version is available.' }
        $env:KOKOVERSE_DATA_ROOT = $dataRoot
        & (Join-Path (Join-Path $versions $current.version) 'scripts/start-instance.ps1')
    }
} finally {
    if ($held) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
}
