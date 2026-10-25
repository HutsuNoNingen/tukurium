param([string]$PackageRoot = (Split-Path -Parent $PSScriptRoot), [string]$Server = 'http://127.0.0.1:8787', [string]$Relay = '')
$ErrorActionPreference = 'Stop'
$PackageRoot = (Resolve-Path -LiteralPath $PackageRoot).Path
. (Join-Path $PackageRoot 'scripts/process-lifecycle.ps1')
$profile = 'window-test-' + [guid]::NewGuid().ToString('N').Substring(0,12)
$online = $Server -ne 'http://127.0.0.1:8787'
$mode = if ($online) { 'online' } else { 'local' }
$run = Join-Path $PackageRoot ".run/$mode/$profile"
$recordPath = Join-Path $run 'processes.json'
$client = $null
$stream = $null
$session = 'lobby'
$epoch = ''
$lastSnapshot = $null
New-Item -ItemType Directory -Path $run -Force | Out-Null
# Use a disposable helper so attaching to a console never changes the test
# runner's own standard handles (including its captured output).
$consoleHelper = Join-Path $run 'console-window.exe'
Add-Type -OutputAssembly $consoleHelper -OutputType ConsoleApplication -TypeDefinition @'
using System;
using System.IO;
using System.Text;
using System.Runtime.InteropServices;
public class ConsoleWindowProbe {
    [StructLayout(LayoutKind.Sequential)] public struct Coord { public short X, Y; }
    [DllImport("kernel32.dll")] public static extern bool FreeConsole();
    [DllImport("kernel32.dll")] public static extern bool AttachConsole(uint pid);
    [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow();
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] public static extern uint GetConsoleTitle(StringBuilder title, uint size);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr CreateFile(string name, uint access, uint share, IntPtr security, uint mode, uint flags, IntPtr template);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] public static extern bool ReadConsoleOutputCharacter(IntPtr console, StringBuilder text, uint size, Coord point, out uint read);
    [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
    public static int Main(string[] args) {
        FreeConsole();
        if (!AttachConsole(uint.Parse(args[0]))) return 1;
        var title = new StringBuilder(512);
        GetConsoleTitle(title,512);
        var window = GetConsoleWindow();
        var output = CreateFile("CONOUT$",0x80000000,3,IntPtr.Zero,3,0,IntPtr.Zero);
        var screen = new StringBuilder(4096);
        uint read;
        ReadConsoleOutputCharacter(output,screen,4096,new Coord(),out read);
        CloseHandle(output);
        FreeConsole();
        File.WriteAllLines(args[1],new[] {window.ToInt64().ToString(),title.ToString()});
        File.WriteAllText(args[1]+".screen.txt",screen.ToString());
        return 0;
    }
}
'@
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class KokoverseWindowTest {
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
}
'@
function Assert-Test($Value, [string]$Message) { if (-not $Value) { throw $Message } }
function Start-TestClient {
    New-Item -ItemType Directory -Path $run -Force | Out-Null
    $arguments = @('-NoProfile','-ExecutionPolicy','Bypass','-File',('"'+(Join-Path $PackageRoot 'scripts/start-local.ps1')+'"'),'-Profile',$profile,'-Server',$Server)
    if ($Relay) { $arguments += @('-Relay',$Relay) }
    $launch = Start-Process powershell.exe -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $run 'launcher-out.log') -RedirectStandardError (Join-Path $run 'launcher-error.log')
    $null = $launch.Handle
    Assert-Test ($launch.WaitForExit(30000)) 'Launcher timed out'
    Assert-Test ($launch.ExitCode -eq 0) ('Launcher failed: ' + (Get-Content -LiteralPath (Join-Path $run 'launcher-error.log') -Raw))
    $state = Get-Content -LiteralPath $recordPath -Raw | ConvertFrom-Json
    $engine = @($state.processes | Where-Object { $_.path -like '*kokoverse-engine.exe' })[0]
    $gui = @($state.processes | Where-Object { $_.path -like '*Godot_*.exe' })[0]
    Assert-Test ($null -ne (Get-KokoverseProcess $engine)) 'Engine is not alive'
    $window = Get-KokoverseWindow $gui
    Assert-Test ($window -ne [IntPtr]::Zero -and [KokoverseWindowTest]::IsWindowVisible($window)) 'Godot window is hidden'
    # Windows Terminal uses a message-only console HWND. Attach to this exact
    # Engine to get its console, never close a shared Terminal window by title.
    $probeFile = Join-Path $run 'console-window.txt'
    $probe = Start-Process $consoleHelper -ArgumentList @($engine.id,('"'+$probeFile+'"')) -WindowStyle Hidden -PassThru
    $null = $probe.Handle
    Assert-Test ($probe.WaitForExit(8000) -and $probe.ExitCode -eq 0) 'Engine has no console'
    $info = Get-Content -LiteralPath $probeFile
    $console = [IntPtr]([long]$info[0])
    Assert-Test ($console -ne [IntPtr]::Zero -and $info[1] -eq "tukurium Engine ($profile)") 'Wrong Engine console'
    Assert-Test ([KokoverseWindowTest]::IsWindowVisible($console)) 'Engine console is hidden'
    Assert-Test ((Get-Content -LiteralPath ($probeFile+'.screen.txt') -Raw).Contains('tukurium')) 'Engine logs are missing from the console'
    return @{state=$state;engine=$engine;gui=$gui;console=$console}
}
function Read-TestFrame {
    $header = New-Object byte[] 4
    $offset = 0
    while ($offset -lt 4) { $n=$stream.Read($header,$offset,4-$offset); Assert-Test ($n -gt 0) 'Engine disconnected'; $offset+=$n }
    $size = [Net.IPAddress]::NetworkToHostOrder([BitConverter]::ToInt32($header,0))
    Assert-Test ($size -gt 0 -and $size -le 1048576) 'Invalid frame length'
    $data = New-Object byte[] $size
    $offset = 0
    while ($offset -lt $size) { $n=$stream.Read($data,$offset,$size-$offset); Assert-Test ($n -gt 0) 'Engine disconnected'; $offset+=$n }
    $frame = [Text.Encoding]::UTF8.GetString($data) | ConvertFrom-Json
    $script:session = $frame.session_id
    $script:epoch = $frame.authority_epoch
    if ($frame.body.kind -eq 'snapshot') { $script:lastSnapshot = $frame.body }
    return $frame
}
function Send-TestFrame($Body) {
    $data = [Text.Encoding]::UTF8.GetBytes((@{protocol_version=3;session_id=$session;authority_epoch=$epoch;sequence='1';body=$Body} | ConvertTo-Json -Depth 30 -Compress))
    $header = [BitConverter]::GetBytes([Net.IPAddress]::HostToNetworkOrder($data.Length))
    $stream.Write($header,0,4); $stream.Write($data,0,$data.Length)
}
function Call-TestService([string]$Action, $Values = @{}) {
    $request = [guid]::NewGuid().ToString()
    $body = @{kind='service';action=$Action;request_id=$request}
    foreach ($key in $Values.Keys) { $body[$key] = $Values[$key] }
    Send-TestFrame $body
    do { $frame=Read-TestFrame } until ($frame.body.kind -eq 'service_result' -and $frame.body.request_id -eq $request)
    Assert-Test $frame.body.ok ("Service failed: " + $Action + ': ' + $frame.body.message)
    return $frame.body.data
}
function Wait-Stopped($Record) {
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    while ((Get-KokoverseProcess $Record) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 50 }
    Assert-Test (-not (Get-KokoverseProcess $Record)) ("Process did not stop: " + $Record.path + ' PID ' + $Record.id)
}
try {
    $first = Start-TestClient
    $again = Start-TestClient
    Assert-Test ($first.engine.id -eq $again.engine.id -and $first.gui.id -eq $again.gui.id) 'Repeated Start duplicated processes'
    $guiProcess = Get-KokoverseProcess $first.gui
    Assert-Test ($guiProcess.CloseMainWindow()) 'Could not close Godot window'
    Wait-Stopped $first.gui
    $reopened = Start-TestClient
    Assert-Test ($first.engine.id -eq $reopened.engine.id -and $first.gui.id -ne $reopened.gui.id) 'Start did not reconnect to the existing Engine'
    Write-Output 'PASS: visible windows, repeated Start, and GUI reopen without Stop'

    $client = New-Object Net.Sockets.TcpClient('127.0.0.1',[int]$first.state.ui_port)
    $stream = $client.GetStream(); $stream.ReadTimeout=15000; $stream.WriteTimeout=5000
    do { $frame=Read-TestFrame } until ($frame.body.kind -eq 'service_state')
    $null = Call-TestService 'login' @{name=$profile}
    $created = Call-TestService 'create_session' @{name='Console close regression'}
    do { $frame=Read-TestFrame } until ($lastSnapshot -and $lastSnapshot.editable -and $session -eq $created.session.id)
    $object = $lastSnapshot.snapshot.objects[0]
    $field = @($object.fields | Where-Object id -eq 'name')[0]
    $marker = 'Saved by console close ' + [guid]::NewGuid().ToString()
    $command = [guid]::NewGuid().ToString()
    Send-TestFrame @{kind='edit';edit=@{command_id=$command;target=$object.address;field_id='name';expected_revision=$field.revision;value=$marker}}
    do { $frame=Read-TestFrame } until ($frame.body.kind -eq 'result' -and $frame.body.command_id -eq $command)
    Assert-Test $frame.body.ok ('World edit failed: ' + $frame.body.message)
    # WM_CLOSE is the same close request as the console's X, not process termination.
    Assert-Test ([KokoverseWindowTest]::PostMessage($reopened.console,0x10,[IntPtr]::Zero,[IntPtr]::Zero)) 'Could not close Engine console'
    Wait-Stopped $reopened.engine
    Wait-Stopped $reopened.gui
    $client.Close(); $client=$null
    $saved = Join-Path $run ("worlds/" + $created.session.id + '.json')
    Assert-Test ((Get-Content -LiteralPath $saved -Raw).Contains($marker)) 'Latest edit was not saved on console close'
    Assert-Test ((Get-Content -LiteralPath (Join-Path $run 'engine-error.log') -Raw).Contains('Engine stopped.')) 'Graceful shutdown did not complete'
    $fresh = Start-TestClient
    Assert-Test ($fresh.engine.startTicks -ne $first.engine.startTicks) 'Start did not create a new Engine after console close'
    Assert-Test ([KokoverseWindowTest]::PostMessage($fresh.console,0x10,[IntPtr]::Zero,[IntPtr]::Zero)) 'Could not close restarted console'
    Wait-Stopped $fresh.engine
    Wait-Stopped $fresh.gui
    Write-Output 'PASS: console X saves the latest edit, closes Godot, and allows a fresh Start'
    Write-Output "TEST_RUN=$run"
} catch {
    $_ | Out-String | Set-Content -LiteralPath (Join-Path $run 'test-failure.txt')
    throw
} finally {
    if ($client) { $client.Close() }
    if (Test-Path -LiteralPath $recordPath) {
        $stopFlags = @()
        if ($online) { $stopFlags += '-Online' }
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PackageRoot 'scripts/stop-local.ps1') -Profile $profile @stopFlags
    }
}
