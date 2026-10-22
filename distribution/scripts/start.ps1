param(
    [ValidateSet('Auto','Host','Join')][string]$Mode = 'Auto',
    [string]$Username = '',
    [switch]$Headless,
    [switch]$SkipMedia,
    [int]$BasePort = 0
)
$ErrorActionPreference = 'Stop'
$packageRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $packageRoot
$settings = Get-Content -LiteralPath (Join-Path $packageRoot 'connection.json') -Raw | ConvertFrom-Json
if ($settings.relay -notmatch '^wss://[^/]+/ws$' -or $settings.session -notmatch '^[A-Za-z0-9_-]{1,80}$') { throw 'Invalid connection.json' }
if ($BasePort -ne 0 -and ($BasePort -lt 1024 -or $BasePort -gt 65534)) { throw 'Invalid local port' }
$runDirectory = Join-Path $packageRoot '.run'
New-Item -ItemType Directory -Path $runDirectory -Force | Out-Null
function Write-Startup([string]$Message) {
    Write-Host $Message
    Add-Content -LiteralPath (Join-Path $runDirectory 'startup.log') -Value ((Get-Date -Format 's') + ' ' + $Message) -Encoding UTF8
}
Write-Startup '[1/4] Preparing Kokoverse...'
$recordPath = Join-Path $runDirectory 'processes.json'
if (Test-Path -LiteralPath $recordPath) {
    $records = Get-Content -LiteralPath $recordPath -Raw | ConvertFrom-Json
    $alive = @($records | Where-Object {
        $process = Get-Process -Id $_.id -ErrorAction SilentlyContinue
        $process -and $process.Path -eq $_.path -and $process.StartTime.ToUniversalTime().Ticks.ToString() -eq $_.startTicks
    })
    if ($alive.Count -gt 0) { throw 'Kokoverse is already running from this folder. Use Stop.bat first.' }
    Remove-Item -LiteralPath $recordPath
}

$profilePath = Join-Path $runDirectory 'profile.json'
if ([string]::IsNullOrWhiteSpace($Username)) {
    $initialName = if (Test-Path -LiteralPath $profilePath) { (Get-Content -LiteralPath $profilePath -Raw -Encoding UTF8 | ConvertFrom-Json).name } else { '' }
    . (Join-Path $PSScriptRoot 'login.ps1')
    $Username = Get-PlayerName $initialName
}
$Username = $Username.Trim()
if ($Username.Length -eq 0 -or $Username.Length -gt 40 -or $Username -match '[\r\n\x00]') { throw 'Enter a name from 1 to 40 characters.' }
@{name=$Username} | ConvertTo-Json | Set-Content -LiteralPath $profilePath -Encoding UTF8

$bundledLogin = Join-Path $packageRoot 'relay-access.json'
if ((-not $env:KOKOSYNC_USERNAME -or -not $env:KOKOSYNC_PASSWORD) -and (Test-Path -LiteralPath $bundledLogin)) {
    $access = Get-Content -LiteralPath $bundledLogin -Raw | ConvertFrom-Json
    if (-not $access.username -or -not $access.password) { throw 'Bundled relay login is incomplete.' }
    $env:KOKOSYNC_USERNAME = $access.username
    $env:KOKOSYNC_PASSWORD = $access.password
    Write-Startup 'Using the bundled relay login.'
}
if (-not $env:KOKOSYNC_USERNAME -or -not $env:KOKOSYNC_PASSWORD) {
    $credentialPath = Join-Path $runDirectory 'relay-login.xml'
    $credential = $null
    if (Test-Path -LiteralPath $credentialPath) {
        try { $credential = Import-Clixml -LiteralPath $credentialPath } catch { $credential = $null }
    }
    if (-not $credential) {
        Write-Startup '[2/4] Waiting for sign-in. A Kokoverse login window is open.'
        . (Join-Path $PSScriptRoot 'login.ps1')
        $credential = Get-RelayLogin
        if (-not $credential) { throw 'Login cancelled.' }
        $credential | Export-Clixml -LiteralPath $credentialPath
    }
    $env:KOKOSYNC_USERNAME = $credential.UserName
    $env:KOKOSYNC_PASSWORD = $credential.GetNetworkCredential().Password
}

# Validate the password before starting background processes. Never print it.
Write-Startup '[2/4] Checking the relay connection (up to 15 seconds)...'
$auth = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($env:KOKOSYNC_USERNAME + ':' + $env:KOKOSYNC_PASSWORD))
$health = ([uri]$settings.relay).GetLeftPart([System.UriPartial]::Authority).Replace('wss://','https://') + '/health'
try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri $health -Headers @{ Authorization = 'Basic ' + $auth } -UseBasicParsing -TimeoutSec 15 | Out-Null
} catch {
    throw 'Relay login or connection failed. Check your connection. Delete .run/relay-login.xml to enter credentials again.'
} finally { $auth = $null }

$peerPath = Join-Path $runDirectory 'peer-id.txt'
$peerFolderPath = Join-Path $runDirectory 'peer-folder.txt'
$peerFolder = [IO.Path]::GetFullPath($packageRoot).ToLowerInvariant()
$knownFolder = if (Test-Path -LiteralPath $peerFolderPath) { (Get-Content -LiteralPath $peerFolderPath -Raw -Encoding UTF8).Trim() } else { '' }
if (-not (Test-Path -LiteralPath $peerPath) -or $knownFolder -ne $peerFolder) {
    ('guest-' + [guid]::NewGuid().ToString('N')) | Set-Content -LiteralPath $peerPath -Encoding Ascii
    $peerFolder | Set-Content -LiteralPath $peerFolderPath -Encoding UTF8
}
$peer = if ($Mode -eq 'Host') { $settings.host_peer } elseif ($Mode -eq 'Auto') { 'auto' } else { (Get-Content -LiteralPath $peerPath -Raw).Trim() }
$role = if ($Mode -eq 'Host') { 'host' } elseif ($Mode -eq 'Auto') { 'auto' } else { 'guest' }
$started = [System.Collections.Generic.List[object]]::new()
function Start-PackageProcess([string]$Name,[string]$Path,[string[]]$Arguments,[bool]$Visible = $false) {
    $fullPath = (Resolve-Path -LiteralPath $Path).Path
    $quotedArguments = @($Arguments | ForEach-Object {
        $escaped = [regex]::Replace($_,'(\\*)"','$1$1\"')
        $escaped = [regex]::Replace($escaped,'(\\+)$','$1$1')
        '"' + $escaped + '"'
    })
    $options = @{ FilePath=$fullPath; ArgumentList=$quotedArguments; WorkingDirectory=$packageRoot; PassThru=$true }
    if (-not $Visible) {
        $options.WindowStyle = 'Hidden'
        $options.RedirectStandardOutput = Join-Path $runDirectory ($Name + '.log')
        $options.RedirectStandardError = Join-Path $runDirectory ($Name + '-error.log')
    }
    $process = Start-Process @options
    $started.Add(@{ id=$process.Id; path=$fullPath; startTicks=$process.StartTime.ToUniversalTime().Ticks.ToString() })
}
$portLock = New-Object Threading.Mutex($false,'Local\Kokoverse-LauncherPorts')
$portLockHeld = $false
try {
    Write-Startup '[3/4] Starting the engine and network adapter...'
    try { $portLockHeld = $portLock.WaitOne(10000) } catch [Threading.AbandonedMutexException] { $portLockHeld = $true }
    if (-not $portLockHeld) { throw 'Another Kokoverse instance is starting. Please retry in a few seconds.' }
    $reservationA = $null
    $reservationB = $null
    try {
        $reservationA = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback,$BasePort)
        $reservationA.Server.ExclusiveAddressUse = $true
        $reservationA.Start()
        $transportRequested = if ($BasePort -eq 0) { 0 } else { $BasePort + 1 }
        $reservationB = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback,$transportRequested)
        $reservationB.Server.ExclusiveAddressUse = $true
        $reservationB.Start()
        $BasePort = $reservationA.LocalEndpoint.Port
        $transportPort = $reservationB.LocalEndpoint.Port
    } finally {
        if ($reservationA) { $reservationA.Stop() }
        if ($reservationB) { $reservationB.Stop() }
    }
    Write-Startup "Local ports: renderer=$BasePort adapter=$transportPort"
    $assetRoot = Join-Path $runDirectory 'assets'
    $sharedAssets = Join-Path $assetRoot 'shared'
    New-Item -ItemType Directory -Path $sharedAssets -Force | Out-Null
    # Preserve assets cached by older releases whose cache changed with the IPC port.
    Get-ChildItem -LiteralPath $assetRoot -Directory | Where-Object { $_.Name -match '^\d+$' } | ForEach-Object {
        Get-ChildItem -LiteralPath $_.FullName -File | Where-Object { $_.Name -match '^[a-f0-9]{64}\.(glb|ogv|png)$' } | ForEach-Object {
            $destination = Join-Path $sharedAssets $_.Name
            if (-not (Test-Path -LiteralPath $destination)) { Copy-Item -LiteralPath $_.FullName -Destination $destination }
        }
    }
    $engineArgs = @('--role',$role,'--name',$Username,'--session',$settings.session,'--peer',$peer,'--host-peer',$settings.host_peer,'--port',"$BasePort",'--transport-port',"$transportPort",'--save',(Join-Path $runDirectory 'world.json'),'--asset-dir',$sharedAssets)
    Start-PackageProcess 'engine' 'bin/kokoverse-engine.exe' $engineArgs
    $listening = $false
    for ($attempt=0; $attempt -lt 50; $attempt++) {
        if (-not (Get-Process -Id $started[0].id -ErrorAction SilentlyContinue)) {
            $details = Get-Content -LiteralPath (Join-Path $runDirectory 'engine-error.log') -Raw -ErrorAction SilentlyContinue
            throw "Engine could not start: $details"
        }
        $probe = New-Object Net.Sockets.TcpClient
        try { $probe.Connect('127.0.0.1',$transportPort); $listening = $true } catch { } finally { $probe.Close() }
        if ($listening) { break }
        Start-Sleep -Milliseconds 100
    }
    if (-not $listening) { throw 'Engine startup timed out. Check .run/engine-error.log.' }
    $portLock.ReleaseMutex()
    $portLockHeld = $false
    Start-PackageProcess 'adapter' 'bin/kokosync-adapter.exe' @('--session',$settings.session,'--engine',"127.0.0.1:$transportPort",'--address',$settings.relay,'--peer',$peer)
    Start-Sleep -Milliseconds 200
    foreach ($record in $started) {
        if (-not (Get-Process -Id $record.id -ErrorAction SilentlyContinue)) { throw 'A startup process exited. Check .run/engine-error.log and .run/adapter-error.log.' }
    }
    if (-not $Headless) {
        Write-Startup '[4/4] Opening the Godot window...'
        Start-PackageProcess 'client' '.tools/godot/Godot_v4.7.2-stable_win64.exe' @('--path','client-godot','--',"--port=$BasePort","--session=$($settings.session)") $true
    }
    Remove-Item Env:KOKOSYNC_PASSWORD -ErrorAction SilentlyContinue
    if (-not $SkipMedia -and -not (Test-Path -LiteralPath '.tools/ffmpeg/ffmpeg.exe')) {
        Write-Startup 'Video import tools are downloading in the background. You can use the world now.'
        $mediaScript = Join-Path $PSScriptRoot 'setup-media.ps1'
        Start-PackageProcess 'media-setup' "$env:WINDIR/System32/WindowsPowerShell/v1.0/powershell.exe" @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',$mediaScript)
    }
    @{ui_port=$BasePort;transport_port=$transportPort;role=$role;peer=$peer;session=$settings.session;autosave=$true} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runDirectory 'connection.json') -Encoding UTF8
    ConvertTo-Json -InputObject @($started.ToArray()) | Set-Content -LiteralPath $recordPath -Encoding UTF8
    Write-Startup "Kokoverse started as $role. Relay: $($settings.relay), room: kokoverse/$($settings.session)"
    Write-Host 'The host saves the world automatically. Use Stop.bat to save and exit. The host must stay connected while others edit.'
} catch {
    foreach ($record in $started) { Stop-Process -Id $record.id -ErrorAction SilentlyContinue }
    throw
} finally {
    if ($portLockHeld) { $portLock.ReleaseMutex() }
    $portLock.Dispose()
    Remove-Item Env:KOKOSYNC_PASSWORD -ErrorAction SilentlyContinue
}
