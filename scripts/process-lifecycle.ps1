# Windows process ownership and window handling shared by Start and Stop.
function Get-KokoverseProcess($Record) {
    if (-not $Record) { return $null }
    $process = Get-Process -Id $Record.id -ErrorAction SilentlyContinue
    try {
        if ($process -and $process.Path -eq $Record.path -and $process.StartTime.ToUniversalTime().Ticks.ToString() -eq $Record.startTicks) { return $process }
    } catch {}
    return $null
}

function Stop-KokoverseProcess($Record) {
    $process = Get-KokoverseProcess $Record
    if ($process) { Stop-Process -Id $process.Id -ErrorAction Stop; $null = $process.WaitForExit(5000) }
}

function Write-KokoverseState([string]$Path, $State) {
    $temporary = $Path + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
    try {
        $State | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $temporary -Encoding UTF8
        Move-Item -LiteralPath $temporary -Destination $Path -Force
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary }
    }
}

function New-KokoverseLaunchMutex([string]$Root, [string]$Profile, [bool]$Online) {
    $hash = [Security.Cryptography.SHA256]::Create()
    try { $key = ([BitConverter]::ToString($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes(([IO.Path]::GetFullPath($Root).ToLowerInvariant() + '|' + $Profile + '|' + $Online))))).Replace('-','') }
    finally { $hash.Dispose() }
    return New-Object Threading.Mutex($false, ('Local\KokoverseStart-' + $key))
}

function Test-KokoverseIpc([int]$Port) {
    $client = New-Object Net.Sockets.TcpClient
    try { return $client.ConnectAsync('127.0.0.1',$Port).Wait(400) -and $client.Connected }
    catch { return $false }
    finally { $client.Dispose() }
}

function Get-KokoverseWindow($Record) {
    $process = Get-KokoverseProcess $Record
    if ($process) { $process.Refresh(); return $process.MainWindowHandle }
    return [IntPtr]::Zero
}

function Show-KokoverseWindow([IntPtr]$Window) {
    if ($Window -eq [IntPtr]::Zero) { return }
    if (-not ('Kokoverse.LaunchWindow' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace Kokoverse {
    public static class LaunchWindow {
        [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr window, int command);
        [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
        [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    }
}
'@
    }
    $null = [Kokoverse.LaunchWindow]::ShowWindowAsync($Window,9)
    $null = [Kokoverse.LaunchWindow]::SetForegroundWindow($Window)
}
