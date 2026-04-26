package core

import "os"

// isProcessRunning sends signal 0 to check if a process is alive.
// On Windows this is a best-effort stub.
func isProcessRunning(proc *os.Process) bool {
	if proc == nil {
		return false
	}
	err := proc.Signal(os.Signal(nil))
	return err == nil
}
