//go:build linux

package main

// ensureElevated is a no-op on Linux.
// Elevation (sudo/pkexec) is requested per-operation when needed.
func ensureElevated() {}
