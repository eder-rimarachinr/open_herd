//go:build !windows

package core

import "os"

// isProcessRunning sends signal 0 to check if a process is alive (Linux/macOS).
func isProcessRunning(proc *os.Process) bool {
	if proc == nil {
		return false
	}
	err := proc.Signal(os.Signal(nil))
	return err == nil
}
