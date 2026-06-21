# Dais Desk Automation

Dais Desk exposes GUI automation through stable accessibility labels and thin
platform wrappers. The wrappers are intentionally small so smoke tests can drive
the real native window without coupling test logic to Slint internals.

## macOS AppleScript

Load the helper from AppleScript or call handlers with `osascript`:

```sh
osascript apps/dais-desk/automation/DaisDesk.applescript screenshotWindow /tmp/dais-desk.png
osascript apps/dais-desk/automation/DaisDesk.applescript clickButton Refresh
osascript apps/dais-desk/automation/DaisDesk.applescript pressShortcut 1 command
```

Handlers:

- `activateDesk()` brings Dais Desk to the front.
- `clickButton(label)` clicks the first visible button with that label.
- `pressShortcut(keyName, modifierName)` sends a keyboard shortcut.
- `typeText(value)` types into the focused control.
- `screenshotWindow(outputPath)` captures only the front Dais Desk window.

## Windows PowerShell

Import the module in a PowerShell test script:

```powershell
. .\apps\dais-desk\automation\DaisDesk.ps1
Show-DaisDeskWindow
Invoke-DaisDeskButton -Name "Refresh"
Save-DaisDeskWindowScreenshot -Path "$env:TEMP\dais-desk.png"
```

Functions:

- `Show-DaisDeskWindow`
- `Invoke-DaisDeskButton`
- `Send-DaisDeskKeys`
- `Save-DaisDeskWindowScreenshot`

Both wrappers target the same visible UI labels. Add new labels to the Slint UI
when adding workflows that should be scriptable.
