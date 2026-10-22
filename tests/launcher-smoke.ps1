param([Parameter(Mandatory=$true)][string]$PackageRoot)
$ErrorActionPreference = 'Stop'
$workspaceRoot = Split-Path -Parent $PSScriptRoot
$PackageRoot = (Resolve-Path -LiteralPath $PackageRoot).Path
$testRoot = Join-Path $workspaceRoot ('.run/launcher test 日本語 ' + [guid]::NewGuid().ToString('N'))
$folders = @()
$launchers = @()
try {
    foreach ($name in @('first','second')) {
        $folder = Join-Path $testRoot $name
        $folders += $folder
        foreach ($dir in @('bin','scripts','.run')) { New-Item -ItemType Directory -Force -Path (Join-Path $folder $dir) | Out-Null }
        foreach ($file in @('connection.json','relay-access.json')) { Copy-Item -LiteralPath (Join-Path $PackageRoot $file) -Destination $folder }
        foreach ($file in @('kokoverse-engine.exe','kokosync-adapter.exe')) { Copy-Item -LiteralPath (Join-Path $PackageRoot ('bin/'+$file)) -Destination (Join-Path $folder 'bin') }
        Get-ChildItem -LiteralPath (Join-Path $PackageRoot 'scripts') -File | Copy-Item -Destination (Join-Path $folder 'scripts')
        # Simulate copying an already-used installation, including its old identity.
        'guest-0123456789abcdef0123456789abcdef' | Set-Content -LiteralPath (Join-Path $folder '.run/peer-id.txt') -Encoding Ascii
        $PackageRoot.ToLowerInvariant() | Set-Content -LiteralPath (Join-Path $folder '.run/peer-folder.txt') -Encoding UTF8
        $arguments = @('-NoProfile','-STA','-ExecutionPolicy','Bypass','-File',('"'+(Join-Path $folder 'scripts/start.ps1')+'"'),'-Mode','Join','-Username',('"'+$name+' Tester"'),'-Headless','-SkipMedia')
        $launchers += Start-Process -FilePath 'powershell.exe' -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $folder 'startup-output.log') -RedirectStandardError (Join-Path $folder 'startup-error.log')
    }
    foreach ($launcher in $launchers) {
        if (-not $launcher.WaitForExit(30000)) { throw 'Concurrent startup timed out' }
        if ($launcher.ExitCode -ne 0) { throw ('Startup failed. Logs: '+$testRoot) }
    }
    $connections = @($folders | ForEach-Object { Get-Content -LiteralPath (Join-Path $_ '.run/connection.json') -Raw -Encoding UTF8 | ConvertFrom-Json })
    $ports = @($connections | ForEach-Object { $_.ui_port; $_.transport_port })
    if (@($ports | Sort-Object -Unique).Count -ne 4) { throw 'Concurrent copies reused local ports' }
    if ($connections[0].peer -eq $connections[1].peer) { throw 'Copied installations reused the same guest identity' }
    foreach ($folder in $folders) {
        $records = Get-Content -LiteralPath (Join-Path $folder '.run/processes.json') -Raw -Encoding UTF8 | ConvertFrom-Json
        if (@($records).Count -ne 2) { throw 'Expected an engine and adapter' }
        foreach ($record in $records) { if (-not (Get-Process -Id $record.id -ErrorAction SilentlyContinue)) { throw 'A runtime process exited' } }
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $folder 'scripts/stop.ps1')
        if ($LASTEXITCODE -ne 0) { throw 'Stop script failed' }
        foreach ($record in $records) { if (Get-Process -Id $record.id -ErrorAction SilentlyContinue) { throw 'Stop left a runtime process alive' } }
    }
    Write-Output 'PASS: concurrent Windows PowerShell launches select four unique ports, copied guest IDs change, and Unicode process records stop both runtimes.'
} finally {
    foreach ($folder in $folders) {
        if (Test-Path -LiteralPath (Join-Path $folder '.run/processes.json')) { & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $folder 'scripts/stop.ps1') }
    }
}
