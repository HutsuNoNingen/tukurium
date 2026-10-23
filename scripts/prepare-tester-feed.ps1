param(
    [Parameter(Mandatory)][string]$PackageRoot,
    [Parameter(Mandatory)][string]$OutputRoot
)
$ErrorActionPreference = 'Stop'
$packagePath = (Resolve-Path -LiteralPath $PackageRoot).Path.TrimEnd('\')
$feedPath = [IO.Path]::GetFullPath($OutputRoot)
$buildInfoPath = Join-Path $packagePath 'build-info.json'
if (-not (Test-Path -LiteralPath $buildInfoPath)) { throw 'Package has no build-info.json.' }
$buildInfo = Get-Content -LiteralPath $buildInfoPath -Raw | ConvertFrom-Json
$version = [string]$buildInfo.version
if ($version -notmatch '^Kokoverse-Windows-x64-[A-Za-z0-9][A-Za-z0-9._-]{0,63}$') { throw 'Invalid package version.' }
if ([IO.Path]::GetFileName($packagePath) -ne $version) { throw 'Package directory and build-info version disagree.' }
if ($buildInfo.includesUserWorld -or $buildInfo.includesCredentials) { throw 'Refusing to publish personal data or credentials.' }

$filesOnDisk = @(Get-ChildItem -LiteralPath $packagePath -Recurse -File -Force)
$expected = @{}
foreach ($entry in @($buildInfo.files)) {
    $path = [string]$entry.path
    if ($path -match '(^|/|\\)\.\.?($|/|\\)' -or $path -match '^[A-Za-z]:' -or $path.StartsWith('/') -or $path.StartsWith('\\') -or $path -notmatch '^[A-Za-z0-9._/+-]+$') { throw "Unsafe package path: $path" }
    if ($expected.ContainsKey($path)) { throw "Duplicate package path: $path" }
    $expected[$path] = $entry
}
if ($expected.ContainsKey('build-info.json')) { throw 'build-info.json must not list itself.' }
$expected['build-info.json'] = @{path='build-info.json';size=(Get-Item -LiteralPath $buildInfoPath).Length;sha256=(Get-FileHash -LiteralPath $buildInfoPath -Algorithm SHA256).Hash.ToLowerInvariant()}
if ($filesOnDisk.Count -ne $expected.Count) { throw "Package file count mismatch: disk=$($filesOnDisk.Count), manifest=$($expected.Count)." }

$blobsPath = Join-Path $feedPath 'blobs'
$manifestsPath = Join-Path $feedPath 'manifests'
New-Item -ItemType Directory -Force -Path $blobsPath,$manifestsPath | Out-Null
$resultFiles = @()
foreach ($file in $filesOnDisk) {
    if (($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Package contains a link: $($file.FullName)" }
    $relative = $file.FullName.Substring($packagePath.Length + 1).Replace('\','/')
    if (-not $expected.ContainsKey($relative)) { throw "Unlisted package file: $relative" }
    $entry = $expected[$relative]
    $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne [string]$entry.sha256 -or $file.Length -ne [long]$entry.size) { throw "Package changed after build: $relative" }
    $blob = Join-Path $blobsPath ($hash + '.gz')
    if (-not (Test-Path -LiteralPath $blob)) {
        $temporary = $blob + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
        $inputStream = $null; $outputStream = $null; $gzip = $null
        try {
            $inputStream = [IO.File]::OpenRead($file.FullName)
            $outputStream = [IO.File]::Create($temporary)
            $gzip = New-Object IO.Compression.GZipStream($outputStream,[IO.Compression.CompressionLevel]::Optimal,$true)
            $inputStream.CopyTo($gzip)
            $gzip.Dispose(); $gzip = $null
            $outputStream.Dispose(); $outputStream = $null
            Move-Item -LiteralPath $temporary -Destination $blob
        } finally {
            if ($gzip) { $gzip.Dispose() }
            if ($outputStream) { $outputStream.Dispose() }
            if ($inputStream) { $inputStream.Dispose() }
            if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary }
        }
    }
    $resultFiles += [ordered]@{path=$relative;size=[long]$file.Length;sha256=$hash;blob="blobs/$hash.gz"}
}
$manifest = [ordered]@{schema=1;version=$version;files=@($resultFiles | Sort-Object { $_.path })}
$manifestPath = Join-Path $manifestsPath ($version + '.json')
$manifestText = ConvertTo-Json -InputObject $manifest -Depth 5
if (Test-Path -LiteralPath $manifestPath) {
    if ((Get-Content -LiteralPath $manifestPath -Raw) -ne ($manifestText + [Environment]::NewLine)) { throw "Version already has a different manifest: $version" }
} else {
    [IO.File]::WriteAllText($manifestPath,$manifestText + [Environment]::NewLine,(New-Object Text.UTF8Encoding($false)))
}
$manifestHash = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
$latest = [ordered]@{schema=1;version=$version;manifest="manifests/$version.json";manifestSha256=$manifestHash}
[IO.File]::WriteAllText((Join-Path $feedPath 'latest.json'),((ConvertTo-Json -InputObject $latest -Depth 3) + [Environment]::NewLine),(New-Object Text.UTF8Encoding($false)))
Write-Output "FEED_ROOT=$feedPath"
Write-Output "FEED_VERSION=$version"
Write-Output "FEED_FILES=$($resultFiles.Count)"
