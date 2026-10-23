param([ValidatePattern('^[A-Za-z0-9_-]{1,40}$')][string]$Profile = 'default', [switch]$Build, [switch]$Headless,
    [string]$Server = 'http://127.0.0.1:8787', [string]$Relay = '')
$ErrorActionPreference = 'Stop'
$localRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $localRoot
. (Join-Path $PSScriptRoot 'process-lifecycle.ps1')
$Server = $Server.TrimEnd('/')
$isLocal = $Server -eq 'http://127.0.0.1:8787'
if (-not $isLocal -and ([uri]$Server).Scheme -ne 'https') { throw 'Remote service requires HTTPS.' }
$runRoot = Join-Path $localRoot $(if ($isLocal) { '.run/local' } else { '.run/online' })
$clientRoot = Join-Path $runRoot $Profile
$recordsPath = Join-Path $clientRoot 'processes.json'
New-Item -ItemType Directory -Force -Path $clientRoot | Out-Null
$launchMutex = New-KokoverseLaunchMutex $localRoot $Profile (-not $isLocal)
$locked = $false
$newClients = @()
$processes = @()
$uiPort = 0

function Save-ClientState {
    Write-KokoverseState $recordsPath @{server=$Server;ui_port=$uiPort;processes=@($processes)}
}
function Start-LocalProcess([string]$File, [string[]]$Arguments, [string]$LogPrefix, [bool]$Visible = $false, [bool]$Console = $false) {
    $resolvedFile = (Resolve-Path -LiteralPath $File).Path
    $quotedArguments = @($Arguments | ForEach-Object {
        $escaped = [regex]::Replace($_,'(\\*)"','$1$1\"')
        $escaped = [regex]::Replace($escaped,'(\\+)$','$1$1')
        '"' + $escaped + '"'
    })
    foreach ($suffix in @('-out.log','-error.log')) {
        if (Test-Path -LiteralPath ($LogPrefix + $suffix)) {
            Copy-Item -LiteralPath ($LogPrefix + $suffix) -Destination ($LogPrefix + '.previous' + $suffix) -Force
        }
    }
    # Godot is an interactive window. SW_HIDE suppresses that window as well as consoles.
    $style = if ($Visible) { 'Normal' } else { 'Hidden' }
    if ($Console) {
        # An explicit console host gives the Engine its own window, even when
        # Windows Terminal is the default or Start was called from a pipeline.
        $consoleArguments = @('"' + $resolvedFile + '"') + $quotedArguments
        $consoleProcess = Start-Process -FilePath (Join-Path $env:WINDIR 'System32/conhost.exe') -ArgumentList $consoleArguments -WorkingDirectory $localRoot -WindowStyle Normal -PassThru
        $deadline = [DateTime]::UtcNow.AddSeconds(5)
        $p = $null
        do {
            $child = Get-CimInstance Win32_Process -Filter "ParentProcessId=$($consoleProcess.Id)" | Where-Object { $_.ExecutablePath -eq $resolvedFile } | Select-Object -First 1
            if ($child) { $p = Get-Process -Id $child.ProcessId -ErrorAction SilentlyContinue }
            if ($p) { break }
            Start-Sleep -Milliseconds 50
        } while ([DateTime]::UtcNow -lt $deadline)
        if (-not $p) { throw "Engine console could not start. Check $LogPrefix-error.log." }
    } else {
        $p = Start-Process -FilePath $resolvedFile -ArgumentList $quotedArguments -WorkingDirectory $localRoot -WindowStyle $style -PassThru -RedirectStandardOutput "$LogPrefix-out.log" -RedirectStandardError "$LogPrefix-error.log"
    }
    return @{id=$p.Id;path=$resolvedFile;startTicks=$p.StartTime.ToUniversalTime().Ticks.ToString()}
}
function Client-Failure([string]$Label, [string]$LogPrefix) {
    $detail = if (Test-Path -LiteralPath "$LogPrefix-error.log") { (Get-Content -LiteralPath "$LogPrefix-error.log" -Tail 12) -join "`n" } else { '' }
    throw "$Label. Logs: $LogPrefix-error.log`n$detail"
}
function Start-Frontend {
    $godotFile = Join-Path $localRoot '.tools/godot/Godot_v4.7.2-stable_win64.exe'
    $record = Start-LocalProcess $godotFile @('--path',(Join-Path $localRoot 'client-godot'),'--',"--port=$uiPort",'--session=lobby','--close-with-engine') (Join-Path $clientRoot 'godot') $true
    $script:newClients += $record
    $script:processes += $record
    Save-ClientState
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        if (-not (Get-KokoverseProcess $record)) { Client-Failure 'The application window exited during startup' (Join-Path $clientRoot 'godot') }
        $window = Get-KokoverseWindow $record
        if ($window -ne [IntPtr]::Zero) { Show-KokoverseWindow $window; return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    Client-Failure 'The application window did not become visible' (Join-Path $clientRoot 'godot')
}
function Get-FreeLocalPort {
    $listener = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback,0)
    $listener.Start(); $port = $listener.LocalEndpoint.Port; $listener.Stop(); return $port
}

try {
    try { $locked = $launchMutex.WaitOne(0) } catch [Threading.AbandonedMutexException] { $locked = $true }
    if (-not $locked) { Write-Host 'Kokoverse is already starting. The window will open shortly.'; return }

    # Repeated Start restores the window, even while the remote service is temporarily offline.
    if (Test-Path -LiteralPath $recordsPath) {
        try { $old = Get-Content -LiteralPath $recordsPath -Raw | ConvertFrom-Json }
        catch { throw "Could not read the previous launch record: $recordsPath" }
        if ($old.server -and $old.server -ne $Server) {
            & (Join-Path $PSScriptRoot 'stop-local.ps1') -Profile $Profile -Online:(-not $isLocal)
        } else {
            $engine = @($old.processes | Where-Object { [IO.Path]::GetFileName($_.path) -eq 'kokoverse-engine.exe' -and (Get-KokoverseProcess $_) }) | Select-Object -First 1
            $frontends = @($old.processes | Where-Object { [IO.Path]::GetFileName($_.path) -like 'Godot_*.exe' -and (Get-KokoverseProcess $_) })
            if ($engine) {
                $uiPort = [int]$old.ui_port
                if (-not (Test-KokoverseIpc $uiPort)) { throw "The existing Engine is not responding. It was left running to preserve its world. Check $clientRoot/engine-error.log." }
                $processes = @($engine)
                foreach ($frontend in $frontends) {
                    $window = Get-KokoverseWindow $frontend
                    if ($window -ne [IntPtr]::Zero) {
                        $processes += $frontend
                        Save-ClientState
                        if (-not $Headless) { Show-KokoverseWindow $window }
                        Write-Host "Kokoverse is already running ($Profile). Restored the existing window."
                        return
                    }
                    # Recover windows hidden by an older launcher; the Engine and world stay alive.
                    Stop-KokoverseProcess $frontend
                }
                Save-ClientState
                if (-not $Headless) { Start-Frontend }
                Write-Host "Reopened Kokoverse ($Profile) using the existing session."
                return
            }
            foreach ($frontend in $frontends) { Stop-KokoverseProcess $frontend }
        }
    }

    if (-not $isLocal) {
        . (Join-Path $PSScriptRoot 'connect-relay.ps1')
        Connect-KokoverseRelay -Relay $Relay -Root $localRoot
        $health = Invoke-RestMethod "$Server/health" -TimeoutSec 15
        if ($health.service -ne 'kokoverse') { throw 'Unexpected management service.' }
    }
    $packaged = Test-Path -LiteralPath (Join-Path $localRoot 'bin/kokoverse-engine.exe')
    if (-not $packaged -and ($Build -or -not (Test-Path -LiteralPath 'target/debug/kokoverse-asset-server.exe'))) {
        . ./scripts/env.ps1
        cargo build --workspace --locked
        if ($LASTEXITCODE -ne 0) { throw 'Build failed.' }
    }
    $binaryDir = if ($packaged) { Join-Path $localRoot 'bin' } else { Join-Path $localRoot 'target/debug' }
    $adapterDir = if ($packaged) { Join-Path $localRoot 'bin' } else { Join-Path $localRoot '.tools' }
    if (-not $packaged) {
        $adapterBinary = Join-Path $adapterDir 'kokosync-adapter.exe'
        $relayBinary = Join-Path $adapterDir 'kokosync-relay.exe'
        $latestSource = Get-ChildItem -LiteralPath (Join-Path $localRoot 'tools/kokosync-adapter') -File -Recurse | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
        $needsGoBuild = $Build -or -not (Test-Path -LiteralPath $adapterBinary) -or ($isLocal -and -not (Test-Path -LiteralPath $relayBinary))
        if (-not $needsGoBuild) { $needsGoBuild = (Get-Item -LiteralPath $adapterBinary).LastWriteTimeUtc -lt $latestSource.LastWriteTimeUtc }
        if ($needsGoBuild) {
            $goTool = Join-Path $localRoot '.tools/go/bin/go.exe'
            if (-not (Test-Path -LiteralPath $goTool)) { $goTool = Join-Path $localRoot '../kokosync/.tools/go/bin/go.exe' }
            if (-not (Test-Path -LiteralPath $goTool)) { throw 'Go toolchain is missing. Run scripts/setup.ps1 first.' }
            $goTool = (Resolve-Path -LiteralPath $goTool).Path
            Push-Location (Join-Path $localRoot 'tools/kokosync-adapter')
            try {
                & $goTool build -mod=readonly -trimpath '-ldflags=-s -w' -o $adapterBinary .
                if ($LASTEXITCODE -ne 0) { throw 'Adapter build failed.' }
                & $goTool build -mod=readonly -trimpath '-ldflags=-s -w' -o $relayBinary ./cmd/relay
                if ($LASTEXITCODE -ne 0) { throw 'Local relay build failed.' }
            } finally { Pop-Location }
        }
    }
    if ($isLocal) {
        $shared = @()
        $serverHealthy = $false
        try { $health = Invoke-RestMethod 'http://127.0.0.1:8787/health' -TimeoutSec 2; $serverHealthy = $health.service -eq 'kokoverse' } catch {}
        if (-not $serverHealthy) {
            $shared += Start-LocalProcess (Join-Path $binaryDir 'kokoverse-asset-server.exe') @('--listen','127.0.0.1:8787','--data-dir',(Join-Path $runRoot 'server'),'--relay','127.0.0.1:48001') (Join-Path $runRoot 'server')
            $ready = $false
            for ($i=0; $i -lt 40; $i++) { try { $health = Invoke-RestMethod 'http://127.0.0.1:8787/health' -TimeoutSec 1; if ($health.service -eq 'kokoverse') { $ready=$true; break } } catch {}; Start-Sleep -Milliseconds 100 }
            if (-not $ready) { throw 'Local service did not start. Check .run/local/server-error.log.' }
        }
        $relayListener = Get-NetUDPEndpoint -LocalAddress '127.0.0.1' -LocalPort 48001 -ErrorAction SilentlyContinue
        if (-not $relayListener) {
            $shared += Start-LocalProcess (Join-Path $adapterDir 'kokosync-relay.exe') @('--http','127.0.0.1:48000','--quic','127.0.0.1:48001') (Join-Path $runRoot 'relay')
        } else {
            $relayProcess = Get-Process -Id @($relayListener)[0].OwningProcess -ErrorAction SilentlyContinue
            if (-not $relayProcess -or $relayProcess.ProcessName -ne 'kokosync-relay') { throw 'Port 48001 is used by another application.' }
        }
        if ($shared.Count -gt 0) {
            $sharedPath = Join-Path $runRoot 'services.json'
            $previous = if (Test-Path -LiteralPath $sharedPath) { @(Get-Content -LiteralPath $sharedPath -Raw | ConvertFrom-Json) } else { @() }
            ConvertTo-Json -InputObject @($previous + $shared) -Depth 5 | Set-Content -LiteralPath $sharedPath -Encoding UTF8
        }
    }
    $uiPort = Get-FreeLocalPort
    $transportPort = Get-FreeLocalPort
    while ($transportPort -eq $uiPort) { $transportPort = Get-FreeLocalPort }
    $profileDirectory = "Kokoverse/profiles/$Profile"
    if (-not $isLocal) {
        $hasher = [Security.Cryptography.SHA256]::Create()
        try { $serverId = ([BitConverter]::ToString($hasher.ComputeHash([Text.Encoding]::UTF8.GetBytes($Server)))).Replace('-','').ToLowerInvariant() } finally { $hasher.Dispose() }
        $profileDirectory = "Kokoverse/servers/$serverId/profiles/$Profile"
    }
    $profilePath = Join-Path $env:LOCALAPPDATA "$profileDirectory/profile.dat"
    $engineArgs = @('--server',$Server,'--port',"$uiPort",'--transport-port',"$transportPort",'--profile',$profilePath,'--data-dir',$clientRoot,'--asset-dir',(Join-Path $clientRoot 'assets'),'--adapter',(Join-Path $adapterDir 'kokosync-adapter.exe'))
    if (-not $Headless) {
        $engineArgs += @('--console','--console-title',"Kokoverse Engine ($Profile)",'--log-file',(Join-Path $clientRoot 'engine-error.log'))
    }
    $engine = Start-LocalProcess (Join-Path $binaryDir 'kokoverse-engine.exe') $engineArgs (Join-Path $clientRoot 'engine') (-not $Headless) (-not $Headless)
    $newClients += $engine
    $processes = @($engine)
    Save-ClientState
    $ready = $false
    for ($i=0; $i -lt 100; $i++) {
        if (-not (Get-KokoverseProcess $engine)) { Client-Failure 'The Engine exited during startup' (Join-Path $clientRoot 'engine') }
        if (Test-KokoverseIpc $uiPort) { $ready=$true; break }
        Start-Sleep -Milliseconds 100
    }
    if (-not $ready) { Client-Failure 'The Engine did not start listening' (Join-Path $clientRoot 'engine') }
    if (-not $Headless) { Start-Frontend }
    Write-Host "Kokoverse started ($Profile, $Server)."
    Write-Host "Logs: $clientRoot"
} catch {
    # Roll back only clients created by this invocation. An existing host remains untouched.
    foreach ($record in @($newClients | Select-Object -Last 100)) { Stop-KokoverseProcess $record }
    $processes = @($processes | Where-Object { Get-KokoverseProcess $_ })
    if ($uiPort -gt 0) { Save-ClientState }
    throw
} finally {
    if ($locked) { $launchMutex.ReleaseMutex() }
    $launchMutex.Dispose()
}
