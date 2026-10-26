param(
    [Parameter(Mandatory=$true)][long]$Handle,
    [Parameter(Mandatory=$true)][int]$ExpectedProcessId,
    [Parameter(Mandatory=$true)][ValidateRange(1,2)][int]$DownSteps
)
$ErrorActionPreference = 'Stop'
# PopupMenu handles native Window input before Viewport.push_input can see it.
# Post only to the handle supplied by this test's dropdown; never global input.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class LocalizationPopupInput {
    [DllImport("user32.dll", SetLastError=true)] public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    public static void Send(long handle, int expectedProcessId, int steps) {
        var hwnd=new IntPtr(handle);
        uint processId;
        if(GetWindowThreadProcessId(hwnd,out processId)==0)throw new Exception("Language popup handle is invalid");
        if(processId!=(uint)expectedProcessId)throw new Exception("Language popup is not owned by the current test process");
        var process=System.Diagnostics.Process.GetProcessById((int)processId);
        if(!process.ProcessName.StartsWith("Godot_"))throw new Exception("Language popup is not owned by Godot");
        for(var i=0;i<steps;i++)Key(hwnd,0x28);
        Key(hwnd,0x0D);
    }
    private static void Key(IntPtr hwnd,int key) {
        if(!PostMessage(hwnd,0x0100,new IntPtr(key),new IntPtr(1)) || !PostMessage(hwnd,0x0101,new IntPtr(key),new IntPtr(unchecked((int)0xC0000001))))
            throw new Exception("Could not post language popup key input");
    }
}
'@
[LocalizationPopupInput]::Send($Handle,$ExpectedProcessId,$DownSteps)
