use scripting additions

on run argv
	if (count of argv) is 0 then
		return "usage: osascript DaisDesk.applescript <processName|healthcheck|activate|clickButton|pressShortcut|typeText|screenshotWindow|screenshotActiveScreen|screenshotDisplay|windowBounds> [args]"
	end if
	set commandName to item 1 of argv
	if commandName is "processName" then
		return my findDeskProcess()
	else if commandName is "healthcheck" then
		return my healthcheck()
	else if commandName is "activate" then
		return my activateDesk()
	else if commandName is "clickButton" then
		return my clickButton(item 2 of argv)
	else if commandName is "pressShortcut" then
		set keyName to item 2 of argv
		set modifierName to ""
		if (count of argv) > 2 then set modifierName to item 3 of argv
		return my pressShortcut(keyName, modifierName)
	else if commandName is "typeText" then
		return my typeText(item 2 of argv)
	else if commandName is "screenshotWindow" then
		return my screenshotWindow(item 2 of argv)
	else if commandName is "screenshotActiveScreen" then
		return my screenshotWindow(item 2 of argv)
	else if commandName is "screenshotDisplay" then
		return my screenshotDisplay(item 2 of argv)
	else if commandName is "windowBounds" then
		return my windowBounds()
	else
		error "unknown Dais Desk automation command: " & commandName
	end if
end run

on findDeskProcess()
	set probeScript to "tell application \"System Events\" to return name of every process whose name is \"Dais Desk\" or name is \"DaisDesk\" or name is \"dais-desk\" or name is \"dais-desk-bin\" or name is \"DaisDeskLauncher\""
	try
		set matchesText to do shell script "/usr/bin/osascript -e " & quoted form of probeScript
		if matchesText is not "" then return first paragraph of matchesText
	end try
	error "Dais Desk is not running. Start the app, then retry the automation command."
end findDeskProcess

on healthcheck()
	set processName to my findDeskProcess()
	tell application "System Events"
		tell process processName
			if (count of windows) is 0 then return "ready: " & processName & " running; no AX windows exposed, use screenshotActiveScreen fallback"
			set windowName to name of window 1
		end tell
	end tell
	return "ready: " & processName & " window=" & windowName
end healthcheck

on activateDesk()
	set processName to my findDeskProcess()
	tell application "System Events"
		tell process processName
			set frontmost to true
		end tell
	end tell
	delay 0.2
	return "activated " & processName
end activateDesk

on frontDeskWindow()
	set processName to my findDeskProcess()
	tell application "System Events"
		tell process processName
			if (count of windows) is 0 then error "Dais Desk has no visible windows"
			return window 1
		end tell
	end tell
end frontDeskWindow

on clickButton(buttonLabel)
	my activateDesk()
	set processName to my findDeskProcess()
	tell application "System Events"
		tell process processName
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
	my activateDesk()
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
	my activateDesk()
	tell application "System Events" to keystroke value
	return "typed"
end typeText

on screenshotWindow(outputPath)
	my activateDesk()
	set processName to my findDeskProcess()
	tell application "System Events"
		tell process processName
			if (count of windows) is 0 then error "Dais Desk has no visible windows"
			try
				set unixWindowId to value of attribute "AXWindowNumber" of window 1
			on error
				set unixWindowId to ""
			end try
		end tell
	end tell
	if unixWindowId is "" then
		do shell script "/usr/sbin/screencapture -x " & quoted form of POSIX path of outputPath
	else
		do shell script "/usr/sbin/screencapture -x -l " & unixWindowId & " " & quoted form of POSIX path of outputPath
	end if
	return outputPath
end screenshotWindow

on screenshotDisplay(outputPath)
	my activateDesk()
	do shell script "/usr/sbin/screencapture -x " & quoted form of POSIX path of outputPath
	return outputPath
end screenshotDisplay

on windowBounds()
	my activateDesk()
	set processName to my findDeskProcess()
	tell application "System Events"
		tell process processName
			if (count of windows) is 0 then error "Dais Desk has no visible windows"
			set windowPosition to position of window 1
			set windowSize to size of window 1
		end tell
	end tell
	return "x=" & item 1 of windowPosition & " y=" & item 2 of windowPosition & " width=" & item 1 of windowSize & " height=" & item 2 of windowSize
end windowBounds
