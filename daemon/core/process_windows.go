//go:build windows

package core

import (
	"fmt"
	"os"
	"os/exec"
	"strconv"
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
	return strings.Contains(string(out), strconv.Itoa(proc.Pid))
}
