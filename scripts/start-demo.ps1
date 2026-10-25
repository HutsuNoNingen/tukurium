param([switch]$Headless, [string]$Session = ('demo-' + [guid]::NewGuid().ToString('N')), [string]$WorldFile = '')
$ErrorActionPreference = 'Stop'
if ($Session -notmatch '^[A-Za-z0-9_-]{1,80}$') { throw 'Session must be 1-80 letters, numbers, underscores or hyphens.' }
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    if (Test-Path .run/processes.json) { throw 'Demo process record exists. Run scripts/stop-demo.ps1 first.' }
    if (-not (Test-Path target/debug/kokoverse-engine.exe)) { & ./scripts/build.ps1 }
    New-Item -ItemType Directory -Force .run | Out-Null
    $started = [System.Collections.Generic.List[object]]::new()
    function Start-DemoProcess([string]$Name, [string]$File, [string[]]$Arguments) {
        $resolved = (Resolve-Path $File).Path
        $process = Start-Process -FilePath $resolved -ArgumentList $Arguments -WorkingDirectory $projectRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput ".run/$Name.log" -RedirectStandardError ".run/$Name-error.log"
        $started.Add(@{ id = $process.Id; path = $resolved; startTicks = $process.StartTime.ToUniversalTime().Ticks.ToString() })
    }
    try {
        Start-DemoProcess 'relay' '.tools/kokosync-relay.exe' @('--http','127.0.0.1:48000','--quic','127.0.0.1:48001')
        $hostArguments = @('--session',$Session,'--port','47000','--transport-port','47001')
        if ($WorldFile) { $hostArguments += @('--load',('"' + (Resolve-Path $WorldFile).Path + '"')) }
        Start-DemoProcess 'host' 'target/debug/kokoverse-engine.exe' $hostArguments
        Start-DemoProcess 'guest' 'target/debug/kokoverse-engine.exe' @('--role','guest','--session',$Session,'--port','47010','--transport-port','47011')
        Start-DemoProcess 'host-adapter' '.tools/kokosync-adapter.exe' @('--session',$Session,'--engine','127.0.0.1:47001','--peer','host','--insecure-development')
        Start-DemoProcess 'guest-adapter' '.tools/kokosync-adapter.exe' @('--session',$Session,'--engine','127.0.0.1:47011','--peer','guest','--insecure-development')
        if (-not $Headless) {
            # These are the user-facing interactive clients; keep their windows visible.
            foreach ($port in @(47000, 47010)) {
                $file = (Resolve-Path '.tools/godot/Godot_v4.7.2-stable_win64.exe').Path
                $process = Start-Process -FilePath $file -ArgumentList @('--path','client-godot','--',"--port=$port","--session=$Session") -WorkingDirectory $projectRoot -PassThru
                $started.Add(@{ id = $process.Id; path = $file; startTicks = $process.StartTime.ToUniversalTime().Ticks.ToString() })
            }
        }
        ConvertTo-Json -InputObject @($started.ToArray()) | Set-Content .run/processes.json
        Write-Host "tukurium demo started: $Session"
        Write-Host 'Stop: ./scripts/stop-demo.ps1'
    } catch {
        foreach ($record in $started) { Stop-Process -Id $record.id -ErrorAction SilentlyContinue }
        throw
    }
} finally { Pop-Location }
