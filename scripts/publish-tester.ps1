param(
    [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$')][string]$Tag,
    [string]$Remote = '',
    [string]$PublicBaseUrl = '',
    [string]$RcloneConfig = '',
    [string]$LocalConfigPath = '',
    [switch]$CreateTag,
    [switch]$PushTag,
    [switch]$PrepareOnly,
    [string]$OutputDirectory = '',
    [string]$PackageRoot = ''
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Push-Location $repo
try {
    if (-not $LocalConfigPath) { $LocalConfigPath = Join-Path $repo '.run/tester-publish.local.psd1' }
    if (Test-Path -LiteralPath $LocalConfigPath) {
        $localConfig = Import-PowerShellDataFile -LiteralPath $LocalConfigPath
        if (-not $Remote) { $Remote = [string]$localConfig.Remote }
        if (-not $PublicBaseUrl) { $PublicBaseUrl = [string]$localConfig.PublicBaseUrl }
        if (-not $RcloneConfig) { $RcloneConfig = [string]$localConfig.RcloneConfig }
    }
    if (-not $Remote) { $Remote = $env:KOKOVERSE_TESTER_REMOTE }
    if (-not $PublicBaseUrl) { $PublicBaseUrl = $env:KOKOVERSE_TESTER_PUBLIC_BASE_URL }
    if (-not $RcloneConfig) { $RcloneConfig = $env:RCLONE_CONFIG }
    $status = @(git status --porcelain --untracked-files=all)
    if ($LASTEXITCODE -ne 0) { throw 'Git status failed.' }
    if ($status.Count -ne 0) { throw 'Commit or remove working-tree changes before publishing a tagged build.' }
    $commit = (git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Could not read HEAD.' }
    $existingTag = @(git tag --list $Tag)
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect Git tags.' }
    if ($existingTag.Count -gt 0) {
        $tagCommit = (git rev-list -n 1 "refs/tags/$Tag").Trim()
        if ($tagCommit -ne $commit) { throw "Tag $Tag does not point to HEAD." }
    } elseif (-not $CreateTag) {
        throw "Tag $Tag does not exist. Use -CreateTag after committing."
    }
    if (-not $PrepareOnly) {
        if (-not $Remote -or -not $PublicBaseUrl) { throw 'Remote and PublicBaseUrl are required for upload.' }
        if ($PublicBaseUrl -notmatch '^https://[^?#]+$') { throw 'PublicBaseUrl must be an HTTPS URL.' }
        $rcloneCommand = Get-Command rclone -ErrorAction SilentlyContinue
        $rcloneExe = if ($rcloneCommand) { $rcloneCommand.Source } else { '' }
        if (-not $rcloneExe) {
            foreach ($directory in ([Environment]::GetEnvironmentVariable('Path','User') -split ';')) {
                if (-not $directory) { continue }
                $candidate = Join-Path $directory 'rclone.exe'
                if (Test-Path -LiteralPath $candidate) { $rcloneExe = $candidate; break }
            }
        }
        if (-not $rcloneExe) { throw 'rclone is required to upload to R2.' }
        $rcloneOptions = @()
        if ($RcloneConfig) {
            if (-not (Test-Path -LiteralPath $RcloneConfig)) { throw 'RcloneConfig does not exist.' }
            $rcloneOptions = @('--config', $RcloneConfig)
        }
        & $rcloneExe lsf $Remote --max-depth 1 @rcloneOptions | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Could not access the R2 destination.' }
    }
    if (-not $OutputDirectory) { $OutputDirectory = Join-Path $repo 'dist' }
    if (-not $PackageRoot) {
        $packageOutput = @( & (Join-Path $PSScriptRoot 'package.ps1') -OutputDirectory $OutputDirectory -Version $Tag )
        $packageOutput | Write-Output
        $rootLine = $packageOutput | Where-Object { "$_" -like 'PACKAGE_ROOT=*' } | Select-Object -Last 1
        if (-not $rootLine) { throw 'Packaging did not return PACKAGE_ROOT.' }
        $PackageRoot = ([string]$rootLine).Substring('PACKAGE_ROOT='.Length)
    }
    $postPackageStatus = @(git status --porcelain --untracked-files=all)
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect source status after packaging.' }
    if ($postPackageStatus.Count -ne 0) {
        throw 'Packaging changed the source tree. Commit generated source files before publishing a tagged build.'
    }
    $packageInfo = Get-Content -LiteralPath (Join-Path $PackageRoot 'build-info.json') -Raw | ConvertFrom-Json
    if ($packageInfo.version -ne "Kokoverse-Windows-x64-$Tag" -or $packageInfo.sourceCommit -ne $commit -or $packageInfo.sourceTreeClean -ne $true) {
        throw 'Package version or recorded source does not match the clean tagged commit.'
    }
    $feedRoot = Join-Path $repo '.run/tester-feed'
    & (Join-Path $PSScriptRoot 'prepare-tester-feed.ps1') -PackageRoot $PackageRoot -OutputRoot $feedRoot
    if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) { throw 'Feed preparation failed.' }
    if ($PushTag) {
        $branch = [string](& git symbolic-ref --quiet --short HEAD)
        if ($LASTEXITCODE -ne 0 -or -not $branch) { throw 'Checkout a branch before pushing the release commit.' }
        & git push origin ("HEAD:refs/heads/" + $branch)
        if ($LASTEXITCODE -ne 0) { throw 'Could not push the release commit to origin. R2 was not changed.' }
        $remoteBranch = [string](& git ls-remote origin ("refs/heads/" + $branch))
        if ($LASTEXITCODE -ne 0 -or -not $remoteBranch -or ($remoteBranch -split "`t")[0] -ne $commit) {
            throw 'The remote branch does not point to the release commit. R2 was not changed.'
        }
    }
    if ($existingTag.Count -eq 0) {
        & git tag -a $Tag -m "Kokoverse tester $Tag"
        if ($LASTEXITCODE -ne 0) { throw 'Could not create Git tag.' }
    }
    if ($PushTag) {
        & git push origin "refs/tags/$Tag"
        if ($LASTEXITCODE -ne 0) { throw 'Could not push Git tag.' }
    }
    if ($PrepareOnly) { Write-Output "PREPARED_TAG=$Tag"; return }
    $launcherName = "Kokoverse-Tester-Launcher-$Tag"
    $launcherRoot = Join-Path $OutputDirectory $launcherName
    if (-not (Test-Path -LiteralPath $launcherRoot)) {
        New-Item -ItemType Directory -Path $launcherRoot | Out-Null
        foreach ($name in @('Start.bat','Stop.bat','updater.ps1','config.example.json')) {
            Copy-Item -LiteralPath (Join-Path (Join-Path $repo 'tester-launcher') $name) -Destination $launcherRoot
        }
        @{feedUrl=$PublicBaseUrl.TrimEnd('/')} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $launcherRoot 'config.json') -Encoding UTF8
    } elseif ([string](Get-Content -LiteralPath (Join-Path $launcherRoot 'config.json') -Raw | ConvertFrom-Json).feedUrl -ne $PublicBaseUrl.TrimEnd('/')) {
        throw 'Existing launcher bundle uses a different feed URL.'
    }
    $launcherZip = Join-Path $OutputDirectory ($launcherName + '.zip')
    if (-not (Test-Path -LiteralPath $launcherZip)) {
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        [IO.Compression.ZipFile]::CreateFromDirectory($launcherRoot,$launcherZip,[IO.Compression.CompressionLevel]::Optimal,$false)
    }
    $remotePath = $Remote.TrimEnd('/')
    & $rcloneExe copy (Join-Path $feedRoot 'blobs') "$remotePath/blobs" --ignore-existing @rcloneOptions
    if ($LASTEXITCODE -ne 0) { throw 'Blob upload failed. latest.json was not changed.' }
    $version = "Kokoverse-Windows-x64-$Tag"
    $manifestPath = Join-Path $feedRoot "manifests/$version.json"
    & $rcloneExe copyto $manifestPath "$remotePath/manifests/$version.json" --immutable @rcloneOptions
    if ($LASTEXITCODE -ne 0) { throw 'Manifest upload failed. latest.json was not changed.' }
    $publicManifest = Join-Path $feedRoot 'public-manifest-check.tmp'
    try {
        $verified = $false
        for ($attempt = 1; $attempt -le 3; $attempt++) {
            try {
                Invoke-WebRequest -Uri "$($PublicBaseUrl.TrimEnd('/'))/manifests/$version.json" -OutFile $publicManifest -UseBasicParsing -TimeoutSec 20 | Out-Null
                $verified = (Get-FileHash -LiteralPath $publicManifest -Algorithm SHA256).Hash -eq (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash
                if ($verified) { break }
            } catch {}
            Start-Sleep -Seconds 2
        }
        if (-not $verified) { throw 'Public manifest URL is unavailable or does not match R2 upload. latest.json was not changed.' }
    } finally {
        if (Test-Path -LiteralPath $publicManifest) { Remove-Item -LiteralPath $publicManifest }
    }
    & $rcloneExe copyto (Join-Path $feedRoot 'latest.json') "$remotePath/latest.json" --ignore-times @rcloneOptions
    if ($LASTEXITCODE -ne 0) { throw 'Latest pointer upload failed.' }
    Write-Output "PUBLISHED_TAG=$Tag"
    Write-Output "PUBLISHED_URL=$($PublicBaseUrl.TrimEnd('/'))/latest.json"
    Write-Output "LAUNCHER_ZIP=$launcherZip"
} finally { Pop-Location }
