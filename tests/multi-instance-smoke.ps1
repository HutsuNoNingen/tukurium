param([Parameter(Mandatory=$true)][string]$PackageRoot)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$PackageRoot = (Resolve-Path -LiteralPath $PackageRoot).Path
$testRoot = Join-Path $repo ('.run/multi-instance-' + [guid]::NewGuid().ToString('N'))
$fixture = Join-Path $testRoot 'package with spaces'
New-Item -ItemType Directory -Path $fixture -Force | Out-Null
foreach ($name in @('bin','client-godot','.tools','scripts','connection.json','Start.bat')) {
    Copy-Item -LiteralPath (Join-Path $PackageRoot $name) -Destination $fixture -Recurse
}
. (Join-Path $fixture 'scripts/process-lifecycle.ps1')
$previousAppData = $env:LOCALAPPDATA
$env:LOCALAPPDATA = Join-Path $testRoot 'AppData'
New-Item -ItemType Directory -Path $env:LOCALAPPDATA -Force | Out-Null
$launches = @()
function Assert-Test($value, [string]$message) { if (-not $value) { throw $message } }
function Launch-Batch([string]$label) {
    $command = '""' + (Join-Path $fixture 'Start.bat') + '""'
    $process = Start-Process -FilePath $env:ComSpec -ArgumentList @('/d','/c',$command) -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $testRoot "$label-out.log") -RedirectStandardError (Join-Path $testRoot "$label-error.log")
    $null = $process.Handle
    return $process
}
function Wait-Launch($process) {
    Assert-Test ($process.WaitForExit(60000)) 'Start.bat did not finish'
    Assert-Test ($process.ExitCode -eq 0) 'Start.bat failed'
}
function Read-Profile([string]$profile) {
    return Get-Content -LiteralPath (Join-Path $fixture ".run/online/$profile/processes.json") -Raw | ConvertFrom-Json
}
function Stop-Profile([string]$profile) {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $fixture 'scripts/stop-online.ps1') -Profile $profile
    Assert-Test ($LASTEXITCODE -eq 0) "Could not stop $profile"
}
try {
    # Launch the actual batch file twice before either startup completes.
    $launches += Launch-Batch 'first'
    $launches += Launch-Batch 'second'
    foreach ($launch in $launches) { Wait-Launch $launch }
    $first = Read-Profile 'default'
    $second = Read-Profile 'instance-2'
    Assert-Test ($first.ui_port -ne $second.ui_port) 'Instances share a UI port'
    $records = @($first.processes) + @($second.processes)
    Assert-Test (@($records.id | Sort-Object -Unique).Count -eq 4) 'Instances share runtime processes'
    foreach ($record in $records) { Assert-Test ($null -ne (Get-KokoverseProcess $record)) 'A runtime exited' }
    $windows = @($records | Where-Object { $_.path -like '*Godot_*.exe' })
    Assert-Test ($windows.Count -eq 2) 'Two Godot windows were not started'
    foreach ($record in $windows) { Assert-Test ((Get-KokoverseWindow $record) -ne [IntPtr]::Zero) 'A Godot window is missing' }
    $engines = @($records | Where-Object { $_.path -like '*kokoverse-engine.exe' })
    $commands = @($engines | ForEach-Object { (Get-CimInstance Win32_Process -Filter "ProcessId=$($_.id)").CommandLine.Replace('\','/') })
    Assert-Test (($commands -join "`n").Contains('profiles/default/profile.dat')) 'First identity profile missing'
    Assert-Test (($commands -join "`n").Contains('profiles/instance-2/profile.dat')) 'Second identity profile missing'
    Write-Output 'PASS: two simultaneous Start.bat launches get independent profiles, ports, Engines and Godot windows'

    $marker = Join-Path $fixture '.run/online/default/keep.txt'
    'saved-profile-marker' | Set-Content -LiteralPath $marker
    Stop-Profile 'default'
    foreach ($record in $second.processes) { Assert-Test ($null -ne (Get-KokoverseProcess $record)) 'Stopping one instance stopped another' }
    $third = Launch-Batch 'reuse'
    $launches += $third
    Wait-Launch $third
    $reused = Read-Profile 'default'
    Assert-Test (($reused.processes.id | Where-Object { $first.processes.id -contains $_ }).Count -eq 0) 'Stopped processes were reused'
    Assert-Test ((Get-Content -LiteralPath $marker -Raw).Trim() -eq 'saved-profile-marker') 'Reusing a free profile discarded saved files'
    foreach ($record in $second.processes) { Assert-Test ($null -ne (Get-KokoverseProcess $record)) 'Another launch replaced a running instance' }
    Write-Output 'PASS: instances stop independently and a freed profile is reused without deleting its data'
    Write-Output "TEST_ROOT=$testRoot"
} finally {
    foreach ($profile in @('default','instance-2')) {
        if (Test-Path -LiteralPath (Join-Path $fixture ".run/online/$profile/processes.json")) {
            Stop-Profile $profile
        }
    }
    foreach ($launch in $launches) { if (-not $launch.HasExited) { $launch.Kill() } }
    $env:LOCALAPPDATA = $previousAppData
}
