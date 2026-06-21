$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class DaisDeskNative {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

function Get-DaisDeskProcess {
    $process = Get-Process | Where-Object {
        $_.MainWindowTitle -eq "Dais Desk" -or $_.ProcessName -like "dais-desk*"
    } | Select-Object -First 1
    if (-not $process) {
        throw "Dais Desk is not running or has no visible main window."
    }
    return $process
}

function Show-DaisDeskWindow {
    $process = Get-DaisDeskProcess
    [DaisDeskNative]::ShowWindow($process.MainWindowHandle, 5) | Out-Null
    [DaisDeskNative]::SetForegroundWindow($process.MainWindowHandle) | Out-Null
    Start-Sleep -Milliseconds 200
    return $process
}

function Send-DaisDeskKeys {
    param([Parameter(Mandatory=$true)][string]$Keys)
    Show-DaisDeskWindow | Out-Null
    [System.Windows.Forms.SendKeys]::SendWait($Keys)
}

function Invoke-DaisDeskButton {
    param([Parameter(Mandatory=$true)][string]$Name)
    $process = Show-DaisDeskWindow
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
    $nameCondition = New-Object System.Windows.Automation.PropertyCondition `
        ([System.Windows.Automation.AutomationElement]::NameProperty, $Name)
    $buttonCondition = New-Object System.Windows.Automation.PropertyCondition `
        ([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
    $condition = New-Object System.Windows.Automation.AndCondition $nameCondition, $buttonCondition
    $button = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if (-not $button) {
        throw "Dais Desk button not found: $Name"
    }
    $pattern = $button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    $pattern.Invoke()
}

function Save-DaisDeskWindowScreenshot {
    param([Parameter(Mandatory=$true)][string]$Path)
    $process = Show-DaisDeskWindow
    $rect = New-Object DaisDeskNative+RECT
    if (-not [DaisDeskNative]::GetWindowRect($process.MainWindowHandle, [ref]$rect)) {
        throw "Could not read Dais Desk window bounds."
    }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) {
        throw "Dais Desk window has invalid bounds."
    }
    $bitmap = New-Object System.Drawing.Bitmap $width, $height
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
        $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
    return $Path
}
