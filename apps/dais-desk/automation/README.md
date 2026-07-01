# Dais Desk Automation

Dais Desk exposes GUI automation through stable accessibility labels and thin
platform wrappers. The wrappers are intentionally small so smoke tests can drive
the real native window without coupling test logic to Slint internals.

## macOS AppleScript

Load the helper from AppleScript or call handlers with `osascript`:

```sh
osascript apps/dais-desk/automation/DaisDesk.applescript healthcheck
osascript apps/dais-desk/automation/DaisDesk.applescript screenshotWindow /tmp/dais-desk.png
osascript apps/dais-desk/automation/DaisDesk.applescript screenshotActiveScreen /tmp/dais-desk-active.png
osascript apps/dais-desk/automation/DaisDesk.applescript screenshotDisplay /tmp/dais-desk-display.png
osascript apps/dais-desk/automation/DaisDesk.applescript clickButton Refresh
osascript apps/dais-desk/automation/DaisDesk.applescript pressShortcut 1 command
```

Handlers:

- `processName()` returns the process name used for automation. It accepts a
  packaged app (`Dais Desk`), debug binary (`dais-desk`), or temporary bundle
  launcher (`DaisDeskLauncher`).
- `healthcheck()` verifies that the app is running and has a visible window.
- `activateDesk()` brings Dais Desk to the front.
- `clickButton(label)` clicks the first visible button with that label.
- `pressShortcut(keyName, modifierName)` sends a keyboard shortcut.
- `typeText(value)` types into the focused control.
- `screenshotWindow(outputPath)` captures only the front Dais Desk window.
- `screenshotActiveScreen(outputPath)` is an alias for the front Dais Desk
  window capture, intended for test scripts.
- `screenshotDisplay(outputPath)` captures the full active display after
  bringing Dais Desk forward.
- `windowBounds()` reports the front Dais Desk window bounds.

## Visual UX Review

Build a repeatable local macOS app bundle for live automation:

```sh
scripts/package-dais-desk-macos.sh
open -n "apps/dais-desk/target/macos/Dais Desk.app"
osascript apps/dais-desk/automation/DaisDesk.applescript healthcheck
```

The packaged debug app launches through a small wrapper that defaults
`SLINT_BACKEND=winit`. On macOS this keeps the Slint window registered as an
ordinary visible application process for System Events and screenshot capture.

Run the visual review script to regenerate every modeled screen screenshot and
produce a macOS-oriented UX recommendations report:

```sh
scripts/review-dais-desk-visual-ux.sh
```

To include a live native-window capture from a freshly packaged macOS app:

```sh
LAUNCH_PACKAGED_APP=1 scripts/review-dais-desk-visual-ux.sh
```

The script writes screenshots and `DAIS_DESK_VISUAL_UX_REVIEW.md` under
`tmp/dais-desk-visual-review-<timestamp>/`. It uses the Slint visual smoke test
for deterministic per-screen captures and, when a visible Dais Desk process is
running, also captures the active native window through AppleScript.

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
