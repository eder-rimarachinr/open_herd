//go:build windows

package main

import (
	"fmt"
	"os"
	"os/exec"
	"strings"

	"golang.org/x/sys/windows"
)

// ensureElevated re-launches the daemon as Administrator if the current process
// is not already elevated. When spawned as a Tauri sidecar the parent app's
// UAC token is inherited, so this is a no-op in that case.
func ensureElevated() {
	if isElevated() {
		return
	}

	// Not elevated — re-launch ourselves with the RunAs verb (triggers UAC).
	exe, err := os.Executable()
	if err != nil {
		fmt.Fprintln(os.Stderr, "phpenv: could not determine executable path:", err)
		return
	}

	args := os.Args[1:]
	quotedArgs := make([]string, len(args))
	for i, a := range args {
		quotedArgs[i] = fmt.Sprintf("'%s'", strings.ReplaceAll(a, "'", "''"))
	}

	var psCmd string
	if len(quotedArgs) > 0 {
		psCmd = fmt.Sprintf(`Start-Process '%s' -ArgumentList %s -Verb RunAs`,
			exe, strings.Join(quotedArgs, ","))
	} else {
		psCmd = fmt.Sprintf(`Start-Process '%s' -Verb RunAs`, exe)
	}

	cmd := exec.Command("powershell", "-NonInteractive", "-Command", psCmd)
	if err := cmd.Start(); err != nil {
		fmt.Fprintln(os.Stderr, "phpenv: elevation failed:", err)
		return
	}

	// Original (non-elevated) process exits immediately.
	os.Exit(0)
}

// isElevated reports whether the current process has an elevated Windows token.
func isElevated() bool {
	token := windows.GetCurrentProcessToken()
	return token.IsElevated()
}
