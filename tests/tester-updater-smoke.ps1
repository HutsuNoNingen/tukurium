$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$work = Join-Path $repo ('.run/tester-updater-' + [guid]::NewGuid().ToString('N'))
$feed = Join-Path $work 'feed'
$install = Join-Path $work 'install'
New-Item -ItemType Directory -Force -Path $work | Out-Null

function New-TestPackage([string]$Version,[string]$Value) {
    $package = Join-Path $work "Kokoverse-Windows-x64-$Version"
    New-Item -ItemType Directory -Force -Path (Join-Path $package 'scripts'),(Join-Path $package 'bin') | Out-Null
    Set-Content -LiteralPath (Join-Path $package 'scripts/start-instance.ps1') -Value 'Write-Output started' -Encoding UTF8
    Set-Content -LiteralPath (Join-Path $package 'bin/example.txt') -Value $Value -Encoding UTF8
    $files = @(Get-ChildItem -LiteralPath $package -File -Recurse | ForEach-Object {
        @{path=$_.FullName.Substring($package.Length+1).Replace('\','/');size=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}
    })
    @{version=[IO.Path]::GetFileName($package);includesUserWorld=$false;includesCredentials=$false;files=$files} |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $package 'build-info.json') -Encoding UTF8
    & (Join-Path $repo 'scripts/prepare-tester-feed.ps1') -PackageRoot $package -OutputRoot $feed | Out-Host
    return $package
}
function Assert([bool]$Condition,[string]$Message) { if (-not $Condition) { throw $Message } }

$null = New-TestPackage 'tester-v0.0.1' 'first'
$listener = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback,0)
$listener.Start(); $port = $listener.LocalEndpoint.Port; $listener.Stop()
$node = (Get-Command node).Source
$server = Start-Process -FilePath $node -ArgumentList @((Join-Path $PSScriptRoot 'tester-feed-server.mjs'),$feed,"$port") -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $work 'server-out.log') -RedirectStandardError (Join-Path $work 'server-error.log')
try {
    $url = "http://127.0.0.1:$port"
    & (Join-Path $repo 'tester-launcher/updater.ps1') -InstallRoot $install -FeedUrl $url -NoLaunch
    Assert ((Get-Content -LiteralPath (Join-Path $install 'current.json') -Raw | ConvertFrom-Json).version -eq 'Kokoverse-Windows-x64-tester-v0.0.1') 'Initial install failed.'
    $world = Join-Path $install 'data/.run/online/default/worlds/save.json'
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $world) | Out-Null
    Set-Content -LiteralPath $world -Value '{"saved":true}' -Encoding UTF8
    $null = New-TestPackage 'tester-v0.0.2' 'second'
    & (Join-Path $repo 'tester-launcher/updater.ps1') -InstallRoot $install -FeedUrl $url -NoLaunch
    Assert ((Get-Content -LiteralPath (Join-Path $install 'current.json') -Raw | ConvertFrom-Json).version -eq 'Kokoverse-Windows-x64-tester-v0.0.2') 'Second version was not selected.'
    Assert (Test-Path -LiteralPath (Join-Path $install 'versions/Kokoverse-Windows-x64-tester-v0.0.1/bin/example.txt')) 'Previous version was removed.'
    Assert ((Get-Content -LiteralPath $world -Raw) -match 'saved') 'World data was lost.'
    $null = New-TestPackage 'tester-v0.0.3' 'third'
    $active = Get-Process -Id $PID
    $recordPath = Join-Path $install 'data/.run/online/default/processes.json'
    @{processes=@(@{id=$active.Id;path=$active.Path;startTicks=$active.StartTime.ToUniversalTime().Ticks.ToString()})} |
        ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $recordPath -Encoding UTF8
    & (Join-Path $repo 'tester-launcher/updater.ps1') -InstallRoot $install -FeedUrl $url -NoLaunch
    Assert ((Get-Content -LiteralPath (Join-Path $install 'current.json') -Raw | ConvertFrom-Json).version -eq 'Kokoverse-Windows-x64-tester-v0.0.2') 'Running instance did not defer the update.'
    Remove-Item -LiteralPath $recordPath
    $third = Get-Content -LiteralPath (Join-Path $feed 'manifests/Kokoverse-Windows-x64-tester-v0.0.3.json') -Raw | ConvertFrom-Json
    $blob = Join-Path $feed (($third.files | Where-Object path -eq 'bin/example.txt').blob.Replace('/',[IO.Path]::DirectorySeparatorChar))
    Set-Content -LiteralPath $blob -Value 'damaged' -Encoding UTF8
    & (Join-Path $repo 'tester-launcher/updater.ps1') -InstallRoot $install -FeedUrl $url -NoLaunch
    Assert ((Get-Content -LiteralPath (Join-Path $install 'current.json') -Raw | ConvertFrom-Json).version -eq 'Kokoverse-Windows-x64-tester-v0.0.2') 'Failed update replaced the installed version.'
    Assert ((Get-Content -LiteralPath $world -Raw) -match 'saved') 'World data was lost after a failed update.'
    Write-Output 'PASS: initial install, file reuse, preserved world, active-instance deferral, previous version and failed update rollback'
} finally {
    Stop-Process -Id $server.Id -ErrorAction SilentlyContinue
}
