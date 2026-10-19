param([Parameter(Mandatory = $true)][int]$GameProcessId, [switch]$Close)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class GameWindow {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr handle, out Rect rect);
}
'@
$game = Get-Process -Id $GameProcessId
if ($Close) {
    if (-not $game.CloseMainWindow()) { throw 'Owned game window rejected its close request' }
    exit 0
}
$deadline = [DateTime]::UtcNow.AddSeconds(30)
while ([DateTime]::UtcNow -lt $deadline) {
    $game.Refresh()
    if ($game.HasExited) { throw 'Game exited before opening its window' }
    $handle = $game.MainWindowHandle
    $rect = New-Object GameWindow+Rect
    if ($handle -ne [IntPtr]::Zero -and [GameWindow]::IsWindowVisible($handle) -and
        [GameWindow]::GetWindowRect($handle, [ref]$rect)) {
        $width = $rect.Right - $rect.Left
        $height = $rect.Bottom - $rect.Top
        if ($width -ge 480 -and $height -ge 320) {
            @{ pid = $GameProcessId; width = $width; height = $height; visible = $true } |
                ConvertTo-Json -Compress
            exit 0
        }
    }
    Start-Sleep -Milliseconds 100
}
throw 'No visible game window within 30 seconds'
