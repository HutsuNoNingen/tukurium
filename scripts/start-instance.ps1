param([switch]$Local, [switch]$Headless)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$stateRoot = if ($env:KOKOVERSE_DATA_ROOT) { [IO.Path]::GetFullPath($env:KOKOVERSE_DATA_ROOT) } else { $root }
. (Join-Path $PSScriptRoot 'process-lifecycle.ps1')
$online = -not $Local
$mode = if ($online) { 'online' } else { 'local' }
# Serialize allocation through startup, including rapid double-clicks from
# separate launcher processes. Explicit -Profile launches keep their old behavior.
$allocationMutex = New-KokoverseLaunchMutex $stateRoot ':instance-allocation:' $online
$allocationHeld = $false
$profileMutex = $null
$profileHeld = $false
$profile = $null
try {
    try { $allocationHeld = $allocationMutex.WaitOne(60000) }
    catch [Threading.AbandonedMutexException] { $allocationHeld = $true }
    if (-not $allocationHeld) { throw 'Another instance is still starting. Please retry.' }
    for ($index = 1; $index -le 10000; $index++) {
        $candidate = if ($index -eq 1) { 'default' } else { "instance-$index" }
        $candidateMutex = New-KokoverseLaunchMutex $stateRoot $candidate $online
        $candidateHeld = $false
        try {
            try { $candidateHeld = $candidateMutex.WaitOne(0) }
            catch [Threading.AbandonedMutexException] { $candidateHeld = $true }
            if (-not $candidateHeld) { continue }
            $recordPath = Join-Path $stateRoot ".run/$mode/$candidate/processes.json"
            $busy = $false
            if (Test-Path -LiteralPath $recordPath) {
                try {
                    $state = Get-Content -LiteralPath $recordPath -Raw | ConvertFrom-Json
                    # Don't overwrite a profile whose ownership cannot be read.
                    $busy = -not ($state.PSObject.Properties.Name -contains 'processes')
                    foreach ($record in @($state.processes)) {
                        if (Get-KokoverseProcess $record) { $busy = $true; break }
                    }
                } catch { $busy = $true }
            }
            if (-not $busy) {
                $profile = $candidate
                $profileMutex = $candidateMutex
                $profileHeld = $true
                break
            }
        } finally {
            if ($profileMutex -ne $candidateMutex) {
                if ($candidateHeld) { $candidateMutex.ReleaseMutex() }
                $candidateMutex.Dispose()
            }
        }
    }
    if (-not $profile) { throw 'No free instance profile was found.' }
    Write-Host "Starting a new Kokoverse instance ($profile)."
    $launcher = if ($online) { 'start-online.ps1' } else { 'start-local.ps1' }
    & (Join-Path $PSScriptRoot $launcher) -Profile $profile -Headless:$Headless
} finally {
    if ($profileHeld) { $profileMutex.ReleaseMutex() }
    if ($profileMutex) { $profileMutex.Dispose() }
    if ($allocationHeld) { $allocationMutex.ReleaseMutex() }
    $allocationMutex.Dispose()
}
