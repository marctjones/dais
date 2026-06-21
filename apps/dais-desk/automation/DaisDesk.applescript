use scripting additions

property appName : "Dais Desk"

on run argv
	if (count of argv) is 0 then
		return "usage: osascript DaisDesk.applescript <activate|clickButton|pressShortcut|typeText|screenshotWindow> [args]"
	end if
	set commandName to item 1 of argv
	if commandName is "activate" then
		return activateDesk()
	else if commandName is "clickButton" then
		return clickButton(item 2 of argv)
	else if commandName is "pressShortcut" then
		set keyName to item 2 of argv
		set modifierName to ""
		if (count of argv) > 2 then set modifierName to item 3 of argv
		return pressShortcut(keyName, modifierName)
	else if commandName is "typeText" then
		return typeText(item 2 of argv)
	else if commandName is "screenshotWindow" then
		return screenshotWindow(item 2 of argv)
	else
		error "unknown Dais Desk automation command: " & commandName
	end if
end run

on activateDesk()
	tell application appName to activate
	delay 0.2
	return "activated"
end activateDesk

on frontDeskWindow()
	tell application "System Events"
		if not (exists process appName) then error appName & " is not running"
		tell process appName
			if (count of windows) is 0 then error appName & " has no visible windows"
			return window 1
		end tell
	end tell
end frontDeskWindow

on clickButton(buttonLabel)
	activateDesk()
	tell application "System Events"
		tell process appName
			set targetWindow to window 1
			if exists button buttonLabel of targetWindow then
				click button buttonLabel of targetWindow
				return "clicked " & buttonLabel
			end if
			set matches to every UI element of targetWindow whose role description is "button" and name is buttonLabel
			if (count of matches) > 0 then
				click item 1 of matches
				return "clicked " & buttonLabel
			end if
		end tell
	end tell
	error "button not found: " & buttonLabel
end clickButton

on pressShortcut(keyName, modifierName)
	activateDesk()
	tell application "System Events"
		if modifierName is "command" then
			keystroke keyName using command down
		else if modifierName is "control" then
			keystroke keyName using control down
		else if modifierName is "option" then
			keystroke keyName using option down
		else if modifierName is "shift" then
			keystroke keyName using shift down
		else
			keystroke keyName
		end if
	end tell
	return "pressed " & modifierName & " " & keyName
end pressShortcut

on typeText(value)
	activateDesk()
	tell application "System Events" to keystroke value
	return "typed"
end typeText

on screenshotWindow(outputPath)
	activateDesk()
	tell application "System Events"
		tell process appName
			if (count of windows) is 0 then error appName & " has no visible windows"
			set unixWindowId to value of attribute "AXWindowNumber" of window 1
		end tell
	end tell
	do shell script "/usr/sbin/screencapture -x -l " & unixWindowId & " " & quoted form of POSIX path of outputPath
	return outputPath
end screenshotWindow
