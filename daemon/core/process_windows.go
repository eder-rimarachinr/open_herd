//go:build windows

package core

import (
	"fmt"
	"os"
	"os/exec"
	"strings"
)

// isProcessRunning checks if a process is alive on Windows using tasklist.
// Signal(nil) is not supported on Windows, so we query the OS process table.
func isProcessRunning(proc *os.Process) bool {
	if proc == nil {
		return false
	}
	out, err := exec.Command(
		"tasklist",
		"/FI", fmt.Sprintf("PID eq %d", proc.Pid),
		"/NH", "/FO", "CSV",
	).Output()
	if err != nil {
		return false
	}
	// Tasklist CSV format starts with a quote (e.g. "nginx.exe","123",...).
	// If the process is not found, it returns an info message without leading quotes.
	return strings.HasPrefix(strings.TrimSpace(string(out)), "\"")
}
