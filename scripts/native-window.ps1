param(
    [Parameter(Mandatory = $true)][int]$GameProcessId,
    [string]$ExpectedTitle = '', [string]$CapturePath = '', [switch]$Close
)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class GameWindow {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }
    private delegate bool VisitWindow(IntPtr handle, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(VisitWindow visit, IntPtr parameter);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr handle, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetWindowText(IntPtr handle, StringBuilder text, int capacity);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern int GetClassName(IntPtr handle, StringBuilder text, int capacity);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr handle, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr handle, IntPtr dc, uint flags);
    public static string Title(IntPtr handle) {
        var text = new StringBuilder(1024); GetWindowText(handle, text, text.Capacity); return text.ToString();
    }
    public static string Class(IntPtr handle) {
        var text = new StringBuilder(256); GetClassName(handle, text, text.Capacity); return text.ToString();
    }
    public static IntPtr Find(int processId, string expectedTitle) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((handle, parameter) => {
            uint owner; GetWindowThreadProcessId(handle, out owner);
            if (owner != processId || !IsWindowVisible(handle) || Class(handle) == "ConsoleWindowClass") return true;
            if (expectedTitle.Length != 0 && Title(handle) != expectedTitle) return true;
            found = handle; return false;
        }, IntPtr.Zero);
        return found;
    }
}
'@
$game = Get-Process -Id $GameProcessId
$deadline = [DateTime]::UtcNow.AddSeconds(30)
while ([DateTime]::UtcNow -lt $deadline) {
    $game.Refresh()
    if ($game.HasExited) { throw 'Game exited before opening its window' }
    $handle = [GameWindow]::Find($GameProcessId, $ExpectedTitle)
    $rect = New-Object GameWindow+Rect
    if ($handle -ne [IntPtr]::Zero -and [GameWindow]::IsWindowVisible($handle) -and
        [GameWindow]::GetWindowRect($handle, [ref]$rect)) {
        $width = $rect.Right - $rect.Left
        $height = $rect.Bottom - $rect.Top
        if ($width -ge 480 -and $height -ge 320) {
            if ($Close) {
                if (-not [GameWindow]::PostMessage($handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) {
                    throw 'Owned game window rejected WM_CLOSE'
                }
                exit 0
            }
            if ($CapturePath) {
                if (Test-Path $CapturePath) { throw 'Refusing to overwrite an existing window capture' }
                Add-Type -AssemblyName System.Drawing
                $qaBitmap = New-Object System.Drawing.Bitmap($width, $height)
                $qaGraphics = [System.Drawing.Graphics]::FromImage($qaBitmap)
                try {
                    $qaContext = $qaGraphics.GetHdc()
                    try {
                        if (-not [GameWindow]::PrintWindow($handle, $qaContext, 2)) { throw 'Owned window capture failed' }
                    } finally { $qaGraphics.ReleaseHdc($qaContext) }
                    $qaBitmap.Save($CapturePath, [System.Drawing.Imaging.ImageFormat]::Png)
                } finally { $qaGraphics.Dispose(); $qaBitmap.Dispose() }
            }
            @{ pid = $GameProcessId; width = $width; height = $height; visible = $true;
               title = [GameWindow]::Title($handle); className = [GameWindow]::Class($handle) } |
                ConvertTo-Json -Compress
            exit 0
        }
    }
    Start-Sleep -Milliseconds 100
}
throw 'No visible game window within 30 seconds'
