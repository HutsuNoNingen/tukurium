param([Parameter(Mandatory=$true)][long]$Handle)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class InspectorFrameProbe {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr hwnd, int index);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
    public static void Check(long handle) {
        var hwnd = new IntPtr(handle);
        var style = GetWindowLong(hwnd, -16);
        if ((style & 0x00C00000) != 0x00C00000 || (style & 0x00040000) == 0)
            throw new Exception("Native title bar or resize frame is missing");
        if ((style & 0x00030000) != 0x00030000)
            throw new Exception("Native minimize/maximize buttons are missing");
        Rect rect; GetWindowRect(hwnd, out rect);
        var origin = new Point(); ClientToScreen(hwnd, ref origin);
        var x = (rect.Left + rect.Right) / 2;
        var y = (rect.Top + origin.Y) / 2;
        var point = new IntPtr((y << 16) | (x & 0xFFFF));
        IntPtr hit;
        if (SendMessageTimeout(hwnd, 0x0084, IntPtr.Zero, point, 2, 2000, out hit) == IntPtr.Zero || hit.ToInt64() != 2)
            throw new Exception("Native title bar does not accept window dragging");
    }
}
'@
[InspectorFrameProbe]::Check($Handle)
Write-Output 'PASS: Windows caption, resize border, minimize/maximize, and caption drag hit test'
